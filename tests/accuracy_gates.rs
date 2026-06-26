//! Accuracy Gates
//!
//! Comprehensive correctness gates measuring jqr's accuracy against jq,
//! schema inference precision, token count accuracy, format detection
//! accuracy, and repair quality.
//!
//! Run: cargo test --test accuracy_gates

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use std::io::Write;
use std::process::{Command as StdCommand, Stdio};

// ================================================================
// Category 1: jq Compatibility
// ================================================================

fn jq_available() -> bool {
    StdCommand::new("jq").arg("--version").output().is_ok()
}
fn run_jq(filter: &str, input: &str) -> Vec<u8> {
    let mut child = StdCommand::new("jq")
        .arg(filter)
        .arg("-c")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("jq spawn");
    child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    child.wait_with_output().expect("jq output").stdout
}
#[test]
fn verify_gate_jq_identity() {
    if !jq_available() {
        return;
    }
    let input = r#"{"a":1,"b":2}"#;
    let jq_out = run_jq(".", input);
    let jqr_out = Command::cargo_bin("jqr")
        .expect("binary built")
        .arg(".")
        .arg("--raw")
        .write_stdin(input)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let jq_val: Value = serde_json::from_slice(&jq_out).expect("jq valid JSON");
    let jqr_val: Value = serde_json::from_slice(&jqr_out).expect("jqr valid JSON");
    assert_eq!(jq_val, jqr_val, "jq and jqr --raw must produce identical output");
}
#[test]
fn verify_gate_jq_field_access() {
    if !jq_available() {
        return;
    }
    let input = r#"{"name":"Alice"}"#;
    let jq_out = run_jq(".name", input);
    let jqr_out = Command::cargo_bin("jqr")
        .expect("binary built")
        .arg(".name")
        .arg("--raw")
        .write_stdin(input)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let jq_val: Value = serde_json::from_slice(&jq_out).expect("jq valid JSON");
    let jqr_val: Value = serde_json::from_slice(&jqr_out).expect("jqr valid JSON");
    assert_eq!(jq_val, jqr_val, ".name field access must match jq");
}
#[test]
fn verify_gate_jq_array_iteration() {
    if !jq_available() {
        return;
    }
    let input = "[1,2,3]";
    let jq_out = run_jq(".[]", input);
    let jqr_out = Command::cargo_bin("jqr")
        .expect("binary built")
        .arg(".[]")
        .arg("--raw")
        .write_stdin(input)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    // jq outputs line-by-line; jqr --raw outputs a JSON array for multiple results.
    // Compare the parsed values, not the serialization format.
    let jq_lines: Vec<Value> = String::from_utf8_lossy(&jq_out)
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| serde_json::from_str(l).expect("jq line valid JSON"))
        .collect();
    let jqr_val: Value = serde_json::from_slice(&jqr_out).expect("jqr valid JSON");
    let jqr_arr = jqr_val.as_array().expect("jqr --raw multi-result should be array");
    assert_eq!(jq_lines, *jqr_arr, ".[] array iteration values must match jq");
}
#[test]
fn verify_gate_jq_map() {
    if !jq_available() {
        return;
    }
    let input = r#"[{"name":"A"},{"name":"B"}]"#;
    let jq_out = run_jq("map(.name)", input);
    let jqr_out = Command::cargo_bin("jqr")
        .expect("binary built")
        .arg("map(.name)")
        .arg("--raw")
        .write_stdin(input)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let jq_val: Value = serde_json::from_slice(&jq_out).expect("jq valid JSON");
    let jqr_val: Value = serde_json::from_slice(&jqr_out).expect("jqr valid JSON");
    assert_eq!(jq_val, jqr_val, "map(.name) must match jq");
}
#[test]
fn verify_gate_jq_select() {
    if !jq_available() {
        return;
    }
    let input = r#"[{"age":25},{"age":35}]"#;
    let jq_out = run_jq(".[] | select(.age > 30)", input);
    let jqr_out = Command::cargo_bin("jqr")
        .expect("binary built")
        .arg(".[] | select(.age > 30)")
        .arg("--raw")
        .write_stdin(input)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let jq_val: Value = serde_json::from_slice(&jq_out).expect("jq valid JSON");
    let jqr_val: Value = serde_json::from_slice(&jqr_out).expect("jqr valid JSON");
    assert_eq!(jq_val, jqr_val, "select filter must match jq");
}
#[test]
fn verify_gate_jq_length() {
    if !jq_available() {
        return;
    }
    let input = "[1,2,3,4,5]";
    let jq_out = run_jq("length", input);
    let jqr_out = Command::cargo_bin("jqr")
        .expect("binary built")
        .arg("length")
        .arg("--raw")
        .write_stdin(input)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let jq_val: Value = serde_json::from_slice(&jq_out).expect("jq valid JSON");
    let jqr_val: Value = serde_json::from_slice(&jqr_out).expect("jqr valid JSON");
    assert_eq!(jq_val, jqr_val, "length must match jq");
}
#[test]
fn verify_gate_jq_keys() {
    if !jq_available() {
        return;
    }
    let input = r#"{"a":1,"b":2}"#;
    let jq_out = run_jq("keys", input);
    let jqr_out = Command::cargo_bin("jqr")
        .expect("binary built")
        .arg("keys")
        .arg("--raw")
        .write_stdin(input)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let jq_val: Value = serde_json::from_slice(&jq_out).expect("jq valid JSON");
    let jqr_val: Value = serde_json::from_slice(&jqr_out).expect("jqr valid JSON");
    assert_eq!(jq_val, jqr_val, "keys must match jq");
}
#[test]
fn verify_gate_jq_pipe_chain() {
    if !jq_available() {
        return;
    }
    let input = r#"{"data":[{"id":1},{"id":2}]}"#;
    let jq_out = run_jq(".data | .[] | .id", input);
    let jqr_out = Command::cargo_bin("jqr")
        .expect("binary built")
        .arg(".data | .[] | .id")
        .arg("--raw")
        .write_stdin(input)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    // jq outputs line-by-line; jqr --raw outputs a JSON array for multiple results.
    let jq_lines: Vec<Value> = String::from_utf8_lossy(&jq_out)
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| serde_json::from_str(l).expect("jq line valid JSON"))
        .collect();
    let jqr_val: Value = serde_json::from_slice(&jqr_out).expect("jqr valid JSON");
    let jqr_arr = jqr_val.as_array().expect("jqr --raw multi-result should be array");
    assert_eq!(jq_lines, *jqr_arr, "pipe chain values must match jq");
}

