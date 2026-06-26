use std::io::Read;

use serde_json::Value;

use crate::error::JqrError;

/// The input format for deserializing raw bytes into [`serde_json::Value`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InputFormat {
    /// Try every supported format in order; stop at the first success.
    Auto,
    /// JSON (RFC 8259).
    Json,
    /// YAML 1.2.
    Yaml,
    /// TOML v1.0.
    Toml,
    /// CSV (RFC 4180).
    Csv,
}

/// Reads raw bytes from stdin, auto-detects the serialization format, and
/// deserializes into a [`serde_json::Value`].
pub struct InputReader {
    /// The format to use for parsing.  `Auto` triggers detection on first use.
    pub format: InputFormat,
    content: String,
}

impl InputReader {
    /// Read all bytes from [`std::io::stdin`] and return a reader whose
    /// [`format`](InputReader::format) is [`InputFormat::Auto`].
    ///
    /// # Errors
    ///
    /// Returns [`JqrError::Io`] when stdin cannot be read.
    pub fn from_stdin() -> Result<Self, JqrError> {
        let mut content = String::new();
        std::io::stdin().read_to_string(&mut content)?;
        Ok(InputReader {
            format: InputFormat::Auto,
            content,
        })
    }

    /// Create a reader from a string (for testing and programmatic use).
    /// The format is set to [`InputFormat::Auto`].
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(content: &str) -> Self {
        InputReader {
            format: InputFormat::Auto,
            content: content.to_string(),
        }
    }

    /// Inspect `self.content` and set [`self.format`](InputReader::format) to
    /// the best guess when it is currently [`InputFormat::Auto`].  If the
    /// format was already set explicitly this method is a no-op.
    pub fn detect(&mut self) {
        if self.format != InputFormat::Auto {
            return;
        }

        let trimmed = self.content.trim_start();

        match trimmed.chars().next() {
            Some('{') => {
                self.format = InputFormat::Json;
                return;
            }
            Some('[') => {
                // `[` can open a JSON array *or* a TOML table header.
                // If the content also carries TOML-style `key = value` lines
                // we treat it as TOML; otherwise it is JSON.
                if self.has_toml_pattern() {
                    self.format = InputFormat::Toml;
                } else {
                    self.format = InputFormat::Json;
                }
                return;
            }
            _ => { /* keep probing */ }
        }

        // YAML document separator.
        if self.content.lines().any(|line| line.trim() == "---") {
            self.format = InputFormat::Yaml;
            return;
        }

        // YAML key-value pairs (`key: value`).
        if self.has_yaml_pattern() {
            self.format = InputFormat::Yaml;
            return;
        }

        // TOML key-value pairs (`key = value`).
        if self.has_toml_pattern() {
            self.format = InputFormat::Toml;
            return;
        }

        // CSV: every non-empty line has the same positive number of commas.
        if self.has_csv_pattern() {
            self.format = InputFormat::Csv;
            return;
        }

        // Fallback — treat as JSON and let the parser produce a useful error
        // when the content is genuinely unparseable.
        self.format = InputFormat::Json;
    }

    /// Override the input format explicitly.
    ///
    /// After calling this method, [`detect`](InputReader::detect) becomes a
    /// no-op and [`parse`](InputReader::parse) uses the given format directly.
    pub fn set_format(&mut self, format: InputFormat) {
        self.format = format;
    }

    /// Return a reference to the raw input content.
    pub fn content(&self) -> &str {
        &self.content
    }

    /// Deserialize `self.content` according to [`self.format`](InputReader::format).
    ///
    /// When the format is [`InputFormat::Auto`] each supported format is tried
    /// in order (JSON → YAML → TOML → CSV) and the first successful parse is
    /// returned.
    ///
    /// # Errors
    ///
    /// Returns a [`JqrError`] variant appropriate for the attempted format, or
    /// [`JqrError::UnsupportedFormat`] when auto-detection exhausts every
    /// parser without success.
    pub fn parse(&self) -> Result<Value, JqrError> {
        match self.format {
            InputFormat::Json => serde_json::from_str(&self.content).map_err(JqrError::from),
            InputFormat::Yaml => {
                serde_yaml::from_str::<Value>(&self.content).map_err(JqrError::from)
            }
            InputFormat::Toml => {
                let tv = toml::from_str::<toml::Value>(&self.content)?;
                Ok(toml_value_to_json(tv))
            }
            InputFormat::Csv => Self::parse_csv(&self.content).map_err(JqrError::from),
            InputFormat::Auto => {
                if self.content.trim().is_empty() {
                    return Err(JqrError::UnsupportedFormat(
                        "empty input; nothing to parse".into(),
                    ));
                }
                if let Ok(v) = serde_json::from_str(&self.content) {
                    return Ok(v);
                }
                if let Ok(v) = serde_yaml::from_str::<Value>(&self.content) {
                    return Ok(v);
                }
                if let Ok(v) = toml::from_str::<toml::Value>(&self.content) {
                    return Ok(toml_value_to_json(v));
                }
                if let Ok(v) = Self::parse_csv(&self.content) {
                    return Ok(v);
                }
                Err(JqrError::UnsupportedFormat(
                    "could not auto-detect input format".into(),
                ))
            }
        }
    }

