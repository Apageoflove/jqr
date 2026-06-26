use std::collections::HashMap;

use serde_json::{json, Map, Value};

/// A node in the inferred JSON schema tree.
///
/// Each variant represents a JSON type or a composition of types.
/// The schema is built by walking a `serde_json::Value` recursively
/// and tracking the types observed at each path.
#[derive(Debug, Clone, PartialEq)]
pub enum SchemaNode {
    /// JSON null.
    Null,
    /// JSON boolean (`true` / `false`).
    Boolean,
    /// JSON integer number (no decimal point, no exponent).
    Integer,
    /// JSON floating-point number (has decimal point or exponent).
    Float,
    /// JSON string.
    String,
    /// Homogeneous JSON array — every element shares the inner schema.
    Array(Box<SchemaNode>),
    /// JSON object with known property names and their schemas.
    Object(HashMap<String, SchemaNode>),
    /// Mixed type — the value can be one of several schemas.
    OneOf(Vec<SchemaNode>),
    /// Unknown / any type (used when `max_depth` is reached).
    Any,
}

/// Recursively walks a `serde_json::Value` and builds a [`SchemaNode`] tree.
///
/// The `max_depth` parameter caps recursion depth to prevent stack
/// overflow on deeply nested inputs.  When the depth limit is reached
/// the inferrer returns [`SchemaNode::Any`].
pub struct SchemaInferrer {
    max_depth: usize,
}

impl SchemaInferrer {
    /// Creates a new inferrer with the given maximum recursion depth.
    pub fn new(max_depth: usize) -> Self {
        SchemaInferrer { max_depth }
    }

    /// Infers the schema for the given JSON value.
    ///
    /// This is the public entry point.  It delegates to the
    /// depth-tracking private method starting at depth 0.
    pub fn infer(&self, value: &Value) -> SchemaNode {
        self.infer_with_depth(value, 0)
    }

    /// Recursive schema inference with depth tracking.
    ///
    /// When `depth >= self.max_depth` the function short-circuits and
    /// returns [`SchemaNode::Any`] to bound stack usage.
    fn infer_with_depth(&self, value: &Value, depth: usize) -> SchemaNode {
        if depth >= self.max_depth {
            return SchemaNode::Any;
        }

        match value {
            Value::Null => SchemaNode::Null,
            Value::Bool(_) => SchemaNode::Boolean,
            Value::Number(n) => {
                if n.is_f64() {
                    SchemaNode::Float
                } else {
                    SchemaNode::Integer
                }
            }
            Value::String(_) => SchemaNode::String,
            Value::Array(arr) => {
                if arr.is_empty() {
                    return SchemaNode::Array(Box::new(SchemaNode::Any));
                }

                let schemas: Vec<SchemaNode> = arr
                    .iter()
                    .map(|v| self.infer_with_depth(v, depth + 1))
                    .collect();

                let merged = merge_schemas(&schemas);
                SchemaNode::Array(Box::new(merged))
            }
            Value::Object(obj) => {
                if obj.is_empty() {
                    return SchemaNode::Object(HashMap::new());
                }

                let mut properties: HashMap<String, SchemaNode> = HashMap::new();
                for (key, value) in obj {
                    let schema = self.infer_with_depth(value, depth + 1);
                    properties.insert(key.clone(), schema);
                }

                SchemaNode::Object(properties)
            }
        }
    }
}

/// Merges multiple schemas into a single schema.
///
/// # Merging strategy
///
/// 1. Deduplicate the input schemas.
/// 2. If only one unique schema remains, return it directly.
/// 3. If **all** unique schemas are [`SchemaNode::Object`], merge their
///    properties key-by-key (recursively).  This handles arrays of
///    objects where individual fields vary in type across elements.
/// 4. Otherwise, wrap the unique schemas in [`SchemaNode::OneOf`].
fn merge_schemas(schemas: &[SchemaNode]) -> SchemaNode {
    // --- deduplicate ---------------------------------------------------
    let mut unique: Vec<&SchemaNode> = Vec::new();
    for s in schemas {
        if !unique.contains(&s) {
            unique.push(s);
        }
    }

    if unique.is_empty() {
        return SchemaNode::Any;
    }

    if unique.len() == 1 {
        return unique[0].clone();
    }

    // --- object merge --------------------------------------------------
    let all_objects = unique
        .iter()
        .all(|s| matches!(s, SchemaNode::Object(_)));
    if all_objects {
        let mut merged_props: HashMap<String, Vec<SchemaNode>> = HashMap::new();
        for schema in &unique {
            if let SchemaNode::Object(props) = schema {
                for (key, val) in props {
                    merged_props
                        .entry(key.clone())
                        .or_default()
                        .push(val.clone());
                }
            }
        }

        let mut result_props: HashMap<String, SchemaNode> = HashMap::new();
        for (key, types) in merged_props {
            let merged = merge_schemas(&types);
            result_props.insert(key, merged);
        }

        return SchemaNode::Object(result_props);
    }

    // --- mixed types ---------------------------------------------------
    let types: Vec<SchemaNode> = unique.into_iter().cloned().collect();
    SchemaNode::OneOf(types)
}

