use crate::error::JqrError;

/// Stateless JSON repairer that fixes common LLM output issues.
///
/// Applies five strategies in order to transform malformed JSON-like text
/// into something that `serde_json` can parse.
pub struct JsonRepairer;

impl Default for JsonRepairer {
    fn default() -> Self {
        JsonRepairer
    }
}

impl JsonRepairer {
    /// Creates a new `JsonRepairer`.
    pub fn new() -> Self {
        JsonRepairer
    }

    /// Repairs malformed JSON by applying all repair strategies in order.
    ///
    /// # Strategies (applied in order)
    ///
    /// 1. Strip markdown code fences (` ```json ` / ` ``` `)
    /// 2. Strip prose wrappers (find first `{`/`[` → last `}`/`]`)
    /// 3. Normalize single-quoted JSON to double-quoted
    /// 4. Normalize Unicode smart quotes to ASCII
    /// 5. Remove trailing commas before `}` or `]`
    /// 6. Close unclosed braces, brackets, and strings
    ///
    /// # Errors
    ///
    /// Returns `JqrError::Repair` if the input is empty.
    pub fn repair(&self, input: &str) -> Result<String, JqrError> {
        if input.is_empty() {
            return Err(JqrError::Repair("empty input".into()));
        }

        let mut text = strip_markdown_fences(input);
        text = strip_prose_wrappers(&text);
        text = normalize_single_quotes(&text);
        text = normalize_smart_quotes(&text);
        text = remove_trailing_commas(&text);
        text = close_unclosed_braces(&text);

        Ok(text)
    }
}

/// Removes lines that are exactly ` ```json ` or ` ``` ` (code fence markers).
///
/// Each line is trimmed before comparison. All other lines are preserved.
/// The final result is trimmed of leading/trailing whitespace.
fn strip_markdown_fences(text: &str) -> String {
    let lines: Vec<&str> = text
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            trimmed != "```json" && trimmed != "```"
        })
        .collect();
    let result = lines.join("\n");
    result.trim().to_string()
}

/// Finds the first `{` or `[` and the last `}` or `]`, then extracts the
/// substring between them (inclusive). If either is not found, returns the
/// original text unchanged.
fn strip_prose_wrappers(text: &str) -> String {
    let first_brace = text.find(['{', '[']);
    let last_brace = text.rfind(['}', ']']);

    match (first_brace, last_brace) {
        (Some(start), Some(end)) => {
            if start <= end {
                text[start..=end].trim().to_string()
            } else {
                text.to_string()
            }
        }
        (Some(_), None) | (None, Some(_)) | (None, None) => text.to_string(),
    }
}

/// Converts single-quoted JSON keys and values to double-quoted.
///
/// Uses a character-by-character state machine: when inside a double-quoted
/// string (delimited by ASCII `"` or Unicode smart quotes U+201C/U+201D),
/// characters pass through unchanged. When outside, single quotes (`'`) are
/// replaced with double quotes (`"`). This handles `{'key':'value'}` and
/// `['a','b']` while preserving apostrophes inside double-quoted strings
/// like `"don't"`.
fn normalize_single_quotes(text: &str) -> String {
    let mut in_string = false;
    let mut escape_next = false;

    text.chars()
        .map(|ch| {
            if escape_next {
                escape_next = false;
                return ch;
            }
            if in_string {
                match ch {
                    '\\' => escape_next = true,
                    '"' | '\u{201c}' | '\u{201d}' => in_string = false,
                    _ => {}
                }
                return ch;
            }
            match ch {
                '"' | '\u{201c}' | '\u{201d}' => {
                    in_string = true;
                    ch
                }
                '\'' => '"',
                other => other,
            }
        })
        .collect()
}

