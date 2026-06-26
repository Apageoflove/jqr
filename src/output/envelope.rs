//! Schema-first JSON envelope — the default output mode.
//!
//! [`Envelope`] wraps filter results with schema, sample, and metadata
//! so LLM agents can understand the structure without consuming the full
//! dataset.  Scalar results (length queries, aggregation) use a compact
//! `{schema, value, tokens_used}` shape instead of the full sample
//! envelope.

use serde_json::{json, Value};

use crate::config::SchemaFormat;
use crate::output::truncate::TruncatedOutput;
use crate::schema::SchemaNode;

/// The schema-first output envelope.
///
/// For array / multi-result outputs the envelope contains `schema`,
/// `sample`, `total`, `truncated`, `sample_size`, and `tokens_used`.
///
/// For scalar outputs (single non-array result, e.g. a `length` query)
/// the envelope uses `{schema, value, tokens_used}` instead.
#[derive(Debug, Clone)]
pub struct Envelope {
    /// Compact JSON schema describing the result shape.
    pub schema: Value,
    /// The sample data (subset of results, or the scalar value).
    pub sample: Value,
    /// Total number of results before truncation.
    pub total: usize,
    /// Whether results were truncated to fit the token budget.
    pub truncated: bool,
    /// The requested sample size.
    pub sample_size: usize,
    /// Actual tokens consumed by the serialized sample.
    pub tokens_used: usize,
    /// Whether this is a scalar result (uses `value` key instead of `sample`).
    is_scalar: bool,
}

impl Envelope {
    /// Build an envelope from filter results, inferred schema, and
    /// truncation metadata.
    ///
    /// When `results` contains exactly one element and that element is
    /// not a JSON array, the envelope uses the scalar shape
    /// `{schema, value, tokens_used}`.  Otherwise the full sample
    /// shape is used.
    pub fn from_results(
        results: &[Value],
        schema: SchemaNode,
        truncation: TruncatedOutput,
        format: SchemaFormat,
    ) -> Self {
        let is_scalar = results.len() == 1 && !results[0].is_array();

        let schema_json = match format {
            SchemaFormat::JsonSchema => schema.to_json_schema(),
            SchemaFormat::Typescript => {
                let ts = schema.to_typescript("Root");
                Value::String(ts)
            }
            SchemaFormat::Zod => {
                let zod = schema.to_zod("Root");
                Value::String(zod)
            }
            SchemaFormat::Pydantic => {
                let py = schema.to_pydantic("Root");
                Value::String(py)
            }
        };

        let sample = if is_scalar {
            results[0].clone()
        } else {
            Value::Array(truncation.sample.clone())
        };

        Envelope {
            schema: schema_json,
            sample,
            total: truncation.total,
            truncated: truncation.truncated,
            sample_size: truncation.sample_size,
            tokens_used: truncation.tokens_used,
            is_scalar,
        }
    }