    // ------------------------------------------------------------------
    //  Private helpers
    // ------------------------------------------------------------------

    /// True when at least one non-empty, non-comment line contains `=`.
    fn has_toml_pattern(&self) -> bool {
        self.content.lines().any(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty() && !trimmed.starts_with('#') && trimmed.contains('=')
        })
    }

    /// True when at least one non-empty, non-comment line contains `: `
    /// (YAML-style key-value separator).
    fn has_yaml_pattern(&self) -> bool {
        self.content.lines().any(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty() && !trimmed.starts_with('#') && trimmed.contains(": ")
        })
    }

    /// True when every non-empty line has the same positive number of commas.
    fn has_csv_pattern(&self) -> bool {
        let lines: Vec<&str> = self
            .content
            .lines()
            .filter(|line| !line.trim().is_empty())
            .collect();

        if lines.len() < 2 {
            return false;
        }

        let comma_counts: Vec<usize> = lines
            .iter()
            .map(|line| line.chars().filter(|&c| c == ',').count())
            .collect();

        let first = comma_counts[0];
        first > 0 && comma_counts.iter().all(|&c| c == first)
    }

    /// Parse CSV content into `Value::Array` of objects keyed by header names.
    fn parse_csv(content: &str) -> Result<Value, csv::Error> {
        let mut reader = csv::ReaderBuilder::new()
            .has_headers(true)
            .from_reader(content.as_bytes());
        let headers: Vec<String> = match reader.headers() {
            Ok(h) => h.iter().map(String::from).collect(),
            Err(_) => Vec::new(),
        };
        let mut records: Vec<Value> = Vec::new();
        for result in reader.records() {
            let record = result?;
            let obj: serde_json::Map<String, Value> = headers
                .iter()
                .zip(record.iter())
                .map(|(k, v)| (k.clone(), Value::String(v.to_string())))
                .collect();
            records.push(Value::Object(obj));
        }
        Ok(Value::Array(records))
    }
}

// ------------------------------------------------------------------
//  TOML → serde_json::Value conversion
// ------------------------------------------------------------------

/// Recursively convert a [`toml::Value`] into the equivalent
/// [`serde_json::Value`].
pub fn toml_value_to_json(v: toml::Value) -> Value {
    match v {
        toml::Value::String(s) => Value::String(s),
        toml::Value::Integer(i) => Value::Number(i.into()),
        toml::Value::Float(f) => match serde_json::Number::from_f64(f) {
            Some(n) => Value::Number(n),
            None => Value::Number(serde_json::Number::from(0)),
        },
        toml::Value::Boolean(b) => Value::Bool(b),
        toml::Value::Datetime(d) => Value::String(d.to_string()),
        toml::Value::Array(arr) => {
            Value::Array(arr.into_iter().map(toml_value_to_json).collect())
        }
        toml::Value::Table(t) => Value::Object(
            t.into_iter()
                .map(|(k, v)| (k, toml_value_to_json(v)))
                .collect(),
        ),
    }
}

