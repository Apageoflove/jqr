//! jqr MCP server — exposes `query`, `repair`, `schema`, `sample` over stdio.
//!
//! Built on rmcp v0.6 with the three-macro pattern:
//! [`tool_router`] → [`tool_handler`] → [`ServiceExt::serve`].

use rmcp::{
    ServerHandler,
    handler::server::{
        router::tool::ToolRouter,
        wrapper::Parameters,
    },
    model::{CallToolResult, Content, ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router,
    ServiceExt,
    transport::stdio,
};
use serde_json::Value;

use crate::config::SchemaFormat;
use crate::error::JqrError;
use crate::filter::FilterEngine;
use crate::input::InputReader;
use crate::output::envelope::Envelope;
use crate::output::truncate::Truncator;
use crate::repair::JsonRepairer;
use crate::schema::SchemaInferrer;

// ---- Parameter types -------------------------------------------------------

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[allow(dead_code)]
pub struct QueryRequest {
    #[schemars(description = "jq filter expression (e.g. '.users | map(.name)')")]
    pub expression: String,
    #[schemars(description = "JSON input as a string")]
    pub input: String,
    #[schemars(description = "Token budget for output truncation (default: 4096)")]
    pub token_budget: Option<usize>,
    #[schemars(description = "Number of sample records (default: 5)")]
    pub sample_size: Option<usize>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[allow(dead_code)]
pub struct RepairRequest {
    #[schemars(description = "Possibly-malformed JSON string to repair")]
    pub input: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[allow(dead_code)]
pub struct SchemaRequest {
    #[schemars(description = "JSON input string to infer schema from")]
    pub input: String,
    #[schemars(description = "Maximum recursion depth (default: 32)")]
    pub max_depth: Option<usize>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[allow(dead_code)]
pub struct SampleRequest {
    #[schemars(description = "JSON input string (must be a JSON array)")]
    pub input: String,
    #[schemars(description = "Number of elements to sample")]
    pub count: usize,
}

// ---- Server ---------------------------------------------------------------

#[derive(Clone)]
pub struct JqrServer {
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl JqrServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }
}

impl Default for JqrServer {
    fn default() -> Self {
        Self::new()
    }
}

impl JqrServer {

    /// Evaluate a jq expression against JSON input.
    /// Returns a schema-first envelope with sample data.
    #[tool(description = "Evaluate a jq filter expression against JSON input. Returns a schema-first envelope with sample data, total count, and token usage.")]
    async fn query(
        &self,
        Parameters(req): Parameters<QueryRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let token_budget = req.token_budget.unwrap_or(4096);
        let sample_size = req.sample_size.unwrap_or(5).max(1);

        let value = parse_json_input(&req.input)
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;

        let engine = FilterEngine::compile(&req.expression)
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;

        let results = engine.run(&value)
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;

        let truncator = Truncator::new(token_budget, sample_size);

        let truncation = truncator.truncate(&results);
        let inferrer = SchemaInferrer::new(32);
        let schema = inferrer.infer(&value);
        let envelope = Envelope::from_results(&results, schema, truncation, SchemaFormat::JsonSchema);

        let output = envelope.to_json(true, false);
        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    /// Attempt to repair malformed LLM JSON output.
    #[tool(description = "Repair malformed JSON (LLM output with prose wrappers, trailing commas, unclosed braces, smart quotes, markdown fences)")]
    async fn repair(
        &self,
        Parameters(req): Parameters<RepairRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let repairer = JsonRepairer::new();
        let repaired = repairer.repair(&req.input)
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;

        Ok(CallToolResult::success(vec![Content::text(repaired)]))
    }

    /// Infer a compact JSON schema from the input.
    #[tool(description = "Infer a compact JSON schema describing the structure of the given JSON input")]
    async fn schema(
        &self,
        Parameters(req): Parameters<SchemaRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let max_depth = req.max_depth.unwrap_or(32);

        let value = parse_json_input(&req.input)
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;

        let inferrer = SchemaInferrer::new(max_depth);
        let schema = inferrer.infer(&value);
        let schema_json = schema.to_compact_json();
        let output = serde_json::to_string_pretty(&schema_json)
            .unwrap_or_else(|_| String::from("{}"));

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    /// Sample N elements from a JSON array.
    #[tool(description = "Extract the first N elements from a JSON array (deterministic sampling)")]
    async fn sample(
        &self,
        Parameters(req): Parameters<SampleRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let value = parse_json_input(&req.input)
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;

        let sample = match &value {
            Value::Array(arr) => {
                let n = req.count.min(arr.len());
                Value::Array(arr[..n].to_vec())
            }
            other => {
                return Err(rmcp::ErrorData::internal_error(
                    format!("expected JSON array, got {}", type_name(other)),
                    None,
                ));
            }
        };

        let output = serde_json::to_string_pretty(&sample)
            .unwrap_or_else(|_| String::from("[]"));

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }
}

#[tool_handler]
impl ServerHandler for JqrServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "jqr: jq-style JSON query, repair, schema inference, and sampling for AI agents".into(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

// ---- Entry point ----------------------------------------------------------

/// Start the MCP server over stdio. Blocks until the client disconnects.
///
/// All diagnostic output goes to stderr so the JSON-RPC stream on stdout
/// is not corrupted.
pub async fn run_stdio_server() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let service = JqrServer::new().serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}

// ---- Helpers --------------------------------------------------------------

#[allow(dead_code)]
fn parse_json_input(input: &str) -> Result<Value, JqrError> {
    let mut reader = InputReader::from_str(input);
    reader.detect();
    reader.parse()
}

#[allow(dead_code)]
fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_json_input_valid() {
        let result = parse_json_input(r#"{"key": "value"}"#);
        assert!(result.is_ok());
        let v = result.unwrap();
        assert_eq!(v["key"], Value::String("value".into()));
    }

    #[test]
    fn test_parse_json_input_invalid() {
        let result = parse_json_input("not json");
        assert!(result.is_err());
    }

    #[test]
    fn test_type_name() {
        assert_eq!(type_name(&Value::Null), "null");
        assert_eq!(type_name(&Value::Bool(true)), "boolean");
        assert_eq!(type_name(&Value::Number(serde_json::Number::from(1))), "number");
        assert_eq!(type_name(&Value::String("hi".into())), "string");
        assert_eq!(type_name(&Value::Array(vec![])), "array");
        assert_eq!(type_name(&Value::Object(serde_json::Map::new())), "object");
    }
}