/// Replaces Unicode smart quotes and dashes with their ASCII equivalents.
///
/// | Unicode | Character          | Replacement |
/// |---------|--------------------|-------------|
/// | U+201C  | Left double quote  | `"`         |
/// | U+201D  | Right double quote | `"`         |
/// | U+2018  | Left single quote  | `'`         |
/// | U+2019  | Right single quote | `'`         |
/// | U+2013  | En dash            | `-`         |
/// | U+2014  | Em dash            | `-`         |
///
/// Smart quotes are always replaced. Inside JSON strings, smart double quotes
/// are escaped (`\"`) to keep the JSON valid. Outside strings, they become
/// plain ASCII `"`.
fn normalize_smart_quotes(text: &str) -> String {
    let mut in_string = false;
    let mut escape_next = false;
    let mut result = String::with_capacity(text.len());

    for ch in text.chars() {
        if escape_next {
            escape_next = false;
            result.push(ch);
            continue;
        }
        if in_string {
            match ch {
                '\\' => {
                    escape_next = true;
                    result.push(ch);
                }
                '"' => {
                    in_string = false;
                    result.push(ch);
                }
                '\u{201c}' | '\u{201d}' => {
                    result.push('\\');
                    result.push('"');
                }
                '\u{2018}' | '\u{2019}' => result.push('\''),
                '\u{2013}' | '\u{2014}' => result.push('-'),
                other => result.push(other),
            }
        } else {
            match ch {
                '"' => {
                    in_string = true;
                    result.push(ch);
                }
                '\u{201c}' | '\u{201d}' => result.push('"'),
                '\u{2018}' | '\u{2019}' => result.push('\''),
                '\u{2013}' | '\u{2014}' => result.push('-'),
                other => result.push(other),
            }
        }
    }

    result
}

/// Removes commas that appear immediately before `}` or `]`, including when
/// whitespace separates the comma from the brace.
///
/// Uses a character-by-character scan: when a `,` is found, the scanner looks
/// ahead past whitespace. If the next non-whitespace character is `}` or `]`,
/// the comma is skipped.
fn remove_trailing_commas(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut result = String::with_capacity(text.len());
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == ',' {
            let mut j = i + 1;
            while j < chars.len() && chars[j].is_whitespace() {
                j += 1;
            }
            if j < chars.len() && (chars[j] == '}' || chars[j] == ']') {
                // Skip this comma — it's trailing before a closing brace.
                i += 1;
                continue;
            }
        }
        result.push(chars[i]);
        i += 1;
    }

    result
}