// ================================================================
// Category 2: Schema Accuracy
// ================================================================

#[test]
fn verify_gate_schema_simple_object() {
    let output = Command::cargo_bin("jqr")
        .expect("binary built")
        .arg(".")
        .write_stdin(r#"{"name":"Alice","age":30}"#)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: Value = serde_json::from_slice(&output).expect("valid JSON");
    let schema = &v["schema"];
    assert!(schema.is_object(), "schema should be an object");
    assert_eq!(schema["type"], Value::String("object".into()), "schema type should be object");
    assert_eq!(schema["properties"]["name"]["type"], Value::String("string".into()), "name field should be string");
    assert_eq!(schema["properties"]["age"]["type"], Value::String("integer".into()), "age field should be integer");
}
#[test]
fn verify_gate_schema_array() {
    let output = Command::cargo_bin("jqr")
        .expect("binary built")
        .arg(".")
        .write_stdin("[1,2,3]")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: Value = serde_json::from_slice(&output).expect("valid JSON");
    let schema = &v["schema"];
    assert!(schema.is_object(), "schema should be an object");
    assert_eq!(schema["type"], Value::String("array".into()), "array schema type should be array");
    assert_eq!(schema["items"]["type"], Value::String("integer".into()), "array element should be integer");
}
#[test]
fn verify_gate_schema_array_of_objects() {
    let output = Command::cargo_bin("jqr")
        .expect("binary built")
        .arg(".")
        .write_stdin(r#"[{"id":1},{"id":2}]"#)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: Value = serde_json::from_slice(&output).expect("valid JSON");
    let schema = &v["schema"];
    assert!(schema.is_object(), "schema should be an object");
    assert_eq!(schema["type"], Value::String("array".into()), "schema type should be array");
    let items = &schema["items"];
    assert_eq!(items["type"], Value::String("object".into()), "items type should be object");
    assert_eq!(items["properties"]["id"]["type"], Value::String("integer".into()), "id field should be integer");
}
#[test]
fn verify_gate_schema_null_field() {
    let output = Command::cargo_bin("jqr")
        .expect("binary built")
        .arg(".")
        .write_stdin(r#"{"x":null}"#)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: Value = serde_json::from_slice(&output).expect("valid JSON");
    let schema = &v["schema"];
    assert!(schema.is_object(), "schema should be an object");
    assert_eq!(schema["type"], Value::String("object".into()), "schema type should be object");
    assert_eq!(schema["properties"]["x"]["type"], Value::String("null".into()), "null field should be 'null'");
}
#[test]
fn verify_gate_schema_mixed_array() {
    let output = Command::cargo_bin("jqr")
        .expect("binary built")
        .arg(".")
        .write_stdin(r#"[1,"two",true]"#)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: Value = serde_json::from_slice(&output).expect("valid JSON");
    let schema = &v["schema"];
    assert!(schema.is_object(), "schema should be an object");
    assert_eq!(schema["type"], Value::String("array".into()), "schema type should be array");
    let items = &schema["items"];
    assert!(items["oneOf"].is_array(), "mixed array items should be oneOf");
    let types: Vec<&str> = items["oneOf"].as_array().unwrap().iter()
        .filter_map(|t| t["type"].as_str())
        .collect();
    assert!(types.contains(&"integer"), "mixed array should contain integer");
    assert!(types.contains(&"string"), "mixed array should contain string");
    assert!(types.contains(&"boolean"), "mixed array should contain boolean");
}
#[test]
fn verify_gate_schema_nested() {
    let output = Command::cargo_bin("jqr")
        .expect("binary built")
        .arg(".")
        .write_stdin(r#"{"user":{"name":"Bob"}}"#)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: Value = serde_json::from_slice(&output).expect("valid JSON");
    let schema = &v["schema"];
    assert!(schema.is_object(), "schema should be an object");
    assert_eq!(schema["type"], Value::String("object".into()), "schema type should be object");
    let user = &schema["properties"]["user"];
    assert_eq!(user["type"], Value::String("object".into()), "nested user should be an object");
    assert_eq!(user["properties"]["name"]["type"], Value::String("string".into()), "nested name should be string");
}
#[test]
fn verify_gate_schema_empty_object() {
    let output = Command::cargo_bin("jqr")
        .expect("binary built")
        .arg(".")
        .write_stdin("{}")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: Value = serde_json::from_slice(&output).expect("valid JSON");
    let schema = &v["schema"];
    assert!(schema.is_object(), "schema should be an object");
    assert_eq!(schema["type"], Value::String("object".into()), "schema type should be object");
    assert!(schema["properties"].as_object().unwrap().is_empty(), "empty object schema should have no keys");
}
#[test]
fn verify_gate_schema_float() {
    let output = Command::cargo_bin("jqr")
        .expect("binary built")
        .arg(".")
        .write_stdin(r#"{"price":9.99}"#)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: Value = serde_json::from_slice(&output).expect("valid JSON");
    let schema = &v["schema"];
    assert!(schema.is_object(), "schema should be an object");
    assert_eq!(schema["type"], Value::String("object".into()), "schema type should be object");
    assert_eq!(schema["properties"]["price"]["type"], Value::String("number".into()), "price should be number (float)");
}

// ================================================================
// Category 3: Token Count Accuracy
// ================================================================

#[test]
fn verify_gate_token_positive() {
    let output = Command::cargo_bin("jqr")
        .expect("binary built")
        .arg(".")
        .write_stdin(r#"{"data":[1,2,3,4,5]}"#)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: Value = serde_json::from_slice(&output).expect("valid JSON");
    let tokens = v["tokens_used"].as_u64().expect("tokens_used should be a number");
    assert!(tokens > 0, "non-empty input must have positive token count, got {tokens}");
}
#[test]
fn verify_gate_token_zero_empty() {
    // Identity filter on [] returns one result (the empty array itself).
    // tokens_used reflects the serialized sample, which is non-zero.
    let output = Command::cargo_bin("jqr")
        .expect("binary built")
        .arg(".")
        .write_stdin("[]")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: Value = serde_json::from_slice(&output).expect("valid JSON");
    let tokens = v["tokens_used"].as_u64().expect("tokens_used should be a number");
    assert!(tokens > 0, "single result (empty array) should have positive token count, got {tokens}");
}
#[test]
fn verify_gate_token_monotonic() {
    let small_out = Command::cargo_bin("jqr")
        .expect("binary built")
        .arg(".")
        .write_stdin(r#"{"a":1}"#)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let large_out = Command::cargo_bin("jqr")
        .expect("binary built")
        .arg(".")
        .write_stdin(r#"{"users":[{"id":1,"name":"Alice","email":"alice@example.com"},{"id":2,"name":"Bob","email":"bob@example.com"}]}"#)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let small_v: Value = serde_json::from_slice(&small_out).expect("valid JSON");
    let large_v: Value = serde_json::from_slice(&large_out).expect("valid JSON");
    let small_tokens = small_v["tokens_used"].as_u64().expect("tokens_used should be a number");
    let large_tokens = large_v["tokens_used"].as_u64().expect("tokens_used should be a number");
    assert!(large_tokens > small_tokens,
        "larger input ({large_tokens} tokens) should use more tokens than smaller ({small_tokens})");
}
#[test]
fn verify_gate_token_within_budget() {
    let output = Command::cargo_bin("jqr")
        .expect("binary built")
        .arg(".")
        .arg("--tokens")
        .arg("100")
        .write_stdin(r#"{"data":[1,2,3,4,5,6,7,8,9,10]}"#)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: Value = serde_json::from_slice(&output).expect("valid JSON");
    let tokens = v["tokens_used"].as_u64().expect("tokens_used should be a number");
    assert!(tokens <= 100, "tokens_used ({tokens}) must not exceed budget (100)");
}
#[test]
fn verify_gate_token_english_range() {
    let output = Command::cargo_bin("jqr")
        .expect("binary built")
        .arg(".")
        .write_stdin(r#"{"message":"hello world"}"#)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: Value = serde_json::from_slice(&output).expect("valid JSON");
    let tokens = v["tokens_used"].as_u64().expect("tokens_used should be a number");
    assert!(tokens >= 1, "small English input should have at least 1 token, got {tokens}");
    assert!(tokens <= 50, "small English input should have at most 50 tokens, got {tokens}");
}

// ================================================================
// Category 4: Format Detection Accuracy
// ================================================================

#[test]
fn verify_gate_format_json_object() {
    Command::cargo_bin("jqr")
        .expect("binary built")
        .arg(".")
        .write_stdin(r#"{"key":"value"}"#)
        .assert()
        .success()
        .stdout(predicate::str::contains("key"));
}
#[test]
fn verify_gate_format_json_array() {
    Command::cargo_bin("jqr")
        .expect("binary built")
        .arg(".")
        .write_stdin("[1,2,3]")
        .assert()
        .success()
        .stdout(predicate::str::contains("integer"));
}
#[test]
fn verify_gate_format_yaml() {
    Command::cargo_bin("jqr")
        .expect("binary built")
        .arg(".")
        .write_stdin("key: value")
        .assert()
        .success()
        .stdout(predicate::str::contains("key"));
}
#[test]
fn verify_gate_format_toml() {
    Command::cargo_bin("jqr")
        .expect("binary built")
        .arg(".")
        .write_stdin("[table]\nkey = \"value\"")
        .assert()
        .success()
        .stdout(predicate::str::contains("key"));
}
#[test]
fn verify_gate_format_csv() {
    Command::cargo_bin("jqr")
        .expect("binary built")
        .arg(".")
        .write_stdin("name,age\nAlice,30")
        .assert()
        .success()
        .stdout(predicate::str::contains("name"));
}

// ================================================================
// Category 5: Repair Accuracy
// ================================================================

#[test]
fn verify_gate_repair_prose() {
    let output = Command::cargo_bin("jqr")
        .expect("binary built")
        .arg(".")
        .arg("--repair")
        .write_stdin("Sure, here is the data:\n\n{\"name\": \"Alice\", \"age\": 30}\n\nLet me know if you need anything else!")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: Value = serde_json::from_slice(&output).expect("valid JSON");
    let sample = v.get("sample").or_else(|| v.get("value")).expect("envelope has sample or value");
    assert_eq!(sample["name"], Value::String("Alice".into()), "prose-wrapped repair should preserve name");
    assert_eq!(sample["age"], Value::Number(serde_json::Number::from(30)), "prose-wrapped repair should preserve age");
}
#[test]
fn verify_gate_repair_fence() {
    let output = Command::cargo_bin("jqr")
        .expect("binary built")
        .arg(".")
        .arg("--repair")
        .write_stdin("```json\n{\"key\": \"value\"}\n```")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: Value = serde_json::from_slice(&output).expect("valid JSON");
    let sample = v.get("sample").or_else(|| v.get("value")).expect("envelope has sample or value");
    assert_eq!(sample["key"], Value::String("value".into()), "fenced repair should preserve key");
}
#[test]
fn verify_gate_repair_trailing_comma() {
    let output = Command::cargo_bin("jqr")
        .expect("binary built")
        .arg(".")
        .arg("--repair")
        .write_stdin("{\"a\": 1, \"b\": 2,}")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: Value = serde_json::from_slice(&output).expect("valid JSON");
    let sample = v.get("sample").or_else(|| v.get("value")).expect("envelope has sample or value");
    assert_eq!(sample["a"], Value::Number(serde_json::Number::from(1)), "trailing comma repair should preserve a");
    assert_eq!(sample["b"], Value::Number(serde_json::Number::from(2)), "trailing comma repair should preserve b");
}
#[test]
fn verify_gate_repair_unclosed() {
    let output = Command::cargo_bin("jqr")
        .expect("binary built")
        .arg(".")
        .arg("--repair")
        .write_stdin(r#"{"users": [{"name": "Alice"}, {"name": "Bob""#)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: Value = serde_json::from_slice(&output).expect("valid JSON");
    let sample = v.get("sample").or_else(|| v.get("value")).expect("envelope has sample or value");
    assert_eq!(sample["users"][0]["name"], Value::String("Alice".into()), "unclosed repair should preserve first user");
}
#[test]
fn verify_gate_repair_combined() {
    let output = Command::cargo_bin("jqr")
        .expect("binary built")
        .arg(".")
        .arg("--repair")
        // Smart quotes are normalized to ASCII unconditionally.
        .write_stdin("Here's the data:\n```json\n{\"name\": \"\u{201c}Alice\u{201d}\", \"age\": 30,}\n```\nDone!")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: Value = serde_json::from_slice(&output).expect("valid JSON");
    let sample = v.get("sample").or_else(|| v.get("value")).expect("envelope has sample or value");
    assert_eq!(sample["age"], Value::Number(serde_json::Number::from(30)), "combined repair should preserve age");
    let name = sample["name"].as_str().expect("name should be a string");
    // Smart quotes inside strings are escaped to ASCII double quotes.
    assert_eq!(name, "\"Alice\"", "combined repair should escape smart quotes to ASCII, got '{name}'");
}
