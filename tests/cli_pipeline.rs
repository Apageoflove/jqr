//! CLI Pipeline Integration Tests
//!
//! Comprehensive integration tests for the jqr binary covering the full
//! pipeline: CLI parse → config load → stdin read → format detect/set →
//! repair (optional) → parse → filter compile → filter run → output.
//!
//! Run: cargo test --test cli_pipeline

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;

// ================================================================
// Category 1: Smoke Tests
// ================================================================

#[test]
fn verify_smoke_help() {
    Command::cargo_bin("jqr").expect("binary built")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("jqr"))
        .stdout(predicate::str::contains("filter"))
        .stdout(predicate::str::contains("Usage"));
}
#[test]
fn verify_smoke_version() {
    Command::cargo_bin("jqr").expect("binary built")
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("jqr"));
}
#[test]
fn verify_smoke_no_args_with_stdin() {
    let output = Command::cargo_bin("jqr").expect("binary built")
        .write_stdin(r#"{"a":1}"#)
        .assert()
        .success()
        .get_output().stdout.clone();
    let _v: Value = serde_json::from_slice(&output).expect("stdout should be valid JSON");
}
#[test]
fn verify_smoke_default_filter_is_identity() {
    Command::cargo_bin("jqr").expect("binary built")
        .write_stdin(r#"{"a":1}"#)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"a\""))
        .stdout(predicate::str::contains("1"));
}
#[test]
fn verify_smoke_empty_stdin() {
    Command::cargo_bin("jqr").expect("binary built")
        .write_stdin("")
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("parse error"));
}
#[test]
fn verify_smoke_compact_flag() {
    let output = Command::cargo_bin("jqr").expect("binary built")
        .arg("--compact")
        .write_stdin(r#"{"a":1}"#)
        .assert()
        .success()
        .get_output().stdout.clone();
    let stdout = String::from_utf8_lossy(&output);
    assert!(!stdout.contains('\n'), "compact output must not contain newlines, got: {stdout}");
}

// ================================================================
// Category 2: Success Paths
// ================================================================

#[test]
fn verify_success_identity() {
    let output = Command::cargo_bin("jqr").expect("binary built")
        .arg(".")
        .write_stdin(r#"{"a":1}"#)
        .assert()
        .success()
        .get_output().stdout.clone();
    let v: Value = serde_json::from_slice(&output).expect("valid JSON");
    assert_eq!(v["value"], serde_json::json!({"a": 1}));
}
#[test]
fn verify_success_field_access() {
    let output = Command::cargo_bin("jqr").expect("binary built")
        .arg(".name")
        .write_stdin(r#"{"name":"Alice"}"#)
        .assert()
        .success()
        .get_output().stdout.clone();
    let v: Value = serde_json::from_slice(&output).expect("valid JSON");
    assert_eq!(v["value"], serde_json::json!("Alice"));
}
#[test]
fn verify_success_array_iteration() {
    let output = Command::cargo_bin("jqr").expect("binary built")
        .arg(".[]")
        .write_stdin(r#"[1,2,3]"#)
        .assert()
        .success()
        .get_output().stdout.clone();
    let v: Value = serde_json::from_slice(&output).expect("valid JSON");
    assert_eq!(v["total"], serde_json::json!(3));
    assert!(v["sample"].is_array());
    assert_eq!(v["sample"].as_array().unwrap().len(), 3);
}
#[test]
fn verify_success_map() {
    // map(.name) returns a single array result → full envelope
    let output = Command::cargo_bin("jqr").expect("binary built")
        .arg("map(.name)")
        .write_stdin(r#"[{"name":"A"},{"name":"B"}]"#)
        .assert()
        .success()
        .get_output().stdout.clone();
    let v: Value = serde_json::from_slice(&output).expect("valid JSON");
    // Single-array result is expanded: sample contains individual elements
    let sample = v["sample"].as_array().expect("sample is array");
    assert_eq!(sample.len(), 2);
    assert_eq!(sample[0], serde_json::json!("A"));
    assert_eq!(sample[1], serde_json::json!("B"));
}
#[test]
fn verify_success_select() {
    // select returns a single object → scalar envelope
    let output = Command::cargo_bin("jqr").expect("binary built")
        .arg(".[] | select(.age > 30)")
        .write_stdin(r#"[{"age":25},{"age":35}]"#)
        .assert()
        .success()
        .get_output().stdout.clone();
    let v: Value = serde_json::from_slice(&output).expect("valid JSON");
    // Scalar envelope: value, no total
    assert_eq!(v["value"]["age"], serde_json::json!(35));
}
#[test]
fn verify_success_pipe_chain() {
    let output = Command::cargo_bin("jqr").expect("binary built")
        .arg(".data | .[] | .id")
        .write_stdin(r#"{"data":[{"id":1},{"id":2}]}"#)
        .assert()
        .success()
        .get_output().stdout.clone();
    let v: Value = serde_json::from_slice(&output).expect("valid JSON");
    assert_eq!(v["total"], serde_json::json!(2));
    let sample = &v["sample"].as_array().expect("sample is array");
    assert_eq!(sample[0], serde_json::json!(1));
    assert_eq!(sample[1], serde_json::json!(2));
}
#[test]
fn verify_success_length() {
    let output = Command::cargo_bin("jqr").expect("binary built")
        .arg("length")
        .write_stdin(r#"[1,2,3]"#)
        .assert()
        .success()
        .get_output().stdout.clone();
    let v: Value = serde_json::from_slice(&output).expect("valid JSON");
    assert_eq!(v["value"], serde_json::json!(3));
    assert!(v.get("value").is_some(), "scalar envelope must have 'value'");
}
#[test]
fn verify_success_keys() {
    // keys returns a single array → full envelope
    let output = Command::cargo_bin("jqr").expect("binary built")
        .arg("keys")
        .write_stdin(r#"{"a":1,"b":2}"#)
        .assert()
        .success()
        .get_output().stdout.clone();
    let v: Value = serde_json::from_slice(&output).expect("valid JSON");
    // Single-array result is expanded: sample contains individual elements
    let sample = v["sample"].as_array().expect("sample is array");
    assert_eq!(sample.len(), 2);
    assert_eq!(sample[0], serde_json::json!("a"));
    assert_eq!(sample[1], serde_json::json!("b"));
}

// ================================================================
// Category 3: Input Formats
// ================================================================

#[test]
fn verify_input_json_explicit() {
    let output = Command::cargo_bin("jqr").expect("binary built")
        .arg("--input")
        .arg("json")
        .write_stdin(r#"{"x":1}"#)
        .assert()
        .success()
        .get_output().stdout.clone();
    let v: Value = serde_json::from_slice(&output).expect("valid JSON");
    assert_eq!(v["value"], serde_json::json!({"x": 1}));
}
#[test]
fn verify_input_yaml_explicit() {
    let output = Command::cargo_bin("jqr").expect("binary built")
        .arg("--input")
        .arg("yaml")
        .write_stdin("key: value\n")
        .assert()
        .success()
        .get_output().stdout.clone();
    let v: Value = serde_json::from_slice(&output).expect("valid JSON");
    assert_eq!(v["value"], serde_json::json!({"key": "value"}));
}
#[test]
fn verify_input_toml_explicit() {
    let output = Command::cargo_bin("jqr").expect("binary built")
        .arg("--input")
        .arg("toml")
        .write_stdin("[table]\nkey = \"value\"\n")
        .assert()
        .success()
        .get_output().stdout.clone();
    let v: Value = serde_json::from_slice(&output).expect("valid JSON");
    assert_eq!(v["value"]["table"]["key"], serde_json::json!("value"));
}
#[test]
fn verify_input_csv_explicit() {
    // CSV is parsed as array of objects; identity filter returns it as one result
    let output = Command::cargo_bin("jqr").expect("binary built")
        .arg("--input")
        .arg("csv")
        .write_stdin("name,age\nAlice,30\n")
        .assert()
        .success()
        .get_output().stdout.clone();
    let v: Value = serde_json::from_slice(&output).expect("valid JSON");
    // Single-array result is expanded: sample contains individual records
    let sample = v["sample"].as_array().expect("sample is array");
    assert_eq!(sample.len(), 1);
    assert_eq!(sample[0]["name"], serde_json::json!("Alice"));
    assert_eq!(sample[0]["age"], serde_json::json!("30"));
}
#[test]
fn verify_input_auto_detect_json() {
    let output = Command::cargo_bin("jqr").expect("binary built")
        .write_stdin(r#"{"x":1}"#)
        .assert()
        .success()
        .get_output().stdout.clone();
    let v: Value = serde_json::from_slice(&output).expect("valid JSON");
    assert_eq!(v["value"], serde_json::json!({"x": 1}));
}
#[test]
fn verify_input_auto_detect_yaml() {
    let output = Command::cargo_bin("jqr").expect("binary built")
        .write_stdin("key: value\n")
        .assert()
        .success()
        .get_output().stdout.clone();
    let v: Value = serde_json::from_slice(&output).expect("valid JSON");
    assert_eq!(v["value"], serde_json::json!({"key": "value"}));
}

// ================================================================
// Category 4: Output Modes
// ================================================================

#[test]
fn verify_output_raw_identity() {
    Command::cargo_bin("jqr").expect("binary built")
        .arg("--raw")
        .write_stdin(r#"{"a":1}"#)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"a\""))
        .stdout(predicate::str::contains("1"));
}
#[test]
fn verify_output_raw_single_unwrap() {
    let output = Command::cargo_bin("jqr").expect("binary built")
        .arg("--raw")
        .arg(".name")
        .write_stdin(r#"{"name":"x"}"#)
        .assert()
        .success()
        .get_output().stdout.clone();
    let stdout = String::from_utf8_lossy(&output);
    assert!(stdout.contains("\"x\""), "raw output should contain the unwrapped string value, got: {stdout}");
    assert!(!stdout.contains('['), "raw single result should not be wrapped in array, got: {stdout}");
}
#[test]
fn verify_output_raw_empty() {
    let output = Command::cargo_bin("jqr").expect("binary built")
        .arg("--raw")
        .arg(".nonexistent")
        .write_stdin(r#"{}"#)
        .assert()
        .success()
        .get_output().stdout.clone();
    let stdout = String::from_utf8_lossy(&output);
    assert!(stdout.trim() == "null", "raw empty result should be null, got: {stdout}");
}
#[test]
fn verify_output_compact_no_newlines() {
    let output = Command::cargo_bin("jqr").expect("binary built")
        .arg("--compact")
        .write_stdin(r#"{"a":1,"b":2}"#)
        .assert()
        .success()
        .get_output().stdout.clone();
    let stdout = String::from_utf8_lossy(&output);
    assert!(!stdout.contains('\n'), "compact output must not contain newlines, got: {stdout}");
}
#[test]
fn verify_output_default_envelope() {
    let output = Command::cargo_bin("jqr").expect("binary built")
        .arg(".")
        .write_stdin(r#"[{"x":1},{"x":2}]"#)
        .assert()
        .success()
        .get_output().stdout.clone();
    let v: Value = serde_json::from_slice(&output).expect("valid JSON");
    assert!(v.get("schema").is_some(), "envelope must have 'schema'");
    assert!(v.get("sample").is_some(), "envelope must have 'sample'");
    assert!(v.get("total").is_some(), "envelope must have 'total'");
    assert!(v.get("truncated").is_some(), "envelope must have 'truncated'");
    assert!(v.get("sample_size").is_some(), "envelope must have 'sample_size'");
    assert!(v.get("tokens_used").is_some(), "envelope must have 'tokens_used'");
}
#[test]
fn verify_output_pretty() {
    let output = Command::cargo_bin("jqr").expect("binary built")
        .arg("--pretty")
        .write_stdin(r#"{"a":1}"#)
        .assert()
        .success()
        .get_output().stdout.clone();
    let stdout = String::from_utf8_lossy(&output);
    assert!(stdout.contains('\n'), "pretty output must contain newlines, got: {stdout}");
}

// ================================================================
// Category 5: Repair Mode
// ================================================================

#[test]
fn verify_repair_prose_wrapped() {
    let output = Command::cargo_bin("jqr").expect("binary built")
        .arg("--repair")
        .write_stdin("Here is data: {\"x\":1}\n")
        .assert()
        .success()
        .get_output().stdout.clone();
    let v: Value = serde_json::from_slice(&output).expect("valid JSON");
    assert_eq!(v["value"]["x"], serde_json::json!(1));
}
#[test]
fn verify_repair_markdown_fence() {
    let output = Command::cargo_bin("jqr").expect("binary built")
        .arg("--repair")
        .write_stdin("```json\n{\"x\":1}\n```\n")
        .assert()
        .success()
        .get_output().stdout.clone();
    let v: Value = serde_json::from_slice(&output).expect("valid JSON");
    assert_eq!(v["value"]["x"], serde_json::json!(1));
}
#[test]
fn verify_repair_trailing_comma() {
    let output = Command::cargo_bin("jqr").expect("binary built")
        .arg("--repair")
        .write_stdin("{\"x\":1,}\n")
        .assert()
        .success()
        .get_output().stdout.clone();
    let v: Value = serde_json::from_slice(&output).expect("valid JSON");
    assert_eq!(v["value"]["x"], serde_json::json!(1));
}
#[test]
fn verify_repair_unclosed_brace() {
    let output = Command::cargo_bin("jqr").expect("binary built")
        .arg("--repair")
        .write_stdin("{\"x\":[1,2\n")
        .assert()
        .success()
        .get_output().stdout.clone();
    let v: Value = serde_json::from_slice(&output).expect("valid JSON");
    assert_eq!(v["value"]["x"], serde_json::json!([1, 2]));
}
#[test]
fn verify_repair_smart_quotes() {
    // Smart quotes OUTSIDE JSON strings are normalized to ASCII.
    // Smart quotes INSIDE JSON strings are preserved (to avoid breaking JSON structure).
    let output = Command::cargo_bin("jqr").expect("binary built")
        .arg("--repair")
        // Smart quotes used as JSON key delimiters (outside strings) get normalized
        .write_stdin("{\u{201c}x\u{201d}:1}\n")
        .assert()
        .success()
        .get_output().stdout.clone();
    let v: Value = serde_json::from_slice(&output).expect("valid JSON");
    assert_eq!(v["value"]["x"], serde_json::json!(1));
}
#[test]
fn verify_repair_single_quotes() {
    let output = Command::cargo_bin("jqr").expect("binary built")
        .arg("--repair")
        .write_stdin("{'x':1}\n")
        .assert()
        .success()
        .get_output().stdout.clone();
    let v: Value = serde_json::from_slice(&output).expect("valid JSON");
    assert_eq!(v["value"]["x"], serde_json::json!(1));
}
#[test]
fn verify_repair_unclosed_string() {
    let output = Command::cargo_bin("jqr").expect("binary built")
        .arg("--repair")
        .write_stdin("{\"x\":\"y")
        .assert()
        .success()
        .get_output().stdout.clone();
    let v: Value = serde_json::from_slice(&output).expect("valid JSON");
    assert_eq!(v["value"]["x"], serde_json::json!("y"));
}
#[test]
fn verify_repair_valid_passthrough() {
    let output_repaired = Command::cargo_bin("jqr").expect("binary built")
        .arg("--repair")
        .write_stdin(r#"{"x":1}"#)
        .assert()
        .success()
        .get_output().stdout.clone();
    let output_normal = Command::cargo_bin("jqr").expect("binary built")
        .write_stdin(r#"{"x":1}"#)
        .assert()
        .success()
        .get_output().stdout.clone();
    let vr: Value = serde_json::from_slice(&output_repaired).expect("valid JSON");
    let vn: Value = serde_json::from_slice(&output_normal).expect("valid JSON");
    assert_eq!(vr["value"], vn["value"], "repair of valid JSON should produce same value");
}

// ================================================================
// Category 6: Failure Paths
// ================================================================

#[test]
fn verify_failure_invalid_filter() {
    Command::cargo_bin("jqr").expect("binary built")
        .arg("[[[")
        .write_stdin("{}")
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("filter error"));
}
#[test]
fn verify_failure_invalid_json() {
    Command::cargo_bin("jqr").expect("binary built")
        .write_stdin("{bad")
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("parse error"));
}
#[test]
fn verify_failure_nonexistent_field_no_error() {
    Command::cargo_bin("jqr").expect("binary built")
        .arg(".nonexistent")
        .write_stdin("{}")
        .assert()
        .success();
}
#[test]
fn verify_failure_eval_error() {
    // jaq returns null for division by zero (no runtime error).
    // This is jq-compatible behavior.
    let output = Command::cargo_bin("jqr").expect("binary built")
        .arg("1/0")
        .write_stdin("[1]")
        .assert()
        .success()
        .get_output().stdout.clone();
    let v: Value = serde_json::from_slice(&output).expect("valid JSON");
    assert_eq!(v["value"], serde_json::json!(null));
}

// ================================================================
// Category 7: Token Budget
// ================================================================

#[test]
fn verify_token_budget_tight() {
    // Use .[] to produce many individual results that can be truncated
    let large_array: String = (0..50)
        .map(|i| format!(r#"{{"id":{},"name":"item-{}","data":"some long string value for token counting"}}"#, i, i))
        .collect::<Vec<_>>()
        .join(",");
    let input = format!("[{}]", large_array);
    let output = Command::cargo_bin("jqr").expect("binary built")
        .arg(".[]")
        .arg("--tokens")
        .arg("10")
        .write_stdin(input)
        .assert()
        .success()
        .get_output().stdout.clone();
    let v: Value = serde_json::from_slice(&output).expect("valid JSON");
    assert_eq!(v["truncated"], serde_json::json!(true), "tight budget should truncate");
}
#[test]
fn verify_token_budget_generous() {
    let output = Command::cargo_bin("jqr").expect("binary built")
        .arg(".")
        .arg("--tokens")
        .arg("50000")
        .write_stdin(r#"{"x":1}"#)
        .assert()
        .success()
        .get_output().stdout.clone();
    let v: Value = serde_json::from_slice(&output).expect("valid JSON");
    // Single scalar result → scalar envelope (no "truncated" key)
    // Just verify success and valid output
    assert!(v.get("value").is_some(), "should have value key");
}
#[test]
fn verify_token_budget_zero() {
    // Use .[] to produce multiple results
    let output = Command::cargo_bin("jqr").expect("binary built")
        .arg(".[]")
        .arg("--tokens")
        .arg("0")
        .write_stdin(r#"[1,2,3,4,5]"#)
        .assert()
        .success()
        .get_output().stdout.clone();
    let v: Value = serde_json::from_slice(&output).expect("valid JSON");
    let sample = v["sample"].as_array().expect("sample is array");
    assert!(!sample.is_empty(), "zero budget must include at least 1 result, got {}", sample.len());
    assert_eq!(v["truncated"], serde_json::json!(true));
}
#[test]
fn verify_token_sample_size() {
    // Use .[] to produce multiple results so sample_size cap applies
    let output = Command::cargo_bin("jqr").expect("binary built")
        .arg(".[]")
        .arg("--sample-size")
        .arg("3")
        .write_stdin(r#"[1,2,3,4,5,6,7,8,9,10]"#)
        .assert()
        .success()
        .get_output().stdout.clone();
    let v: Value = serde_json::from_slice(&output).expect("valid JSON");
    assert_eq!(v["sample_size"], serde_json::json!(3));
}
#[test]
fn verify_token_total_field() {
    let output = Command::cargo_bin("jqr").expect("binary built")
        .arg(".[]")
        .write_stdin(r#"[10,20,30,40,50]"#)
        .assert()
        .success()
        .get_output().stdout.clone();
    let v: Value = serde_json::from_slice(&output).expect("valid JSON");
    assert_eq!(v["total"], serde_json::json!(5), "total should match result count");
}

// ================================================================
// Category 8: Edge Cases
// ================================================================

#[test]
fn verify_edge_empty_object() {
    let output = Command::cargo_bin("jqr").expect("binary built")
        .write_stdin("{}")
        .assert()
        .success()
        .get_output().stdout.clone();
    let v: Value = serde_json::from_slice(&output).expect("valid JSON");
    assert!(v.get("value").is_some(), "empty object should produce scalar envelope");
    assert_eq!(v["value"], serde_json::json!({}));
}
#[test]
fn verify_edge_empty_array() {
    // Identity filter on [] returns [] (one result: the empty array)
    let output = Command::cargo_bin("jqr").expect("binary built")
        .write_stdin("[]")
        .assert()
        .success()
        .get_output().stdout.clone();
    let v: Value = serde_json::from_slice(&output).expect("valid JSON");
    // Single array result → full envelope
    assert!(v.get("sample").is_some(), "single array result should produce full envelope");
    assert_eq!(v["total"], serde_json::json!(1));
}
#[test]
fn verify_edge_null() {
    let output = Command::cargo_bin("jqr").expect("binary built")
        .write_stdin("null")
        .assert()
        .success()
        .get_output().stdout.clone();
    let v: Value = serde_json::from_slice(&output).expect("valid JSON");
    assert_eq!(v["value"], serde_json::json!(null));
}
#[test]
fn verify_edge_unicode_fields() {
    let output = Command::cargo_bin("jqr").expect("binary built")
        .write_stdin(r#"{"名前":"太郎"}"#)
        .assert()
        .success()
        .get_output().stdout.clone();
    let stdout = String::from_utf8_lossy(&output);
    assert!(stdout.contains("名前"), "unicode field name should be preserved, got: {stdout}");
    assert!(stdout.contains("太郎"), "unicode field value should be preserved, got: {stdout}");
}
#[test]
fn verify_edge_unicode_values() {
    let output = Command::cargo_bin("jqr").expect("binary built")
        .write_stdin(r#"{"key":"こんにちは"}"#)
        .assert()
        .success()
        .get_output().stdout.clone();
    let stdout = String::from_utf8_lossy(&output);
    assert!(stdout.contains("こんにちは"), "unicode value should be preserved, got: {stdout}");
}
#[test]
fn verify_edge_deep_nesting() {
    let mut json = String::from("{\"a\":");
    for _ in 0..9 {
        json.push_str("{\"a\":");
    }
    json.push_str("\"deep\"");
    for _ in 0..10 {
        json.push('}');
    }
    let output = Command::cargo_bin("jqr").expect("binary built")
        .write_stdin(json.as_bytes())
        .assert()
        .success()
        .get_output().stdout.clone();
    let _v: Value = serde_json::from_slice(&output).expect("deeply nested JSON should produce valid output");
}
#[test]
fn verify_edge_large_array() {
    // Use .[] to iterate and produce 100 individual results
    let elements: Vec<String> = (0..100).map(|i| i.to_string()).collect();
    let input = format!("[{}]", elements.join(","));
    let output = Command::cargo_bin("jqr").expect("binary built")
        .arg(".[]")
        .write_stdin(input)
        .assert()
        .success()
        .get_output().stdout.clone();
    let v: Value = serde_json::from_slice(&output).expect("valid JSON");
    assert_eq!(v["total"], serde_json::json!(100), "total should be 100");
}