impl SchemaNode {
    /// Produces a compact JSON representation of the schema.
    ///
    /// Scalar types become their name as a JSON string (`"string"`,
    /// `"integer"`, …).  Arrays and objects are represented structurally.
    /// `OneOf` becomes a JSON array of alternatives.
    pub fn to_compact_json(&self) -> Value {
        match self {
            SchemaNode::Null => Value::String("null".to_string()),
            SchemaNode::Boolean => Value::String("boolean".to_string()),
            SchemaNode::Integer => Value::String("integer".to_string()),
            SchemaNode::Float => Value::String("float".to_string()),
            SchemaNode::String => Value::String("string".to_string()),
            SchemaNode::Array(inner) => Value::Array(vec![inner.to_compact_json()]),
            SchemaNode::Object(props) => {
                let map: Map<String, Value> = props
                    .iter()
                    .map(|(k, v)| (k.clone(), v.to_compact_json()))
                    .collect();
                Value::Object(map)
            }
            SchemaNode::OneOf(types) => {
                Value::Array(types.iter().map(|t| t.to_compact_json()).collect())
            }
            SchemaNode::Any => Value::String("any".to_string()),
        }
    }

    /// Produces a JSON Schema Draft 2020-12 representation.
    ///
    /// Scalars become `{"type":"string"}`, objects become
    /// `{"type":"object","properties":{...},"required":[...]}`,
    /// arrays become `{"type":"array","items":{...}}`,
    /// OneOf becomes `{"oneOf":[...]}`, Any becomes `{}`.
    pub fn to_json_schema(&self) -> Value {
        match self {
            SchemaNode::Null => json!({"type": "null"}),
            SchemaNode::Boolean => json!({"type": "boolean"}),
            SchemaNode::Integer => json!({"type": "integer"}),
            SchemaNode::Float => json!({"type": "number"}),
            SchemaNode::String => json!({"type": "string"}),
            SchemaNode::Array(inner) => {
                json!({"type": "array", "items": inner.to_json_schema()})
            }
            SchemaNode::Object(props) => {
                let mut properties = Map::new();
                let mut required: Vec<Value> = Vec::new();
                for (key, schema) in props {
                    properties.insert(key.clone(), schema.to_json_schema());
                    // A field is required unless it can be null
                    if !matches!(schema, SchemaNode::OneOf(ref v) if v.iter().any(|s| matches!(s, SchemaNode::Null))) {
                        required.push(Value::String(key.clone()));
                    }
                }
                json!({
                    "type": "object",
                    "properties": properties,
                    "required": required,
                })
            }
            SchemaNode::OneOf(types) => {
                let schemas: Vec<Value> = types.iter().map(|t| t.to_json_schema()).collect();
                json!({"oneOf": schemas})
            }
            SchemaNode::Any => json!({}),
        }
    }

    /// Produces a TypeScript interface definition.
    ///
    /// Object properties use `key?: type` for optional (nullable) fields.
    /// Arrays become `Type[]`. OneOf becomes a union `string | number`.
    pub fn to_typescript(&self, name: &str) -> String {
        self.format_typescript(name, 0)
    }