// ------------------------------------------------------------------
//  Tests
// ------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Convenience: build an `InputReader` with explicit format and content.
    fn reader_with(content: &str, format: InputFormat) -> InputReader {
        InputReader {
            format,
            content: content.to_string(),
        }
    }

    /// Convenience: build an `InputReader` in `Auto` mode.
    fn auto_reader(content: &str) -> InputReader {
        reader_with(content, InputFormat::Auto)
    }

    // -- detect -------------------------------------------------------

    #[test]
    fn test_detect_json_object() {
        let mut r = auto_reader(r#"{"key": "value"}"#);
        r.detect();
        assert_eq!(r.format, InputFormat::Json);
    }

    #[test]
    fn test_detect_json_array() {
        let mut r = auto_reader("[1, 2, 3]");
        r.detect();
        assert_eq!(r.format, InputFormat::Json);
    }

    #[test]
    fn test_detect_yaml() {
        let mut r = auto_reader("key: value\nlist:\n  - a");
        r.detect();
        assert_eq!(r.format, InputFormat::Yaml);
    }

    #[test]
    fn test_detect_toml() {
        let mut r = auto_reader("[table]\nkey = \"value\"");
        r.detect();
        assert_eq!(r.format, InputFormat::Toml);
    }

    #[test]
    fn test_detect_csv() {
        let mut r = auto_reader("name,age\nAlice,30");
        r.detect();
        assert_eq!(r.format, InputFormat::Csv);
    }

    #[test]
    fn test_detect_respects_explicit_format() {
        let mut r = reader_with("[1, 2, 3]", InputFormat::Yaml);
        r.detect();
        // detect() is a no-op when format != Auto
        assert_eq!(r.format, InputFormat::Yaml);
    }

    // -- parse --------------------------------------------------------

    #[test]
    fn test_parse_json() {
        let r = reader_with(r#"{"key": "value"}"#, InputFormat::Json);
        let v = r.parse().expect("valid JSON must parse");
        assert_eq!(v["key"], Value::String("value".into()));
    }

    #[test]
    fn test_parse_yaml() {
        let r = reader_with("key: value", InputFormat::Yaml);
        let v = r.parse().expect("valid YAML must parse");
        assert_eq!(v["key"], Value::String("value".into()));
    }

    #[test]
    fn test_parse_toml() {
        let r = reader_with("[table]\nkey = \"value\"", InputFormat::Toml);
        let v = r.parse().expect("valid TOML must parse");
        assert_eq!(v["table"]["key"], Value::String("value".into()));
    }

    #[test]
    fn test_parse_csv() {
        let r = reader_with("name,age\nAlice,30", InputFormat::Csv);
        let v = r.parse().expect("valid CSV must parse");

        let mut obj = serde_json::Map::new();
        obj.insert("name".into(), Value::String("Alice".into()));
        obj.insert("age".into(), Value::String("30".into()));
        let expected = Value::Array(vec![Value::Object(obj)]);
        assert_eq!(v, expected);
    }

    #[test]
    fn test_parse_invalid_json() {
        let r = reader_with("{invalid", InputFormat::Json);
        let result = r.parse();
        assert!(result.is_err(), "invalid JSON must produce an error");
    }

    #[test]
    fn test_auto_detect_json() {
        let mut r = auto_reader(r#"{"key": "value"}"#);
        r.detect();
        let v = r.parse().expect("auto-detected JSON must parse");
        assert_eq!(v["key"], Value::String("value".into()));
    }

    #[test]
    fn test_empty_input() {
        let r = auto_reader("");
        let result = r.parse();
        assert!(result.is_err(), "empty input must produce an error");
    }

    #[test]
    fn test_unicode_content() {
        let r = reader_with(r#"{"名字": "小明"}"#, InputFormat::Json);
        let v = r.parse().expect("unicode JSON must parse");
        assert_eq!(v["名字"], Value::String("小明".into()));
    }

    #[test]
    fn test_toml_value_to_json_all_variants() {
        // String
        assert_eq!(
            toml_value_to_json(toml::Value::String("hi".into())),
            Value::String("hi".into())
        );
        // Integer
        assert_eq!(
            toml_value_to_json(toml::Value::Integer(42)),
            Value::Number(serde_json::Number::from(42))
        );
        // Float
        let f = toml_value_to_json(toml::Value::Float(2.71));
        assert!(f.is_number());
        // Boolean
        assert_eq!(
            toml_value_to_json(toml::Value::Boolean(true)),
            Value::Bool(true)
        );
        // Datetime
        let dt = toml_value_to_json(toml::Value::Datetime(
            "2024-01-01T00:00:00Z".parse().unwrap(),
        ));
        assert!(dt.is_string());
        // Array
        let arr = toml_value_to_json(toml::Value::Array(vec![
            toml::Value::Integer(1),
            toml::Value::Integer(2),
        ]));
        assert_eq!(
            arr,
            Value::Array(vec![
                Value::Number(serde_json::Number::from(1)),
                Value::Number(serde_json::Number::from(2)),
            ])
        );
        // Table
        let mut map = toml::map::Map::new();
        map.insert("k".into(), toml::Value::String("v".into()));
        let tbl = toml_value_to_json(toml::Value::Table(map));
        assert_eq!(tbl["k"], Value::String("v".into()));
    }
}
