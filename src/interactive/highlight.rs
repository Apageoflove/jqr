use crossterm::style::Color;

const PRIMARY: Color = Color::Rgb {
    r: 84,
    g: 184,
    b: 171,
};
const SECONDARY: Color = Color::Rgb {
    r: 230,
    g: 165,
    b: 100,
};
const MUTED: Color = Color::Rgb {
    r: 120,
    g: 128,
    b: 138,
};
const BRIGHT: Color = Color::Rgb {
    r: 240,
    g: 243,
    b: 248,
};
const SEPARATOR: Color = Color::Rgb {
    r: 48,
    g: 54,
    b: 61,
};

/// A segment of highlighted JSON text with an associated color.
#[derive(Debug, Clone, PartialEq)]
pub struct HighlightSegment {
    pub text: String,
    pub color: Color,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Container {
    Object,
    Array,
}

/// Highlight JSON text, returning a vector of colored segments.
///
/// Uses a char-by-char state machine that tracks:
/// - Whether we are inside a string (`in_string`)
/// - Whether the next `\` starts an escape sequence (`escape_next`)
/// - The container stack (objects vs arrays) to determine key vs value context
/// - Whether the current position expects a key (`expecting_key`)
///
/// Handles nested objects/arrays, escaped quotes, CJK characters,
/// and gracefully handles malformed input without panicking.
pub fn highlight_json(json: &str) -> Vec<HighlightSegment> {
    let mut segments: Vec<HighlightSegment> = Vec::new();
    let mut current_text = String::new();
    let mut current_color: Option<Color> = None;

    let mut chars = json.chars().peekable();
    let mut stack: Vec<Container> = Vec::new();
    let mut expecting_key = false;
    let mut in_string = false;
    let mut escape_next = false;

    // Emit accumulated text as a segment, then clear the buffer.
    macro_rules! flush {
        () => {
            if !current_text.is_empty() {
                if let Some(c) = current_color {
                    segments.push(HighlightSegment {
                        text: std::mem::take(&mut current_text),
                        color: c,
                    });
                } else {
                    current_text.clear();
                }
            }
        };
    }

    while let Some(ch) = chars.next() {
        // ── Inside a string ──────────────────────────────────────────
        if in_string {
            if escape_next {
                current_text.push(ch);
                escape_next = false;
                continue;
            }
            if ch == '\\' {
                current_text.push(ch);
                escape_next = true;
                continue;
            }
            if ch == '"' {
                // End of string — emit the full string (including quotes)
                // in the color determined when the string started.
                current_text.push(ch);
                flush!();
                in_string = false;
                continue;
            }
            current_text.push(ch);
            continue;
        }

        // ── Outside a string ─────────────────────────────────────────
        match ch {
            '{' => {
                flush!();
                stack.push(Container::Object);
                expecting_key = true;
                segments.push(HighlightSegment {
                    text: "{".to_string(),
                    color: SEPARATOR,
                });
                current_color = None;
            }
            '}' => {
                flush!();
                stack.pop();
                expecting_key = false;
                segments.push(HighlightSegment {
                    text: "}".to_string(),
                    color: SEPARATOR,
                });
                current_color = None;
            }
            '[' => {
                flush!();
                stack.push(Container::Array);
                expecting_key = false;
                segments.push(HighlightSegment {
                    text: "[".to_string(),
                    color: SEPARATOR,
                });
                current_color = None;
            }
            ']' => {
                flush!();
                stack.pop();
                expecting_key = false;
                segments.push(HighlightSegment {
                    text: "]".to_string(),
                    color: SEPARATOR,
                });
                current_color = None;
            }
            ':' => {
                flush!();
                expecting_key = false;
                segments.push(HighlightSegment {
                    text: ":".to_string(),
                    color: SEPARATOR,
                });
                current_color = None;
            }
            ',' => {
                flush!();
                // After a comma: in an object → expect a key; in an array → expect a value.
                expecting_key = matches!(stack.last(), Some(Container::Object));
                segments.push(HighlightSegment {
                    text: ",".to_string(),
                    color: SEPARATOR,
                });
                current_color = None;
            }
            '"' => {
                flush!();
                in_string = true;
                let string_color = if expecting_key { PRIMARY } else { SECONDARY };
                current_text.push('"');
                current_color = Some(string_color);
            }
            ch if ch.is_whitespace() => {
                if current_color != Some(BRIGHT) {
                    flush!();
                    current_color = Some(BRIGHT);
                }
                current_text.push(ch);
            }
            ch if ch == '-' || ch.is_ascii_digit() => {
                if current_color != Some(BRIGHT) {
                    flush!();
                    current_color = Some(BRIGHT);
                }
                current_text.push(ch);
                // Consume the rest of the number token (int, float, scientific).
                let mut last = ch;
                while let Some(&next) = chars.peek() {
                    let consume = match next {
                        d if d.is_ascii_digit() => true,
                        '.' => true,
                        'e' | 'E' => true,
                        // +/- only valid immediately after e/E in scientific notation
                        '+' | '-' => matches!(last, 'e' | 'E'),
                        _ => false,
                    };
                    if consume {
                        current_text.push(next);
                        chars.next();
                        last = next;
                    } else {
                        break;
                    }
                }
            }
            ch if ch == 't' || ch == 'f' || ch == 'n' => {
                if current_color != Some(MUTED) {
                    flush!();
                    current_color = Some(MUTED);
                }
                current_text.push(ch);
                // Consume the rest of the keyword (true, false, null).
                while let Some(&next) = chars.peek() {
                    if next.is_ascii_alphabetic() {
                        current_text.push(next);
                        chars.next();
                    } else {
                        break;
                    }
                }
            }
            _ => {
                // Catch-all for any unexpected character (malformed JSON).
                // Color as BRIGHT so it's visible but doesn't panic.
                if current_color != Some(BRIGHT) {
                    flush!();
                    current_color = Some(BRIGHT);
                }
                current_text.push(ch);
            }
        }
    }

    // Flush any remaining accumulated text.
    flush!();

    segments
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_highlight_empty_string() {
        let result = highlight_json("");
        assert!(result.is_empty());
    }

    #[test]
    fn test_highlight_simple_object() {
        let result = highlight_json(r#"{"name": "Alice"}"#);
        // Expected segments: {  "name"  :  " "  "Alice"  }
        // The space after ':' produces a BRIGHT segment.
        assert_eq!(result.len(), 6);

        assert_eq!(result[0].text, "{");
        assert_eq!(result[0].color, SEPARATOR);

        assert_eq!(result[1].text, r#""name""#);
        assert_eq!(result[1].color, PRIMARY);

        assert_eq!(result[2].text, ":");
        assert_eq!(result[2].color, SEPARATOR);

        assert_eq!(result[3].text, " ");
        assert_eq!(result[3].color, BRIGHT);

        assert_eq!(result[4].text, r#""Alice""#);
        assert_eq!(result[4].color, SECONDARY);

        assert_eq!(result[5].text, "}");
        assert_eq!(result[5].color, SEPARATOR);
    }

    #[test]
    fn test_highlight_nested_object() {
        let result = highlight_json(r#"{"user": {"name": "Bob"}}"#);
        // Both "user" and "name" should be keys (PRIMARY).
        let keys: Vec<_> = result.iter().filter(|s| s.color == PRIMARY).collect();
        assert_eq!(keys.len(), 2);
        assert_eq!(keys[0].text, r#""user""#);
        assert_eq!(keys[1].text, r#""name""#);

        // "Bob" should be a value (SECONDARY).
        let values: Vec<_> = result.iter().filter(|s| s.color == SECONDARY).collect();
        assert_eq!(values.len(), 1);
        assert_eq!(values[0].text, r#""Bob""#);
    }

    #[test]
    fn test_highlight_array() {
        let result = highlight_json(r#"["a", "b", "c"]"#);
        // All strings in an array are values (SECONDARY), not keys.
        let strings: Vec<_> = result.iter().filter(|s| s.color == SECONDARY).collect();
        assert_eq!(strings.len(), 3);
        assert_eq!(strings[0].text, r#""a""#);
        assert_eq!(strings[1].text, r#""b""#);
        assert_eq!(strings[2].text, r#""c""#);
    }

    #[test]
    fn test_highlight_numbers() {
        let result =
            highlight_json(r#"{"count": 42, "pi": 3.14, "neg": -17, "exp": 1.5e10}"#);
        let numbers: Vec<_> = result.iter().filter(|s| s.color == BRIGHT).collect();
        // Numbers: 42, 3.14, -17, 1.5e10 (whitespace may also be BRIGHT).
        let num_texts: Vec<&str> = numbers.iter().map(|s| s.text.as_str()).collect();
        assert!(
            num_texts.iter().any(|t| t.contains("42")),
            "Expected a BRIGHT segment containing '42', got: {:?}",
            num_texts
        );
        assert!(
            num_texts.iter().any(|t| t.contains("3.14")),
            "Expected a BRIGHT segment containing '3.14', got: {:?}",
            num_texts
        );
        assert!(
            num_texts.iter().any(|t| t.contains("-17")),
            "Expected a BRIGHT segment containing '-17', got: {:?}",
            num_texts
        );
        assert!(
            num_texts.iter().any(|t| t.contains("1.5e10")),
            "Expected a BRIGHT segment containing '1.5e10', got: {:?}",
            num_texts
        );
    }

    #[test]
    fn test_highlight_booleans_and_null() {
        let result = highlight_json(r#"{"a": true, "b": false, "c": null}"#);
        let muted: Vec<_> = result.iter().filter(|s| s.color == MUTED).collect();
        assert_eq!(muted.len(), 3);
        let muted_texts: Vec<&str> = muted.iter().map(|s| s.text.as_str()).collect();
        assert!(muted_texts.contains(&"true"));
        assert!(muted_texts.contains(&"false"));
        assert!(muted_texts.contains(&"null"));
    }

    #[test]
    fn test_highlight_escaped_quotes() {
        let result = highlight_json(r#"{"msg": "hello \"world\""}"#);
        // The value string should contain the escaped quotes intact.
        let values: Vec<_> = result.iter().filter(|s| s.color == SECONDARY).collect();
        assert_eq!(values.len(), 1);
        assert_eq!(values[0].text, r#""hello \"world\"""#);
    }

    #[test]
    fn test_highlight_cjk_strings() {
        let result = highlight_json(r#"{"zh": "中文", "ja": "日本語", "ko": "한국어"}"#);
        let keys: Vec<_> = result.iter().filter(|s| s.color == PRIMARY).collect();
        assert_eq!(keys.len(), 3);

        let values: Vec<_> = result.iter().filter(|s| s.color == SECONDARY).collect();
        assert_eq!(values.len(), 3);
        assert!(values.iter().any(|s| s.text.contains("中文")));
        assert!(values.iter().any(|s| s.text.contains("日本語")));
        assert!(values.iter().any(|s| s.text.contains("한국어")));
    }

    #[test]
    fn test_highlight_mixed_content() {
        let json = r#"{
  "name": "Alice",
  "age": 30,
  "active": true,
  "tags": ["dev", "ops"],
  "meta": null,
  "score": 3.14
}"#;
        let result = highlight_json(json);

        // Verify all color types appear.
        let has_primary = result.iter().any(|s| s.color == PRIMARY);
        let has_secondary = result.iter().any(|s| s.color == SECONDARY);
        let has_bright = result.iter().any(|s| s.color == BRIGHT);
        let has_muted = result.iter().any(|s| s.color == MUTED);
        let has_separator = result.iter().any(|s| s.color == SEPARATOR);

        assert!(has_primary, "Should have PRIMARY (key) segments");
        assert!(has_secondary, "Should have SECONDARY (string value) segments");
        assert!(has_bright, "Should have BRIGHT (number/whitespace) segments");
        assert!(has_muted, "Should have MUTED (bool/null) segments");
        assert!(has_separator, "Should have SEPARATOR segments");

        // Verify all expected keys are present.
        let key_texts: Vec<&str> = result
            .iter()
            .filter(|s| s.color == PRIMARY)
            .map(|s| s.text.as_str())
            .collect();
        for expected in &["name", "age", "active", "tags", "meta", "score"] {
            assert!(
                key_texts.iter().any(|t| t.contains(expected)),
                "Expected key containing '{}'",
                expected
            );
        }
    }

    #[test]
    fn test_highlight_malformed_json() {
        // None of these should panic. The state machine gracefully handles
        // unexpected input by coloring it BRIGHT.
        let _ = highlight_json(r#"{unquoted: value}"#);
        let _ = highlight_json(r#"{"a": }"#);
        let _ = highlight_json(r#"[1, 2,]"#);
        let _ = highlight_json(r#"{"a": 1,}"#);
        let _ = highlight_json(r#"plain text without json"#);
        let _ = highlight_json(r#"{"a": "unclosed"#);
        let _ = highlight_json(r#"{"a": 1.2.3}"#);
        let _ = highlight_json(r#"{"a": [1, 2}"#); // mismatched brackets
    }
}