    fn format_typescript(&self, name: &str, indent: usize) -> String {
        let pad = " ".repeat(indent);
        match self {
            SchemaNode::Null => format!("{pad}null"),
            SchemaNode::Boolean => format!("{pad}boolean"),
            SchemaNode::Integer | SchemaNode::Float => format!("{pad}number"),
            SchemaNode::String => format!("{pad}string"),
            SchemaNode::Any => format!("{pad}any"),
            SchemaNode::Array(inner) => {
                let inner_str = inner.type_script_type();
                format!("{pad}{inner_str}[]")
            }
            SchemaNode::OneOf(types) => {
                let parts: Vec<String> = types.iter().map(|t| t.type_script_type().trim().to_string()).collect();
                format!("{pad}{}", parts.join(" | "))
            }
            SchemaNode::Object(props) => {
                let mut lines: Vec<String> = Vec::new();
                lines.push(format!("{pad}interface {name} {{"));
                for (key, schema) in props {
                    let optional = matches!(schema, SchemaNode::OneOf(ref v) if v.iter().any(|s| matches!(s, SchemaNode::Null)));
                    let ts_type = schema.type_script_type();
                    let q = if optional { "?" } else { "" };
                    lines.push(format!("  {pad}{key}{q}: {ts_type};"));
                }
                lines.push(format!("{pad}}}"));
                lines.join("\n")
            }
        }
    }

    fn type_script_type(&self) -> String {
        match self {
            SchemaNode::Null => "null".to_string(),
            SchemaNode::Boolean => "boolean".to_string(),
            SchemaNode::Integer | SchemaNode::Float => "number".to_string(),
            SchemaNode::String => "string".to_string(),
            SchemaNode::Any => "any".to_string(),
            SchemaNode::Array(inner) => format!("{}[]", inner.type_script_type()),
            SchemaNode::OneOf(types) => {
                let parts: Vec<String> = types.iter().map(|t| t.type_script_type()).collect();
                parts.join(" | ")
            }
            SchemaNode::Object(props) => {
                let mut parts: Vec<String> = Vec::new();
                for (key, schema) in props {
                    let optional = matches!(schema, SchemaNode::OneOf(ref v) if v.iter().any(|s| matches!(s, SchemaNode::Null)));
                    let q = if optional { "?" } else { "" };
                    parts.push(format!("{key}{q}: {}", schema.type_script_type()));
                }
                format!("{{ {} }}", parts.join("; "))
            }
        }
    }

    /// Produces a Zod schema definition.
    ///
    /// Arrays use `z.array(...)`. OneOf uses `z.union([...])`.
    /// Nullable uses `.nullable()`.
    pub fn to_zod(&self, name: &str) -> String {
        let body = self.format_zod();
        format!("const {name} = {body};")
    }

    fn format_zod(&self) -> String {
        match self {
            SchemaNode::Null => "z.null()".to_string(),
            SchemaNode::Boolean => "z.boolean()".to_string(),
            SchemaNode::Integer => "z.number().int()".to_string(),
            SchemaNode::Float => "z.number()".to_string(),
            SchemaNode::String => "z.string()".to_string(),
            SchemaNode::Any => "z.any()".to_string(),
            SchemaNode::Array(inner) => format!("z.array({})", inner.format_zod()),
            SchemaNode::OneOf(types) => {
                // Check if this is a nullable pattern (OneOf containing Null)
                let has_null = types.iter().any(|t| matches!(t, SchemaNode::Null));
                if has_null {
                    let non_null: Vec<&SchemaNode> = types.iter().filter(|t| !matches!(t, SchemaNode::Null)).collect();
                    if non_null.len() == 1 {
                        return format!("{}.nullable()", non_null[0].format_zod());
                    }
                }
                let parts: Vec<String> = types.iter().map(|t| t.format_zod()).collect();
                format!("z.union([{}])", parts.join(", "))
            }
            SchemaNode::Object(props) => {
                let mut parts: Vec<String> = Vec::new();
                for (key, schema) in props {
                    parts.push(format!("{}: {}", key, schema.format_zod()));
                }
                format!("z.object({{ {} }})", parts.join(", "))
            }
        }
    }

    /// Produces a Pydantic v2 model definition.
    ///
    /// Arrays use `list[Type]`. OneOf uses `Union[...]`.
    /// Optional uses `Optional[Type]`.
    pub fn to_pydantic(&self, name: &str) -> String {
        let mut lines: Vec<String> = Vec::new();
        lines.push("from pydantic import BaseModel".to_string());
        lines.push("from typing import Optional, Union".to_string());
        lines.push(String::new());
        lines.push(format!("class {name}(BaseModel):"));
        match self {
            SchemaNode::Object(props) => {
                for (key, schema) in props {
                    let (py_type, is_optional) = schema.pydantic_type();
                    if is_optional {
                        lines.push(format!("    {key}: Optional[{py_type}]"));
                    } else {
                        lines.push(format!("    {key}: {py_type}"));
                    }
                }
            }
            _ => {
                let (py_type, _) = self.pydantic_type();
                lines.push(format!("    value: {py_type}"));
            }
        }
        lines.join("\n")
    }

