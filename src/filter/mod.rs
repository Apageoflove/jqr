use jaq_interpret::{Ctx, Filter, FilterT, ParseCtx, RcIter, Val};
use serde_json::Value;

use crate::error::JqrError;

/// A compiled jaq filter ready for execution against JSON values.
///
/// `FilterEngine` wraps the full jaq pipeline: parsing, compilation with the
/// core library and standard library, and execution against `serde_json::Value`
/// inputs.
pub struct FilterEngine {
    filter: Filter,
    inputs: RcIter<std::iter::Empty<Result<Val, String>>>,
}

impl std::fmt::Debug for FilterEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FilterEngine")
            .field("filter", &self.filter)
            .finish_non_exhaustive()
    }
}

impl FilterEngine {
    /// Parse and compile a jaq filter string.
    ///
    /// The filter is compiled with the full jaq core library (native filters
    /// like `length`, `keys_unsorted`, etc.) and the standard library
    /// (definitions like `map`, `select`, `keys`, etc.).
    ///
    /// # Errors
    ///
    /// Returns [`JqrError::JaqParse`] if the filter string has syntax errors
    /// or if compilation fails (e.g. undefined variables).
    pub fn compile(filter_str: &str) -> Result<Self, JqrError> {
        let (parsed, errs) = jaq_parse::parse(filter_str, jaq_parse::main());
        if let Some(err) = errs.into_iter().next() {
            return Err(JqrError::JaqParse(format!("{err:?}")));
        }
        let main = parsed.ok_or_else(|| JqrError::JaqParse("no parse result".into()))?;

        let mut parse_ctx = ParseCtx::new(Vec::new());
        parse_ctx.insert_natives(jaq_core::core());
        parse_ctx.insert_defs(jaq_std::std());

        let filter = parse_ctx.compile(main);
        if !parse_ctx.errs.is_empty() {
            return Err(JqrError::JaqParse(format!("{}", parse_ctx.errs[0].0)));
        }

        let inputs = RcIter::new(std::iter::empty::<Result<Val, String>>());

        Ok(FilterEngine { filter, inputs })
    }

    /// Run the compiled filter against a JSON input value.
    ///
    /// Returns a vector of result values. An empty vector means the filter
    /// produced no output for the given input (e.g. accessing a nonexistent
    /// field, or `select` filtering out the value).
    ///
    /// # Errors
    ///
    /// Returns [`JqrError::JaqEval`] if the filter encounters a runtime error
    /// (e.g. type mismatch, division by zero).
    pub fn run(&self, input: &Value) -> Result<Vec<Value>, JqrError> {
        let val = Val::from(input.clone());
        let inputs_ref: &RcIter<dyn Iterator<Item = Result<Val, String>>> = &self.inputs;
        let ctx = Ctx::new(Vec::new(), inputs_ref);
        let results = self.filter.run((ctx, val));

        let mut output = Vec::new();
        for result in results {
            match result {
                Ok(v) => output.push(Value::from(v)),
                Err(e) => return Err(JqrError::JaqEval(e.to_string())),
            }
        }
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Helper: compile a filter and run it on `input`, returning the result vec.
    fn run_filter(filter_str: &str, input: Value) -> Result<Vec<Value>, JqrError> {
        let engine = FilterEngine::compile(filter_str)?;
        engine.run(&input)
    }

    #[test]
    fn test_identity_filter() {
        let result = run_filter(".", json!({"a": 1})).expect("identity filter should succeed");
        assert_eq!(result, vec![json!({"a": 1})]);
    }

    #[test]
    fn test_field_access() {
        let result =
            run_filter(".a", json!({"a": 1, "b": 2})).expect("field access should succeed");
        assert_eq!(result, vec![json!(1)]);
    }

    #[test]
    fn test_array_iteration() {
        let result = run_filter(".[]", json!([1, 2, 3])).expect("array iteration should succeed");
        assert_eq!(result, vec![json!(1), json!(2), json!(3)]);
    }

    #[test]
    fn test_map_field() {
        let result = run_filter("map(.name)", json!([{"name": "A"}, {"name": "B"}]))
            .expect("map should succeed");
        assert_eq!(result, vec![json!(["A", "B"])]);
    }

    #[test]
    fn test_select_filter() {
        let result = run_filter(
            ".[] | select(.age > 30)",
            json!([{"age": 25}, {"age": 35}]),
        )
        .expect("select should succeed");
        assert_eq!(result, vec![json!({"age": 35})]);
    }

    #[test]
    fn test_length() {
        let result = run_filter("length", json!([1, 2, 3])).expect("length should succeed");
        assert_eq!(result, vec![json!(3)]);
    }

    #[test]
    fn test_keys() {
        let result = run_filter("keys", json!({"a": 1, "b": 2})).expect("keys should succeed");
        // keys returns sorted keys
        assert_eq!(result, vec![json!(["a", "b"])]);
    }

    #[test]
    fn test_pipe_chain() {
        let result = run_filter(".data | .[] | .id", json!({"data": [{"id": 1}, {"id": 2}]}))
            .expect("pipe chain should succeed");
        assert_eq!(result, vec![json!(1), json!(2)]);
    }

    #[test]
    fn test_empty_input() {
        let result = run_filter(".", json!({})).expect("empty input should succeed");
        assert_eq!(result, vec![json!({})]);
    }

    #[test]
    fn test_null_input() {
        let result = run_filter(".", json!(null)).expect("null input should succeed");
        assert_eq!(result, vec![json!(null)]);
    }

    #[test]
    fn test_unicode_fields() {
        let result = run_filter(".[\"名前\"]", json!({"名前": "太郎"}))
            .expect("unicode field should succeed");
        assert_eq!(result, vec![json!("太郎")]);
    }

    #[test]
    fn test_invalid_filter_syntax() {
        let result = FilterEngine::compile("[[[");
        assert!(result.is_err(), "expected parse error for '[[['");
        match result {
            Err(JqrError::JaqParse(_)) => {}
            other => panic!("expected JqrError::JaqParse, got {other:?}"),
        }
    }

    #[test]
    fn test_nonexistent_field() {
        let result = run_filter(".nonexistent", json!({"a": 1}))
            .expect("nonexistent field should not error");
        assert_eq!(result, vec![json!(null)]);
    }

    #[test]
    fn test_compile_error() {
        let result = FilterEngine::compile("invalid(");
        assert!(result.is_err(), "expected parse error for 'invalid('");
        match result {
            Err(JqrError::JaqParse(_)) => {}
            other => panic!("expected JqrError::JaqParse, got {other:?}"),
        }
    }
}