/// Counts unmatched opening braces (`{`) and brackets (`[`), then appends the
/// corresponding closing characters at the end of the string.
///
/// Braces and brackets inside JSON strings are ignored. The scanner tracks
/// whether it is currently inside a string by watching for unescaped `"`
/// characters. Backslash (`\`) escapes the next character inside a string.
///
/// Closing characters are appended in inside-out order: all missing `]` first,
/// then all missing `}`. If the input ends inside an unclosed string, a closing
/// `"` is appended. If the input ends with an unterminated escape sequence
/// (trailing backslash inside a string), a closing `"` is appended to terminate
/// the escape, followed by another `"` to close the string.
fn close_unclosed_braces(text: &str) -> String {
    let mut in_string = false;
    let mut escape_next = false;
    let mut open_braces: usize = 0;
    let mut open_brackets: usize = 0;

    for ch in text.chars() {
        if escape_next {
            escape_next = false;
            continue;
        }

        if in_string {
            match ch {
                '\\' => escape_next = true,
                '"' => in_string = false,
                _ => { /* character inside string — ignore */ }
            }
        } else {
            match ch {
                '"' => in_string = true,
                '{' => open_braces += 1,
                '}' => {
                    open_braces = open_braces.saturating_sub(1);
                }
                '[' => open_brackets += 1,
                ']' => {
                    open_brackets = open_brackets.saturating_sub(1);
                }
                _ => { /* regular character outside string — ignore */ }
            }
        }
    }

    let mut result = text.to_string();
    // Close unclosed string: if we ended inside a string, append closing quote.
    // If escape_next is true at EOF, the trailing backslash needs a quote to
    // close the escape, then another quote to close the string.
    if in_string {
        if escape_next {
            result.push('"');
        }
        result.push('"');
    }
    for _ in 0..open_brackets {
        result.push(']');
    }
    for _ in 0..open_braces {
        result.push('}');
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_repair_valid_json() {
        let repairer = JsonRepairer::new();
        let input = r#"{"key": "value"}"#;
        let result = repairer.repair(input).unwrap();
        assert_eq!(result, input);
    }

    #[test]
    fn test_repair_prose_wrapped() {
        let repairer = JsonRepairer::new();
        let input = r#"Here is the data: {"key": "value"}"#;
        let result = repairer.repair(input).unwrap();
        assert_eq!(result, r#"{"key": "value"}"#);
    }

    #[test]
    fn test_repair_markdown_fence() {
        let repairer = JsonRepairer::new();
        let input = "```json\n{\"key\": \"value\"}\n```";
        let result = repairer.repair(input).unwrap();
        assert_eq!(result, r#"{"key": "value"}"#);
    }

    #[test]
    fn test_repair_trailing_comma() {
        let repairer = JsonRepairer::new();
        let input = r#"{"key": "value",}"#;
        let result = repairer.repair(input).unwrap();
        assert_eq!(result, r#"{"key": "value"}"#);
    }

    #[test]
    fn test_repair_trailing_comma_array() {
        let repairer = JsonRepairer::new();
        let input = "[1, 2, 3,]";
        let result = repairer.repair(input).unwrap();
        assert_eq!(result, "[1, 2, 3]");
    }

    #[test]
    fn test_repair_smart_quotes() {
        let repairer = JsonRepairer::new();
        // Smart quotes inside strings are escaped to keep JSON valid.
        let input = "{\"key\": \"\u{201c}hello\u{201d}\"}";
        let result = repairer.repair(input).unwrap();
        assert_eq!(result, "{\"key\": \"\\\"hello\\\"\"}");
    }

    #[test]
    fn test_repair_unclosed_braces() {
        let repairer = JsonRepairer::new();
        let input = r#"{"key": [1, 2"#;
        let result = repairer.repair(input).unwrap();
        assert_eq!(result, r#"{"key": [1, 2]}"#);
    }

    #[test]
    fn test_repair_empty_input() {
        let repairer = JsonRepairer::new();
        let result = repairer.repair("");
        match result {
            Err(JqrError::Repair(msg)) => {
                assert!(
                    msg.contains("empty"),
                    "expected 'empty' in message, got: {msg}"
                );
            }
            Err(other) => panic!("expected JqrError::Repair, got {other:?}"),
            Ok(val) => panic!("expected Err, got Ok({val:?})"),
        }
    }

    #[test]
    fn test_repair_nested_truncation() {
        let repairer = JsonRepairer::new();
        let input = r#"{"a": {"b": {"c":"#;
        let result = repairer.repair(input).unwrap();
        assert_eq!(result, r#"{"a": {"b": {"c":}}}"#);
    }

    #[test]
    fn test_repair_unicode_in_strings() {
        let repairer = JsonRepairer::new();
        let input = r#"{"key": "日本語"}"#;
        let result = repairer.repair(input).unwrap();
        assert_eq!(result, input);
    }

    #[test]
    fn test_repair_combined() {
        let repairer = JsonRepairer::new();
        // Smart quotes inside strings are escaped to keep JSON valid.
        let input = "```json\nHere is data: {\"name\": \"\u{201c}hello\u{201d}\", \"items\": [1, 2, 3,]\n```";
        let result = repairer.repair(input).unwrap();
        assert_eq!(
            result,
            "{\"name\": \"\\\"hello\\\"\", \"items\": [1, 2, 3]}"
        );
    }

    #[test]
    fn test_repair_smart_quotes_inside_string() {
        let repairer = JsonRepairer::new();
        let input = "{\"key\": \"\u{201c}hello\u{201d}\"}";
        let result = repairer.repair(input).unwrap();
        assert_eq!(result, "{\"key\": \"\\\"hello\\\"\"}");
    }

    #[test]
    fn test_repair_single_quoted_object() {
        let repairer = JsonRepairer::new();
        let input = "{'name':'Alice','age':30}";
        let result = repairer.repair(input).unwrap();
        assert_eq!(result, "{\"name\":\"Alice\",\"age\":30}");
    }

    #[test]
    fn test_repair_single_quoted_array() {
        let repairer = JsonRepairer::new();
        let input = "['a','b','c']";
        let result = repairer.repair(input).unwrap();
        assert_eq!(result, "[\"a\",\"b\",\"c\"]");
    }

    #[test]
    fn test_repair_apostrophe_in_double_quoted_string() {
        let repairer = JsonRepairer::new();
        let input = "{\"text\":\"don't\"}";
        let result = repairer.repair(input).unwrap();
        assert_eq!(result, "{\"text\":\"don't\"}");
    }

    #[test]
    fn test_repair_unclosed_string() {
        let repairer = JsonRepairer::new();
        let input = "{\"name\":\"Alice";
        let result = repairer.repair(input).unwrap();
        assert_eq!(result, "{\"name\":\"Alice\"}");
    }

    #[test]
    fn test_repair_unclosed_string_with_escape() {
        let repairer = JsonRepairer::new();
        // Two literal backslashes: first is escape, second is escaped char.
        // At EOF: in_string=true, escape_next=false → append one closing quote.
        let input = "{\"path\":\"C:\\\\";
        let result = repairer.repair(input).unwrap();
        assert_eq!(result, "{\"path\":\"C:\\\\\"}");
    }
}