    fn pydantic_type(&self) -> (String, bool) {
        match self {
            SchemaNode::Null => ("None".to_string(), false),
            SchemaNode::Boolean => ("bool".to_string(), false),
            SchemaNode::Integer => ("int".to_string(), false),
            SchemaNode::Float => ("float".to_string(), false),
            SchemaNode::String => ("str".to_string(), false),
            SchemaNode::Any => ("Any".to_string(), false),
            SchemaNode::Array(inner) => {
                let (inner_type, _) = inner.pydantic_type();
                (format!("list[{inner_type}]"), false)
            }
            SchemaNode::OneOf(types) => {
                let has_null = types.iter().any(|t| matches!(t, SchemaNode::Null));
                let non_null: Vec<&SchemaNode> = types.iter().filter(|t| !matches!(t, SchemaNode::Null)).collect();
                if has_null && non_null.len() == 1 {
                    (non_null[0].pydantic_type().0, true)
                } else {
                    let parts: Vec<String> = types.iter().map(|t| t.pydantic_type().0).collect();
                    (format!("Union[{}]", parts.join(", ")), false)
                }
            }
            SchemaNode::Object(props) => {
                let mut parts: Vec<String> = Vec::new();
                for (key, schema) in props {
                    let (py_type, is_optional) = schema.pydantic_type();
                    if is_optional {
                        parts.push(format!("{key}: Optional[{py_type}]"));
                    } else {
                        parts.push(format!("{key}: {py_type}"));
                    }
                }
                (format!("dict[str, Any] /* {{ {} }} */", parts.join("; ")), false)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------
    // Helper: build a deeply nested object for depth-limit tests.
    // ------------------------------------------------------------------
    fn build_deep_object(depth: usize) -> Value {
        let mut val = serde_json::json!({"leaf": true});
        for _ in 0..depth {
            val = serde_json::json!({"nested": val});
        }
        val
    }

    // ==================================================================
    // Schema inference tests
    // ==================================================================

    #[test]
    fn test_simple_object() {
        let inferrer = SchemaInferrer::new(32);
        let value = serde_json::json!({"name": "Alice", "age": 30});
        let schema = inferrer.infer(&value);

        let expected = SchemaNode::Object({
            let mut props = HashMap::new();
            props.insert("name".to_string(), SchemaNode::String);
            props.insert("age".to_string(), SchemaNode::Integer);
            props
        });
        assert_eq!(schema, expected);
    }

    #[test]
    fn test_nested_object() {
        let inferrer = SchemaInferrer::new(32);
        let value = serde_json::json!({"user": {"name": "Bob"}});
        let schema = inferrer.infer(&value);

        let expected = SchemaNode::Object({
            let mut props = HashMap::new();
            let mut inner = HashMap::new();
            inner.insert("name".to_string(), SchemaNode::String);
            props.insert("user".to_string(), SchemaNode::Object(inner));
            props
        });
        assert_eq!(schema, expected);
    }

    #[test]
    fn test_array_of_objects() {
        let inferrer = SchemaInferrer::new(32);
        let value = serde_json::json!([{"id": 1}, {"id": 2}]);
        let schema = inferrer.infer(&value);

        let expected = SchemaNode::Array(Box::new(SchemaNode::Object({
            let mut props = HashMap::new();
            props.insert("id".to_string(), SchemaNode::Integer);
            props
        })));
        assert_eq!(schema, expected);
    }

    #[test]
    fn test_mixed_array() {
        let inferrer = SchemaInferrer::new(32);
        let value = serde_json::json!([1, "two", true]);
        let schema = inferrer.infer(&value);

        let expected = SchemaNode::Array(Box::new(SchemaNode::OneOf(vec![
            SchemaNode::Integer,
            SchemaNode::String,
            SchemaNode::Boolean,
        ])));
        assert_eq!(schema, expected);
    }

    #[test]
    fn test_null_field() {
        let inferrer = SchemaInferrer::new(32);
        let value = serde_json::json!({"x": null});
        let schema = inferrer.infer(&value);

        let expected = SchemaNode::Object({
            let mut props = HashMap::new();
            props.insert("x".to_string(), SchemaNode::Null);
            props
        });
        assert_eq!(schema, expected);
    }

    #[test]
    fn test_nullable_field() {
        let inferrer = SchemaInferrer::new(32);
        let value = serde_json::json!([{"x": 1}, {"x": null}]);
        let schema = inferrer.infer(&value);

        let expected = SchemaNode::Array(Box::new(SchemaNode::Object({
            let mut props = HashMap::new();
            props.insert(
                "x".to_string(),
                SchemaNode::OneOf(vec![SchemaNode::Integer, SchemaNode::Null]),
            );
            props
        })));
        assert_eq!(schema, expected);
    }

    #[test]
    fn test_empty_object() {
        let inferrer = SchemaInferrer::new(32);
        let value = serde_json::json!({});
        let schema = inferrer.infer(&value);

        let expected = SchemaNode::Object(HashMap::new());
        assert_eq!(schema, expected);
    }

    #[test]
    fn test_empty_array() {
        let inferrer = SchemaInferrer::new(32);
        let value = serde_json::json!([]);
        let schema = inferrer.infer(&value);

        let expected = SchemaNode::Array(Box::new(SchemaNode::Any));
        assert_eq!(schema, expected);
    }

    #[test]
    fn test_deeply_nested() {
        let inferrer = SchemaInferrer::new(10);
        let value = build_deep_object(100);
        let schema = inferrer.infer(&value);

        // The top level should still be an Object (depth 0 < max_depth).
        assert!(
            matches!(&schema, SchemaNode::Object(_)),
            "expected Object at top level, got {schema:?}"
        );
    }

    #[test]
    fn test_unicode_field_names() {
        let inferrer = SchemaInferrer::new(32);
        let value = serde_json::json!({"名前": "太郎"});
        let schema = inferrer.infer(&value);

        let expected = SchemaNode::Object({
            let mut props = HashMap::new();
            props.insert("名前".to_string(), SchemaNode::String);
            props
        });
        assert_eq!(schema, expected);
    }

    #[test]
    fn test_homogeneous_array() {
        let inferrer = SchemaInferrer::new(32);
        let value = serde_json::json!([1, 2, 3, 4, 5]);
        let schema = inferrer.infer(&value);

        let expected = SchemaNode::Array(Box::new(SchemaNode::Integer));
        assert_eq!(schema, expected);
    }

    #[test]
    fn test_float_detection() {
        let inferrer = SchemaInferrer::new(32);
        let value = serde_json::json!({"price": 9.99});
        let schema = inferrer.infer(&value);

        let expected = SchemaNode::Object({
            let mut props = HashMap::new();
            props.insert("price".to_string(), SchemaNode::Float);
            props
        });
        assert_eq!(schema, expected);
    }

    // ==================================================================
    // to_compact_json tests
    // ==================================================================

    #[test]
    fn test_to_compact_json_simple() {
        let mut props = HashMap::new();
        props.insert("name".to_string(), SchemaNode::String);
        props.insert("age".to_string(), SchemaNode::Integer);
        let schema = SchemaNode::Object(props);

        let json = schema.to_compact_json();
        let expected = serde_json::json!({"name": "string", "age": "integer"});
        assert_eq!(json, expected);
    }

    #[test]
    fn test_to_compact_json_nested() {
        let mut props = HashMap::new();
        props.insert("id".to_string(), SchemaNode::Integer);
        let schema = SchemaNode::Array(Box::new(SchemaNode::Object(props)));

        let json = schema.to_compact_json();
        let expected = serde_json::json!([{"id": "integer"}]);
        assert_eq!(json, expected);
    }

    // ==================================================================
    // Deduplication tests
    // ==================================================================

    #[test]
    fn test_dedup_oneof() {
        let inferrer = SchemaInferrer::new(32);
        let value = serde_json::json!([1, 1, 1, "a"]);
        let schema = inferrer.infer(&value);

        // Should be Array(OneOf([Integer, String])), NOT 4 Integers + 1 String.
        let expected = SchemaNode::Array(Box::new(SchemaNode::OneOf(vec![
            SchemaNode::Integer,
            SchemaNode::String,
        ])));
        assert_eq!(schema, expected);
    }

    #[test]
    fn test_mixed_object_field() {
        let inferrer = SchemaInferrer::new(32);
        let value = serde_json::json!([{"x": 1}, {"x": "two"}]);
        let schema = inferrer.infer(&value);

        let expected = SchemaNode::Array(Box::new(SchemaNode::Object({
            let mut props = HashMap::new();
            props.insert(
                "x".to_string(),
                SchemaNode::OneOf(vec![SchemaNode::Integer, SchemaNode::String]),
            );
            props
        })));
        assert_eq!(schema, expected);
    }
}
