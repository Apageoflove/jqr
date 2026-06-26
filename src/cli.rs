use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser, Debug, Clone)]
#[command(
    name = "jqr",
    version,
    about = "jq, but it knows you're an LLM — schema-first, token-budgeted output for AI agents"
)]
pub struct Cli {
    #[arg(help = "jq filter expression (e.g. '.users | map(.name)')")]
    pub filter: Option<String>,

    #[arg(long, short = 't', help = "Token budget for output truncation")]
    pub tokens: Option<usize>,

    #[arg(long, short = 'r', help = "Raw jq-compatible output (no schema envelope)")]
    pub raw: bool,

    #[arg(long, short = 'c', help = "Compact JSON output (no whitespace)")]
    pub compact: bool,

    #[arg(long, short = 'I', help = "Input format (auto-detect by default)")]
    pub input: Option<InputFormat>,

    #[arg(long, short = 'o', help = "Output mode")]
    pub output: Option<OutputFormat>,

    #[arg(long, short = 'R', help = "Repair malformed LLM JSON before processing")]
    pub repair: bool,

    #[arg(long, short = 'x', help = "Explain the filter without executing")]
    pub explain: bool,

    #[arg(long, short = 'f', help = "Schema output format")]
    pub schema_format: Option<SchemaFormat>,

    #[arg(long, short = 'S', help = "Output schema only, no sample data")]
    pub schema_only: bool,

    #[arg(long, short = 'n', default_value = "5", help = "Number of sample records")]
    pub sample_size: usize,

    #[arg(long, short = 'p', help = "Pretty-print output")]
    pub pretty: bool,

    #[arg(long, short = 'i', help = "Interactive mode — Tab to cycle output, / to filter")]
    pub interactive: bool,

    #[arg(long, short = 'F', help = "Read JSON from file instead of stdin")]
    pub file: Option<String>,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand, Debug, Clone)]
pub enum Command {
    #[command(about = "Start MCP server for AI agent integration")]
    Mcp {
        #[arg(long, default_value = "0.0.0.0")]
        host: String,

        #[arg(long, default_value = "3456")]
        port: u16,
    },
}

#[derive(ValueEnum, Debug, Clone, PartialEq)]
pub enum InputFormat {
    Auto,
    Json,
    Yaml,
    Toml,
    Csv,
}

#[derive(ValueEnum, Debug, Clone, PartialEq)]
pub enum OutputFormat {
    Schema,
    Raw,
    Compact,
}

#[derive(ValueEnum, Debug, Clone, PartialEq)]
pub enum SchemaFormat {
    JsonSchema,
    Typescript,
    Zod,
    Pydantic,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_help() {
        let result = Cli::try_parse_from(["jqr", "--help"]);
        let is_help = matches!(
            &result,
            Err(e) if e.kind() == clap::error::ErrorKind::DisplayHelp
        );
        assert!(is_help, "--help should return DisplayHelp error");
    }

    #[test]
    fn test_cli_version() {
        let result = Cli::try_parse_from(["jqr", "--version"]);
        let is_version = matches!(
            &result,
            Err(e) if e.kind() == clap::error::ErrorKind::DisplayVersion
        );
        assert!(is_version, "--version should return DisplayVersion error");
    }

    #[test]
    fn test_cli_filter_positional() {
        let result = Cli::try_parse_from(["jqr", ".users | map(.name)"]);
        let is_ok = result.is_ok();
        let filter = result.ok().and_then(|c| c.filter);
        assert!(is_ok, "filter positional should parse");
        assert_eq!(filter, Some(".users | map(.name)".to_string()));
    }

    #[test]
    fn test_cli_tokens_flag() {
        let result = Cli::try_parse_from(["jqr", "--tokens", "100"]);
        let is_ok = result.is_ok();
        let tokens = result.ok().and_then(|c| c.tokens);
        assert!(is_ok, "--tokens flag should parse");
        assert_eq!(tokens, Some(100usize));
    }

    #[test]
    fn test_cli_raw_flag() {
        let result = Cli::try_parse_from(["jqr", "--raw"]);
        let is_ok = result.is_ok();
        let raw = result.ok().map(|c| c.raw);
        assert!(is_ok, "--raw flag should parse");
        assert_eq!(raw, Some(true));
    }

    #[test]
    fn test_cli_mcp_subcommand() {
        let result = Cli::try_parse_from(["jqr", "mcp"]);
        let is_ok = result.is_ok();
        let cmd_host = result.as_ref().ok().and_then(|c| {
            c.command.as_ref().map(|Command::Mcp { host, .. }| host.clone())
        });
        let cmd_port = result.as_ref().ok().and_then(|c| {
            c.command.as_ref().map(|Command::Mcp { port, .. }| *port)
        });
        assert!(is_ok, "mcp subcommand should parse");
        assert_eq!(cmd_host, Some("0.0.0.0".to_string()));
        assert_eq!(cmd_port, Some(3456u16));
    }

    #[test]
    fn test_cli_default_sample_size() {
        let result = Cli::try_parse_from(["jqr"]);
        let is_ok = result.is_ok();
        let sample = result.ok().map(|c| c.sample_size);
        assert!(is_ok, "empty args should parse");
        assert_eq!(sample, Some(5usize));
    }
}