    /// Serialise the envelope to a JSON string.
    ///
    /// When `pretty` is `true` the output is pretty-printed with 2-space
    /// indentation.  Otherwise it is compact (no extra whitespace).
    ///
    /// Returns `"null"` if serialisation fails (should never happen for
    /// valid schema + sample values).
    pub fn to_json(&self, pretty: bool, schema_only: bool) -> String {
        if schema_only {
            let value = json!({"schema": self.schema});
            if pretty {
                return serde_json::to_string_pretty(&value).unwrap_or_else(|_| String::from("null"));
            }
            return serde_json::to_string(&value).unwrap_or_else(|_| String::from("null"));
        }

        let value = if self.is_scalar {
            json!({
                "schema": self.schema,
                "value": self.sample,
                "tokens_used": self.tokens_used,
            })
        } else {
            json!({
                "schema": self.schema,
                "sample": self.sample,
                "total": self.total,
                "truncated": self.truncated,
                "sample_size": self.sample_size,
                "tokens_used": self.tokens_used,
            })
        };

        if pretty {
            serde_json::to_string_pretty(&value).unwrap_or_else(|_| String::from("null"))
        } else {
            serde_json::to_string(&value).unwrap_or_else(|_| String::from("null"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::truncate::TruncatedOutput;
    use crate::schema::SchemaInferrer;
    use serde_json::json;

    fn make_truncation(sample: Vec<Value>, total: usize, truncated: bool) -> TruncatedOutput {
        TruncatedOutput {
            sample,
            total,
            truncated,
            sample_size: 10,
            tokens_used: 42,
        }
    }

    fn infer_schema(value: &Value) -> SchemaNode {
        SchemaInferrer::new(32).infer(value)
    }

    // ------------------------------------------------------------------
    // Full sample envelope (array results)
    // ------------------------------------------------------------------

    #[test]
    fn test_array_results_full_envelope() {
        let results = vec![json!({"id": 1}), json!({"id": 2}), json!({"id": 3})];
        let schema = infer_schema(&Value::Array(results.clone()));
        let trunc = make_truncation(results.clone(), 3, false);

        let envelope = Envelope::from_results(&results, schema, trunc, SchemaFormat::JsonSchema);
        let json_str = envelope.to_json(false, false);
        let parsed: Value = serde_json::from_str(&json_str).expect("valid JSON");

        assert_eq!(parsed["total"], json!(3));
        assert_eq!(parsed["truncated"], json!(false));
        assert_eq!(parsed["sample_size"], json!(10));
        assert_eq!(parsed["tokens_used"], json!(42));
        assert!(parsed["schema"].is_object() || parsed["schema"].is_array());
        assert!(parsed["sample"].is_array());
        assert_eq!(parsed["sample"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn test_truncated_envelope() {
        let all_results: Vec<Value> = (0..100).map(|i| json!({"id": i})).collect();
        let schema = infer_schema(&Value::Array(all_results.clone()));
        let trunc = TruncatedOutput {
            sample: all_results[..5].to_vec(),
            total: 100,
            truncated: true,
            sample_size: 5,
            tokens_used: 150,
        };

        let envelope = Envelope::from_results(&all_results, schema, trunc, SchemaFormat::JsonSchema);
        let json_str = envelope.to_json(false, false);
        let parsed: Value = serde_json::from_str(&json_str).expect("valid JSON");

        assert_eq!(parsed["total"], json!(100));
        assert_eq!(parsed["truncated"], json!(true));
        assert_eq!(parsed["sample"].as_array().unwrap().len(), 5);
    }

    #[test]
    fn test_empty_results() {
        let results: Vec<Value> = vec![];
        let schema = infer_schema(&Value::Array(vec![]));
        let trunc = make_truncation(vec![], 0, false);

        let envelope = Envelope::from_results(&results, schema, trunc, SchemaFormat::JsonSchema);
        let json_str = envelope.to_json(false, false);
        let parsed: Value = serde_json::from_str(&json_str).expect("valid JSON");

        assert_eq!(parsed["total"], json!(0));
        assert_eq!(parsed["truncated"], json!(false));
        assert!(parsed["sample"].as_array().unwrap().is_empty());
    }

    // ------------------------------------------------------------------
    // Scalar envelope (single non-array result)
    // ------------------------------------------------------------------

    #[test]
    fn test_scalar_result_value_envelope() {
        let results = vec![json!(42)];
        let schema = infer_schema(&json!(42));
        let trunc = make_truncation(vec![json!(42)], 1, false);

        let envelope = Envelope::from_results(&results, schema, trunc, SchemaFormat::JsonSchema);
        let json_str = envelope.to_json(false, false);
        let parsed: Value = serde_json::from_str(&json_str).expect("valid JSON");

        // Scalar shape: {schema, value, tokens_used}
        assert!(parsed.get("value").is_some(), "scalar envelope must have 'value' key");
        assert!(parsed.get("sample").is_none(), "scalar envelope must NOT have 'sample' key");
        assert!(parsed.get("total").is_none(), "scalar envelope must NOT have 'total' key");
        assert_eq!(parsed["value"], json!(42));
        assert_eq!(parsed["tokens_used"], json!(42));
    }

    #[test]
    fn test_scalar_null_result() {
        let results = vec![json!(null)];
        let schema = infer_schema(&json!(null));
        let trunc = make_truncation(vec![json!(null)], 1, false);

        let envelope = Envelope::from_results(&results, schema, trunc, SchemaFormat::JsonSchema);
        let json_str = envelope.to_json(false, false);
        let parsed: Value = serde_json::from_str(&json_str).expect("valid JSON");

        assert_eq!(parsed["value"], json!(null));
    }

    #[test]
    fn test_scalar_string_result() {
        let results = vec![json!("hello")];
        let schema = infer_schema(&json!("hello"));
        let trunc = make_truncation(vec![json!("hello")], 1, false);

        let envelope = Envelope::from_results(&results, schema, trunc, SchemaFormat::JsonSchema);
        let json_str = envelope.to_json(false, false);
        let parsed: Value = serde_json::from_str(&json_str).expect("valid JSON");

        assert_eq!(parsed["value"], json!("hello"));
    }

    #[test]
    fn test_scalar_boolean_result() {
        let results = vec![json!(true)];
        let schema = infer_schema(&json!(true));
        let trunc = make_truncation(vec![json!(true)], 1, false);

        let envelope = Envelope::from_results(&results, schema, trunc, SchemaFormat::JsonSchema);
        let json_str = envelope.to_json(false, false);
        let parsed: Value = serde_json::from_str(&json_str).expect("valid JSON");

        assert_eq!(parsed["value"], json!(true));
    }

    // ------------------------------------------------------------------
    // Single-element array → still full envelope (not scalar)
    // ------------------------------------------------------------------

    #[test]
    fn test_single_array_result_is_full_envelope() {
        // A single result that IS an array → full envelope, not scalar
        let results = vec![json!([1, 2, 3])];
        let schema = infer_schema(&json!([1, 2, 3]));
        let trunc = make_truncation(vec![json!([1, 2, 3])], 1, false);

        let envelope = Envelope::from_results(&results, schema, trunc, SchemaFormat::JsonSchema);
        let json_str = envelope.to_json(false, false);
        let parsed: Value = serde_json::from_str(&json_str).expect("valid JSON");

        // Should use full envelope shape because the result is an array
        assert!(parsed.get("sample").is_some(), "array result should use 'sample' key");
        assert!(parsed.get("total").is_some(), "array result should have 'total' key");
    }

    // ------------------------------------------------------------------
    // Pretty vs compact output
    // ------------------------------------------------------------------

    #[test]
    fn test_pretty_output_has_newlines() {
        let results = vec![json!({"name": "Alice"})];
        let schema = infer_schema(&json!({"name": "Alice"}));
        let trunc = make_truncation(vec![json!({"name": "Alice"})], 1, false);

        let envelope = Envelope::from_results(&results, schema, trunc, SchemaFormat::JsonSchema);
        let pretty = envelope.to_json(true, false);
        let compact = envelope.to_json(false, false);

        assert!(pretty.contains('\n'), "pretty output should contain newlines");
        assert!(!compact.contains('\n'), "compact output should not contain newlines");
    }

    // ------------------------------------------------------------------
    // Schema field is present and valid
    // ------------------------------------------------------------------

    #[test]
    fn test_schema_field_is_compact_json() {
        let results = vec![json!({"id": 1, "name": "test"})];
        let schema = infer_schema(&json!({"id": 1, "name": "test"}));
        let trunc = make_truncation(vec![json!({"id": 1, "name": "test"})], 1, false);

        let envelope = Envelope::from_results(&results, schema, trunc, SchemaFormat::JsonSchema);
        let json_str = envelope.to_json(false, false);
        let parsed: Value = serde_json::from_str(&json_str).expect("valid JSON");

        let schema_val = &parsed["schema"];
        assert!(schema_val.is_object(), "schema should be an object, got {schema_val}");
        assert_eq!(schema_val["type"], json!("object"));
        assert_eq!(schema_val["properties"]["id"]["type"], json!("integer"));
        assert_eq!(schema_val["properties"]["name"]["type"], json!("string"));
    }

    // ------------------------------------------------------------------
    // tokens_used consistency
    // ------------------------------------------------------------------

    #[test]
    fn test_tokens_used_matches_truncation() {
        let results = vec![json!({"a": 1}), json!({"b": 2})];
        let schema = infer_schema(&Value::Array(results.clone()));
        let trunc = TruncatedOutput {
            sample: results.clone(),
            total: 2,
            truncated: false,
            sample_size: 10,
            tokens_used: 999,
        };

        let envelope = Envelope::from_results(&results, schema, trunc, SchemaFormat::JsonSchema);
        let json_str = envelope.to_json(false, false);
        let parsed: Value = serde_json::from_str(&json_str).expect("valid JSON");

        assert_eq!(parsed["tokens_used"], json!(999));
    }

    // ------------------------------------------------------------------
    // Schema-only output
    // ------------------------------------------------------------------

    #[test]
    fn test_schema_only_scalar() {
        let results = vec![json!(42)];
        let schema = infer_schema(&json!(42));
        let trunc = make_truncation(vec![json!(42)], 1, false);

        let envelope = Envelope::from_results(&results, schema, trunc, SchemaFormat::JsonSchema);
        let json_str = envelope.to_json(false, true);
        let parsed: Value = serde_json::from_str(&json_str).expect("valid JSON");

        assert!(parsed.get("schema").is_some(), "schema_only must have 'schema' key");
        assert!(parsed.get("value").is_none(), "schema_only must NOT have 'value' key");
        assert!(parsed.get("sample").is_none(), "schema_only must NOT have 'sample' key");
        assert!(parsed.get("total").is_none(), "schema_only must NOT have 'total' key");
    }

    #[test]
    fn test_schema_only_array() {
        let results = vec![json!({"id": 1}), json!({"id": 2})];
        let schema = infer_schema(&Value::Array(results.clone()));
        let trunc = make_truncation(results.clone(), 2, false);

        let envelope = Envelope::from_results(&results, schema, trunc, SchemaFormat::JsonSchema);
        let json_str = envelope.to_json(false, true);
        let parsed: Value = serde_json::from_str(&json_str).expect("valid JSON");

        assert!(parsed.get("schema").is_some(), "schema_only must have 'schema' key");
        assert!(parsed.get("sample").is_none(), "schema_only must NOT have 'sample' key");
        assert!(parsed.get("total").is_none(), "schema_only must NOT have 'total' key");
    }
}
