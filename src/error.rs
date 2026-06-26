use std::io;

use thiserror::Error;

/// All errors that can occur in the jqr CLI tool.
///
/// Every other module converts its specific failure into a [`JqrError`] at the
/// boundary so the rest of the program can propagate with a single type and
/// present one consistent error story to the user.
#[derive(Error, Debug)]
#[allow(dead_code)]
pub enum JqrError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("YAML error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("TOML deserialization error: {0}")]
    TomlDe(#[from] toml::de::Error),

    #[error("TOML serialization error: {0}")]
    TomlSer(#[from] toml::ser::Error),

    #[error("CSV error: {0}")]
    Csv(#[from] csv::Error),

    #[error("jaq parse error: {0}")]
    JaqParse(String),

    #[error("jaq eval error: {0}")]
    JaqEval(String),

    #[error("unsupported format: {0}")]
    UnsupportedFormat(String),

    #[error("config error: {0}")]
    Config(String),

    #[error("JSON repair error: {0}")]
    Repair(String),

    #[error("schema inference error: {0}")]
    Schema(String),

    #[error("MCP server error: {0}")]
    Mcp(String),
}

/// Convert a `jaq_parse::Error` (a `chumsky::error::Simple<String>`) into
/// a [`JqrError::JaqParse`] by formatting the chumsky diagnostic via `Debug`.
///
/// In `jaq-parse 1.0.x` the type is re-exported as `jaq_parse::Error`, not
/// `jaq_parse::ParseError`.
impl From<jaq_parse::Error> for JqrError {
    fn from(err: jaq_parse::Error) -> Self {
        JqrError::JaqParse(format!("{err:?}"))
    }
}

/// Convert a `jaq_interpret::Error` into a [`JqrError::JaqEval`] using its
/// own `Display` impl.
impl From<jaq_interpret::Error> for JqrError {
    fn from(err: jaq_interpret::Error) -> Self {
        JqrError::JaqEval(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_io_error_from() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let err: JqrError = io_err.into();
        assert!(
            matches!(err, JqrError::Io(_)),
            "expected JqrError::Io, got {err:?}"
        );
    }

    #[test]
    fn test_json_error_from() {
        let result: Result<serde_json::Value, serde_json::Error> =
            serde_json::from_str("not valid json");
        if let Err(e) = result {
            let err: JqrError = e.into();
            assert!(
                matches!(err, JqrError::Json(_)),
                "expected JqrError::Json, got {err:?}"
            );
            return;
        }
        // The conversion test is only meaningful when the parser produced an
        // error. If this branch is ever reached, the test environment has
        // changed in a way that invalidates the assertion's premise.
        unreachable!("test setup: expected JSON parse to fail for invalid input")
    }

    #[test]
    fn test_display_format() {
        let err = JqrError::Config("test".into());
        let msg = err.to_string();
        assert!(
            msg.contains("config error: test"),
            "expected display to contain 'config error: test', got: {msg}"
        );
    }

    #[test]
    fn test_debug_format() {
        let err = JqrError::Repair("bad".into());
        let dbg = format!("{err:?}");
        assert!(
            dbg.contains("Repair"),
            "expected debug to contain 'Repair', got: {dbg}"
        );
    }

    #[test]
    fn test_jaq_parse_error_from() {
        let (_parsed, errs) = jaq_parse::parse("@", jaq_parse::main());
        assert!(!errs.is_empty(), "test setup: expected jaq parse errors");
        let jerr: JqrError = errs[0].clone().into();
        assert!(
            matches!(jerr, JqrError::JaqParse(_)),
            "expected JqrError::JaqParse, got {jerr:?}"
        );
    }
}
