//! RULE 10: Per-Module Functional Accuracy Verification
//!
//! This file exercises each module's public API with real-world inputs
//! and verifies functional correctness, edge case handling, and
//! cross-module compatibility.
//!
//! Run: cargo test --test functional_verification

use jqr::config::agent::{Agent, AgentDetector, ENV_LOCK};
use jqr::config::JqrConfig;
use jqr::input::{InputFormat, InputReader};
use jqr::output::token::TokenCounter;
use jqr::repair::JsonRepairer;

// ============================================================================
// Module: InputReader (T4)
// ============================================================================

#[test]
fn verify_input_json_object() {
    let mut reader = InputReader::from_str(r#"{"name": "Alice", "age": 30}"#);
    reader.detect();
    assert_eq!(reader.format, InputFormat::Json);
    let val = reader.parse().expect("parse JSON object");
    assert_eq!(val["name"], "Alice");
    assert_eq!(val["age"], 30);
}

#[test]
fn verify_input_json_array() {
    let mut reader = InputReader::from_str(r#"[1, 2, 3, 4, 5]"#);
    reader.detect();
    assert_eq!(reader.format, InputFormat::Json);
    let val = reader.parse().expect("parse JSON array");
    assert_eq!(val.as_array().map(|a| a.len()), Some(5));
}

#[test]
fn verify_input_nested_json() {
    let input = r#"{"users": [{"id": 1, "name": "Alice"}, {"id": 2, "name": "Bob"}], "total": 2}"#;
    let mut reader = InputReader::from_str(input);
    reader.detect();
    let val = reader.parse().expect("parse nested JSON");
    assert_eq!(val["total"], 2);
    assert_eq!(val["users"][0]["name"], "Alice");
    assert_eq!(val["users"][1]["id"], 2);
}

#[test]
fn verify_input_yaml() {
    let yaml = "name: Alice\nage: 30\nhobbies:\n  - reading\n  - coding\n";
    let mut reader = InputReader::from_str(yaml);
    reader.detect();
    let val = reader.parse().expect("parse YAML");
    assert_eq!(val["name"], "Alice");
    assert_eq!(val["age"], 30);
    assert_eq!(val["hobbies"][0], "reading");
}

#[test]
fn verify_input_toml() {
    let toml = "[server]\nhost = \"localhost\"\nport = 8080\n\n[database]\nurl = \"postgres://localhost/db\"\n";
    let mut reader = InputReader::from_str(toml);
    reader.detect();
    let val = reader.parse().expect("parse TOML");
    assert_eq!(val["server"]["host"], "localhost");
    assert_eq!(val["server"]["port"], 8080);
    assert_eq!(val["database"]["url"], "postgres://localhost/db");
}

#[test]
fn verify_input_csv() {
    let csv = "name,age,city\nAlice,30,NYC\nBob,25,SF\nCharlie,35,LA\n";
    let mut reader = InputReader::from_str(csv);
    reader.detect();
    let val = reader.parse().expect("parse CSV");
    let arr = val.as_array().expect("CSV should parse to array");
    assert_eq!(arr.len(), 3);
    assert_eq!(arr[0]["name"], "Alice");
    assert_eq!(arr[1]["city"], "SF");
}

#[test]
fn verify_input_unicode_json() {
    let input = r#"{"名前": "太郎", "年龄": 25, "城市": "東京"}"#;
    let mut reader = InputReader::from_str(input);
    reader.detect();
    let val = reader.parse().expect("parse Unicode JSON");
    assert_eq!(val["名前"], "太郎");
    assert_eq!(val["年龄"], 25);
}

#[test]
fn verify_input_deeply_nested() {
    // 20 levels deep
    let mut json = String::from("{\"a\":");
    for _ in 0..19 {
        json.push_str("{\"b\":");
    }
    json.push_str("\"deep\"");
    for _ in 0..20 {
        json.push('}');
    }
    let mut reader = InputReader::from_str(&json);
    reader.detect();
    let val = reader.parse().expect("parse deeply nested JSON");
    // Navigate to the deep value
    let mut current = &val;
    for _ in 0..20 {
        current = current.as_object().and_then(|o| o.values().next()).expect("nested object");
    }
    assert_eq!(current, "deep");
}

#[test]
fn verify_input_explicit_format() {
    // Force JSON format even for YAML-looking content
    let mut reader = InputReader::from_str(r#"{"key": "value"}"#);
    reader.format = InputFormat::Json;
    reader.detect(); // should NOT change format
    assert_eq!(reader.format, InputFormat::Json);
    let val = reader.parse().expect("parse with explicit format");
    assert_eq!(val["key"], "value");
}

#[test]
fn verify_input_empty_error() {
    let reader = InputReader::from_str("");
    let result = reader.parse();
    assert!(result.is_err());
}

#[test]
fn verify_input_malformed_json() {
    let mut reader = InputReader::from_str(r#"{"key": "value""#);
    reader.detect();
    let result = reader.parse();
    assert!(result.is_err());
}

// ============================================================================
// Module: TokenCounter (T7)
// ============================================================================

#[test]
fn verify_token_count_english() {
    let counter = TokenCounter::new();
    let text = "The quick brown fox jumps over the lazy dog.";
    let count = counter.count(text);
    assert!(count >= 8, "expected at least 8 tokens for 9-word sentence, got {count}");
    assert!(count <= 15, "expected at most 15 tokens, got {count}");
}

#[test]
fn verify_token_count_json() {
    let counter = TokenCounter::new();
    let json = r#"{"users":[{"id":1,"name":"Alice","email":"alice@example.com"},{"id":2,"name":"Bob","email":"bob@example.com"}]}"#;
    let count = counter.count(json);
    assert!(count > 20, "expected >20 tokens for JSON with 2 records, got {count}");
}

#[test]
fn verify_token_count_chinese() {
    let counter = TokenCounter::new();
    let text = "人工智能正在改变世界，深度学习是其中的核心技术。";
    let count = counter.count(text);
    assert!(count > 0, "Chinese text should have positive token count");
    // Chinese characters typically tokenize to 1-3 tokens each
    assert!(count >= 10, "expected at least 10 tokens for Chinese sentence, got {count}");
}

#[test]
fn verify_token_count_japanese() {
    let counter = TokenCounter::new();
    let text = "こんにちは、世界！私は日本語を勉強しています。";
    let count = counter.count(text);
    assert!(count > 0, "Japanese text should have positive token count");
}

#[test]
fn verify_token_count_korean() {
    let counter = TokenCounter::new();
    let text = "안녕하세요, 세계! 저는 한국어를 공부하고 있습니다.";
    let count = counter.count(text);
    assert!(count > 0, "Korean text should have positive token count");
}

#[test]
fn verify_token_count_monotonic() {
    let counter = TokenCounter::new();
    let short = counter.count("hi");
    let medium = counter.count("hello world this is a test");
    let long = counter.count("hello world this is a much longer test with many more words to count tokens for");
    assert!(medium > short, "medium ({medium}) should have more tokens than short ({short})");
    assert!(long > medium, "long ({long}) should have more tokens than medium ({medium})");
}

#[test]
fn verify_token_truncation_preserves_prefix() {
    let counter = TokenCounter::new();
    let original = "Hello, world! This is a test of the truncation function.";
    let truncated = counter.truncate_to_tokens(original, 3);
    assert!(!truncated.is_empty(), "truncated should not be empty");
    assert!(truncated.len() < original.len(), "truncated should be shorter");
    // The truncated text should be a prefix of the original
    assert!(original.starts_with(&truncated),
        "truncated '{truncated}' should be prefix of original '{original}'");
}

#[test]
fn verify_token_truncation_large_budget() {
    let counter = TokenCounter::new();
    let original = "Short text.";
    let truncated = counter.truncate_to_tokens(original, 99999);
    assert_eq!(truncated, original, "large budget should preserve original");
}

#[test]
fn verify_token_split_reassembly() {
    let counter = TokenCounter::new();
    let original = "Hello, world!";
    let tokens = counter.split_by_token(original);
    let reassembled: String = tokens.concat();
    assert_eq!(reassembled, original, "split+join should reconstruct original");
}

// ============================================================================
// Module: JsonRepairer (T11)
// ============================================================================

#[test]
fn verify_repair_valid_json_passthrough() {
    let repairer = JsonRepairer::new();
    let input = r#"{"name": "Alice", "age": 30}"#;
    let result = repairer.repair(input).expect("repair valid JSON");
    let parsed: serde_json::Value = serde_json::from_str(&result).expect("result should be valid JSON");
    assert_eq!(parsed["name"], "Alice");
    assert_eq!(parsed["age"], 30);
}

#[test]
fn verify_repair_prose_wrapped() {
    let repairer = JsonRepairer::new();
    let input = "Sure, here is the data you requested:\n\n{\"name\": \"Alice\", \"age\": 30}\n\nLet me know if you need anything else!";
    let result = repairer.repair(input).expect("repair prose-wrapped");
    let parsed: serde_json::Value = serde_json::from_str(&result).expect("result should be valid JSON");
    assert_eq!(parsed["name"], "Alice");
}

#[test]
fn verify_repair_markdown_fence() {
    let repairer = JsonRepairer::new();
    let input = "```json\n{\"key\": \"value\"}\n```";
    let result = repairer.repair(input).expect("repair markdown fence");
    let parsed: serde_json::Value = serde_json::from_str(&result).expect("result should be valid JSON");
    assert_eq!(parsed["key"], "value");
}

#[test]
fn verify_repair_trailing_comma_object() {
    let repairer = JsonRepairer::new();
    let input = "{\"a\": 1, \"b\": 2,}";
    let result = repairer.repair(input).expect("repair trailing comma");
    let parsed: serde_json::Value = serde_json::from_str(&result).expect("result should be valid JSON");
    assert_eq!(parsed["a"], 1);
    assert_eq!(parsed["b"], 2);
}

#[test]
fn verify_repair_trailing_comma_array() {
    let repairer = JsonRepairer::new();
    let input = "[1, 2, 3,]";
    let result = repairer.repair(input).expect("repair trailing comma in array");
    let parsed: serde_json::Value = serde_json::from_str(&result).expect("result should be valid JSON");
    assert_eq!(parsed.as_array().map(|a| a.len()), Some(3));
}

#[test]
fn verify_repair_unclosed_braces() {
    let repairer = JsonRepairer::new();
    let input = r#"{"users": [{"name": "Alice"}, {"name": "Bob""#;
    let result = repairer.repair(input).expect("repair unclosed braces");
    let parsed: serde_json::Value = serde_json::from_str(&result).expect("result should be valid JSON");
    assert_eq!(parsed["users"][0]["name"], "Alice");
}

#[test]
fn verify_repair_smart_quotes() {
    let repairer = JsonRepairer::new();
    // Smart quotes inside strings are escaped to keep JSON valid.
    let input = "{\"key\": \"\u{201c}hello\u{201d}\"}";
    let result = repairer.repair(input).expect("repair smart quotes");
    let parsed: serde_json::Value = serde_json::from_str(&result).expect("result should be valid JSON");
    assert_eq!(parsed["key"], "\"hello\"", "smart quotes inside strings should be escaped to ASCII");
}

#[test]
fn verify_repair_single_quoted_json() {
    let repairer = JsonRepairer::new();
    let input = "{'name':'Alice'}";
    let result = repairer.repair(input).expect("repair single-quoted JSON");
    let parsed: serde_json::Value = serde_json::from_str(&result).expect("result should be valid JSON");
    assert_eq!(parsed["name"], "Alice");
}

#[test]
fn verify_repair_unclosed_string() {
    let repairer = JsonRepairer::new();
    let input = "{\"name\":\"Alice";
    let result = repairer.repair(input).expect("repair unclosed string");
    let parsed: serde_json::Value = serde_json::from_str(&result).expect("result should be valid JSON");
    assert_eq!(parsed["name"], "Alice");
}

#[test]
fn verify_repair_combined_llm_output() {
    let repairer = JsonRepairer::new();
    // Simulate a typical LLM output: prose + markdown + trailing comma
    let input = "Here's the data:\n```json\n{\"name\": \"Alice\", \"age\": 30,}\n```\nHope that helps!";
    let result = repairer.repair(input).expect("repair combined LLM output");
    let parsed: serde_json::Value = serde_json::from_str(&result).expect("result should be valid JSON");
    assert_eq!(parsed["name"], "Alice");
    assert_eq!(parsed["age"], 30);
}

#[test]
fn verify_repair_nested_truncation() {
    let repairer = JsonRepairer::new();
    // Simulate truncated nested JSON from LLM
    let input = r#"{"data": {"users": [{"id": 1, "name": "Alice"}]"#;
    let result = repairer.repair(input).expect("repair nested truncation");
    let parsed: serde_json::Value = serde_json::from_str(&result).expect("result should be valid JSON");
    assert_eq!(parsed["data"]["users"][0]["id"], 1);
}

#[test]
fn verify_repair_unicode_in_strings() {
    let repairer = JsonRepairer::new();
    let input = r#"{"message": "こんにちは世界"}"#;
    let result = repairer.repair(input).expect("repair with Unicode");
    let parsed: serde_json::Value = serde_json::from_str(&result).expect("result should be valid JSON");
    assert_eq!(parsed["message"], "こんにちは世界");
}

// ============================================================================
// Module: AgentDetector (T12)
// ============================================================================

#[test]
fn verify_agent_detection_claude() {
    let _guard = ENV_LOCK.lock().unwrap();
    std::env::set_var("CLAUDE_CODE", "1");
    let agent = AgentDetector::detect();
    std::env::remove_var("CLAUDE_CODE");
    assert_eq!(agent, Some(Agent::ClaudeCode));
}

#[test]
fn verify_agent_detection_opencode() {
    let _guard = ENV_LOCK.lock().unwrap();
    std::env::set_var("OPENCODE", "1");
    let agent = AgentDetector::detect();
    std::env::remove_var("OPENCODE");
    assert_eq!(agent, Some(Agent::OpenCode));
}

#[test]
fn verify_agent_detection_none() {
    let _guard = ENV_LOCK.lock().unwrap();
    // Ensure no agent env vars are set
    std::env::remove_var("CLAUDE_CODE");
    std::env::remove_var("OPENCODE");
    std::env::remove_var("CURSOR_TRACE");
    std::env::remove_var("CODEX_CLI");
    std::env::remove_var("GEMINI_CLI");
    let agent = AgentDetector::detect();
    assert_eq!(agent, None);
}

#[test]
fn verify_agent_token_budgets() {
    assert_eq!(AgentDetector::default_token_budget_for(Agent::ClaudeCode), 4096);
    assert_eq!(AgentDetector::default_token_budget_for(Agent::OpenCode), 8192);
    assert_eq!(AgentDetector::default_token_budget_for(Agent::Cursor), 4096);
    assert_eq!(AgentDetector::default_token_budget_for(Agent::CodexCli), 8192);
    assert_eq!(AgentDetector::default_token_budget_for(Agent::GeminiCli), 8192);
    assert_eq!(AgentDetector::default_token_budget_for(Agent::Unknown), 4096);
}

#[test]
fn verify_agent_sample_sizes() {
    assert_eq!(AgentDetector::default_sample_size_for(Agent::ClaudeCode), 3);
    assert_eq!(AgentDetector::default_sample_size_for(Agent::OpenCode), 5);
    assert_eq!(AgentDetector::default_sample_size_for(Agent::Cursor), 3);
    assert_eq!(AgentDetector::default_sample_size_for(Agent::CodexCli), 5);
    assert_eq!(AgentDetector::default_sample_size_for(Agent::GeminiCli), 5);
    assert_eq!(AgentDetector::default_sample_size_for(Agent::Unknown), 5);
}

// ============================================================================
// Module: Config (T2)
// ============================================================================

#[test]
fn verify_config_defaults() {
    let config = JqrConfig::default();
    assert_eq!(config.output.mode.to_string(), "schema");
    assert_eq!(config.output.default_token_budget, 4096);
    assert_eq!(config.output.sample_size, 100);
    assert!(config.output.pretty);
    assert_eq!(config.input.format.to_string(), "auto");
    assert_eq!(config.schema.max_depth, 10);
}

// ============================================================================
// Cross-Module Compatibility Tests
// ============================================================================

#[test]
fn verify_input_to_token_pipeline() {
    // InputReader produces valid JSON → TokenCounter can count it
    let mut reader = InputReader::from_str(r#"{"data": [1, 2, 3, 4, 5]}"#);
    reader.detect();
    let val = reader.parse().expect("parse JSON");
    let json_str = serde_json::to_string(&val).expect("serialize to string");
    let counter = TokenCounter::new();
    let count = counter.count(&json_str);
    assert!(count > 0, "token count should be positive");
}

#[test]
fn verify_repair_to_input_pipeline() {
    // JsonRepairer output → InputReader can parse it
    let repairer = JsonRepairer::new();
    let broken = "```json\n{\"name\": \"Alice\", \"age\": 30,}\n```";
    let repaired = repairer.repair(broken).expect("repair");
    let mut reader = InputReader::from_str(&repaired);
    reader.detect();
    let val = reader.parse().expect("parse repaired JSON");
    assert_eq!(val["name"], "Alice");
    assert_eq!(val["age"], 30);
}

#[test]
fn verify_full_pre_pipeline() {
    // Full pre-filter pipeline: Repair → Input → TokenCount
    let repairer = JsonRepairer::new();

    // Simulate LLM output with multiple issues
    let llm_output = "Here's your data:\n```json\n{\"users\": [{\"name\": \"Alice\", \"score\": 95}, {\"name\": \"Bob\", \"score\": 87,}], \"total\": 2}\n```\nDone!";

    let repaired = repairer.repair(llm_output).expect("repair");
    let mut reader = InputReader::from_str(&repaired);
    reader.detect();
    let val = reader.parse().expect("parse");

    // Verify structure
    assert_eq!(val["total"], 2);
    assert_eq!(val["users"][0]["name"], "Alice");
    assert_eq!(val["users"][1]["score"], 87);

    // Verify token counting works on the result
    let json_str = serde_json::to_string(&val).expect("serialize");
    let counter = TokenCounter::new();
    let count = counter.count(&json_str);
    assert!(count > 0, "token count should be positive");
}
