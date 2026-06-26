//! Raw JSON output formatting (jq-compatible).
//!
//! [`format_raw`] takes filter results and produces a JSON string in either
//! pretty-printed or compact form. Scalar results are printed as-is (not
//! wrapped in an array), and single-element arrays are unwrapped to their
//! sole value.

use serde_json::Value;

/// Format a slice of filter results as a JSON string.
///
/// When `compact` is `true`, no whitespace is added. Otherwise the output
/// is pretty-printed with 2-space indentation.
///
/// # Output rules
///
/// | Results                | Output                          |
/// |------------------------|---------------------------------|
/// | `[]`                   | `null`                          |
/// | `[v]` (single element) | `v` (unwrapped)                 |
/// | `[v1, v2, ...]`        | `[v1, v2, ...]` (JSON array)    |
pub fn format_raw(results: &[Value], compact: bool) -> String {
    match results.len() {
        0 => String::from("null"),
        1 => {
            let val = &results[0];
            if compact {
                serde_json::to_string(val).unwrap_or_else(|_| String::from("null"))
            } else {
                serde_json::to_string_pretty(val).unwrap_or_else(|_| String::from("null"))
            }
        }
        _ => {
            let arr = Value::Array(results.to_vec());
            if compact {
                serde_json::to_string(&arr).unwrap_or_else(|_| String::from("[]"))
            } else {
                serde_json::to_string_pretty(&arr).unwrap_or_else(|_| String::from("[]"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_empty_results() {
        let result = format_raw(&[], false);
        assert_eq!(result, "null");
    }

    #[test]
    fn test_empty_results_compact() {
        let result = format_raw(&[], true);
        assert_eq!(result, "null");
    }

    #[test]
    fn test_single_scalar() {
        let result = format_raw(&[json!(42)], false);
        assert_eq!(result.trim(), "42");
    }

    #[test]
    fn test_single_scalar_compact() {
        let result = format_raw(&[json!(42)], true);
        assert_eq!(result, "42");
    }

    #[test]
    fn test_single_object() {
        let result = format_raw(&[json!({"a": 1})], false);
        let parsed: Value = serde_json::from_str(&result).expect("valid JSON");
        assert_eq!(parsed, json!({"a": 1}));
    }

    #[test]
    fn test_single_object_compact() {
        let result = format_raw(&[json!({"a": 1})], true);
        let parsed: Value = serde_json::from_str(&result).expect("valid JSON");
        assert_eq!(parsed, json!({"a": 1}));
    }

    #[test]
    fn test_multiple_results() {
        let results = vec![json!(1), json!(2), json!(3)];
        let result = format_raw(&results, false);
        let parsed: Value = serde_json::from_str(&result).expect("valid JSON");
        assert_eq!(parsed, json!([1, 2, 3]));
    }

    #[test]
    fn test_multiple_results_compact() {
        let results = vec![json!(1), json!(2), json!(3)];
        let result = format_raw(&results, true);
        let parsed: Value = serde_json::from_str(&result).expect("valid JSON");
        assert_eq!(parsed, json!([1, 2, 3]));
    }

    #[test]
    fn test_multiple_objects() {
        let results = vec![json!({"id": 1}), json!({"id": 2})];
        let result = format_raw(&results, false);
        let parsed: Value = serde_json::from_str(&result).expect("valid JSON");
        assert_eq!(parsed, json!([{"id": 1}, {"id": 2}]));
    }

    #[test]
    fn test_null_output() {
        let result = format_raw(&[json!(null)], false);
        assert_eq!(result.trim(), "null");
    }

    #[test]
    fn test_unicode_output() {
        let result = format_raw(&[json!("こんにちは")], false);
        let parsed: Value = serde_json::from_str(&result).expect("valid JSON");
        assert_eq!(parsed, json!("こんにちは"));
    }

    #[test]
    fn test_string_output() {
        let result = format_raw(&[json!("hello")], false);
        let parsed: Value = serde_json::from_str(&result).expect("valid JSON");
        assert_eq!(parsed, json!("hello"));
    }

    #[test]
    fn test_boolean_output() {
        let result = format_raw(&[json!(true)], false);
        assert_eq!(result.trim(), "true");
    }

    #[test]
    fn test_pretty_has_newlines() {
        let results = vec![json!({"a": 1, "b": 2})];
        let result = format_raw(&results, false);
        assert!(
            result.contains('\n'),
            "pretty output should contain newlines, got: {result}"
        );
    }

    #[test]
    fn test_compact_no_newlines() {
        let results = vec![json!({"a": 1, "b": 2})];
        let result = format_raw(&results, true);
        assert!(
            !result.contains('\n'),
            "compact output should not contain newlines, got: {result}"
        );
    }
}
