mod cli;
mod config;
mod error;
mod explain;
mod filter;
mod input;
mod interactive;
mod output;
mod repair;
mod schema;

#[cfg(feature = "mcp")]
mod mcp;

use std::io::{self, IsTerminal};
use std::os::fd::AsRawFd;
use std::process;

extern "C" {
    fn dup2(oldfd: i32, newfd: i32) -> i32;
}

use clap::Parser;
use serde_json::Value;

use crate::cli::{Cli, Command, InputFormat as CliInputFormat, OutputFormat as CliOutputFormat};
use crate::config::{CliOverrides, InputFormat as CfgInputFormat, JqrConfig, OutputMode};
use crate::filter::FilterEngine;
use crate::input::{InputFormat, InputReader};
use crate::interactive::pipeline::{run_pipeline, PipelineOutput};
use crate::interactive::session::InteractiveSession;
use crate::interactive::OutputMode as InteractiveOutputMode;
use crate::output::truncate::Truncator;
use crate::repair::JsonRepairer;
use crate::schema::SchemaInferrer;

fn main() {
    let cli = Cli::parse();

    if let Some(Command::Mcp { .. }) = &cli.command {
        #[cfg(feature = "mcp")]
        {
            let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
            if let Err(e) = rt.block_on(crate::mcp::run_stdio_server()) {
                eprintln!("MCP server error: {e}");
                process::exit(1);
            }
            return;
        }
        #[cfg(not(feature = "mcp"))]
        {
            eprintln!("MCP server requires the 'mcp' feature (--features mcp)");
            process::exit(1);
        }
    }

    let overrides = build_overrides(&cli);
    let config = match JqrConfig::load(Some(&overrides)) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("config error: {e}");
            process::exit(1);
        }
    };

    let stdin_is_tty = io::stdin().is_terminal();
    let has_explicit_non_interactive = cli.raw
        || cli.compact
        || cli.explain
        || cli.output.is_some()
        || cli.schema_only
        || cli.repair;

    let enter_interactive = cli.interactive || (!has_explicit_non_interactive && cli.file.is_none());

    if enter_interactive {
        let (value, has_data, raw_content) = if let Some(ref file_path) = cli.file {
            match std::fs::read_to_string(file_path) {
                Ok(content) => {
                    let mut reader = InputReader::from_str(&content);
                    let format = cli_input_to_internal(cli.input.as_ref());
                    if let Some(fmt) = format {
                        reader.set_format(fmt);
                    } else {
                        reader.detect();
                    }
                    match reader.parse() {
                        Ok(v) => (v, true, content),
                        Err(e) => {
                            eprintln!("parse error: {e}");
                            process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("cannot read {}: {e}", file_path);
                    process::exit(1);
                }
            }
        } else if stdin_is_tty {
            (Value::Null, false, String::new())
        } else {
            let mut reader = match InputReader::from_stdin() {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("read error: {e}");
                    process::exit(1);
                }
            };
            let raw = reader.content().to_string();
            let format = cli_input_to_internal(cli.input.as_ref());
            if let Some(fmt) = format {
                reader.set_format(fmt);
            } else {
                reader.detect();
            }
            let v = match reader.parse() {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("parse error: {e}");
                    process::exit(1);
                }
            };
            (v, true, raw)
        };

        // After reading stdin data, reopen stdin to /dev/tty so crossterm
        // can read keyboard events (needed when stdin was redirected).
        if !stdin_is_tty {
            let tty = match std::fs::File::open("/dev/tty") {
                Ok(f) => f,
                Err(_) => {
                    // No /dev/tty available (e.g. test environment, CI, daemon).
                    // Fall back to non-interactive pipeline.
                    let filter_str = cli.filter.as_deref().unwrap_or(".").to_string();
                    let mode = if cli.raw || matches!(cli.output, Some(CliOutputFormat::Raw)) {
                        InteractiveOutputMode::Raw
                    } else if cli.compact || matches!(cli.output, Some(CliOutputFormat::Compact)) {
                        InteractiveOutputMode::Compact
                    } else if cli.schema_only {
                        InteractiveOutputMode::SchemaOnly
                    } else {
                        InteractiveOutputMode::Envelope
                    };
                    match run_pipeline(&value, &filter_str, &config, &cli, mode) {
                        Ok(output) => {
                            let s = output.into_string();
                            print!("{s}");
                        }
                        Err(e) => {
                            eprintln!("filter error: {e}");
                            process::exit(1);
                        }
                    }
                    return;
                }
            };
            let tty_fd = tty.as_raw_fd();
            if unsafe { dup2(tty_fd, 0) } == -1 {
                eprintln!("cannot redirect stdin to /dev/tty");
                process::exit(1);
            }
        }

        let filter_str = cli.filter.as_deref().unwrap_or(".").to_string();
        let initial_mode = if cli.raw || matches!(cli.output, Some(CliOutputFormat::Raw)) {
            InteractiveOutputMode::Raw
        } else if cli.compact || matches!(cli.output, Some(CliOutputFormat::Compact)) {
            InteractiveOutputMode::Compact
        } else if cli.schema_only {
            InteractiveOutputMode::SchemaOnly
        } else {
            InteractiveOutputMode::Envelope
        };
        let mut session = InteractiveSession::new(value, config, cli, initial_mode, filter_str, has_data, raw_content);
        session.run();
        return;
    }

    if stdin_is_tty && cli.file.is_none() {
        eprintln!("jqr: expect piped JSON input, e.g.:  echo '{{\"a\":1}}' | jqr .");
        eprintln!("     interactive mode:  jqr");
        process::exit(1);
    }

    let value = if let Some(ref file_path) = cli.file {
        read_file_value(&cli, file_path)
    } else {
        read_stdin_value(&cli)
    };

    let filter_str = cli.filter.as_deref().unwrap_or(".");

    if cli.explain {
        let engine = match FilterEngine::compile(filter_str) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("filter error: {e}");
                process::exit(1);
            }
        };
        let results = match engine.run(&value) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("eval error: {e}");
                process::exit(1);
            }
        };

        let token_budget = cli
            .tokens
            .unwrap_or(config.output.default_token_budget);
        let sample_size = cli.sample_size.max(1);
        let truncator = Truncator::new(token_budget, sample_size);
        let truncation = truncator.truncate(&results);
        let inferrer = SchemaInferrer::new(config.schema.max_depth);
        let schema = inferrer.infer(&value);

        let input_desc = format!(
            "stdin ({}, auto-detect)",
            cli.input
                .as_ref()
                .map(|f| format!("{:?}", f).to_lowercase())
                .unwrap_or_else(|| "json".to_string())
        );
        let output_desc: String = if cli.raw || matches!(cli.output, Some(CliOutputFormat::Raw)) {
            "raw".to_string()
        } else if cli.compact || matches!(cli.output, Some(CliOutputFormat::Compact)) {
            "compact".to_string()
        } else {
            "schema-first (default)".to_string()
        };

        explain::print_plan(
            filter_str,
            &input_desc,
            &output_desc,
            truncation.tokens_used,
            truncation.total,
            truncation.truncated,
            truncation.sample_size,
            &schema,
        );
        return;
    }

    let raw_mode = cli.raw || matches!(cli.output, Some(CliOutputFormat::Raw));
    let compact = cli.compact || matches!(cli.output, Some(CliOutputFormat::Compact));

    if raw_mode {
        match run_pipeline(&value, filter_str, &config, &cli, InteractiveOutputMode::Raw) {
            Ok(PipelineOutput::Raw(output)) => println!("{output}"),
            Ok(_) => unreachable!(),
            Err(e) => {
                eprintln!("filter error: {e}");
                process::exit(1);
            }
        }
        return;
    }

    if compact {
        match run_pipeline(&value, filter_str, &config, &cli, InteractiveOutputMode::Compact) {
            Ok(PipelineOutput::Compact(output)) => print!("{output}"),
            Ok(_) => unreachable!(),
            Err(e) => {
                eprintln!("filter error: {e}");
                process::exit(1);
            }
        }
        return;
    }

    let mode = if cli.schema_only {
        InteractiveOutputMode::SchemaOnly
    } else {
        InteractiveOutputMode::Envelope
    };
    match run_pipeline(&value, filter_str, &config, &cli, mode) {
        Ok(PipelineOutput::Envelope(output)) | Ok(PipelineOutput::SchemaOnly(output)) => {
            println!("{output}")
        }
        Ok(_) => unreachable!(),
        Err(e) => {
            eprintln!("filter error: {e}");
            process::exit(1);
        }
    }
}

fn read_stdin_value(cli: &Cli) -> Value {
    let mut reader = match InputReader::from_stdin() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("read error: {e}");
            process::exit(1);
        }
    };

    let format = cli_input_to_internal(cli.input.as_ref());
    if let Some(fmt) = format {
        reader.set_format(fmt);
    } else {
        reader.detect();
    }

    let parse_reader = if cli.repair {
        let repairer = JsonRepairer::new();
        let fixed = match repairer.repair(reader.content()) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("repair error: {e}");
                process::exit(1);
            }
        };
        let mut r = InputReader::from_str(&fixed);
        if let Some(fmt) = format {
            r.set_format(fmt);
        } else {
            r.detect();
        }
        r
    } else {
        reader
    };

    match parse_reader.parse() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("parse error: {e}");
            process::exit(1);
        }
    }
}

fn read_file_value(cli: &Cli, file_path: &str) -> Value {
    let content = match std::fs::read_to_string(file_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("cannot read {}: {e}", file_path);
            process::exit(1);
        }
    };

    let mut reader = InputReader::from_str(&content);
    let format = cli_input_to_internal(cli.input.as_ref());
    if let Some(fmt) = format {
        reader.set_format(fmt);
    } else {
        reader.detect();
    }

    let parse_reader = if cli.repair {
        let repairer = JsonRepairer::new();
        let fixed = match repairer.repair(reader.content()) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("repair error: {e}");
                process::exit(1);
            }
        };
        let mut r = InputReader::from_str(&fixed);
        if let Some(fmt) = format {
            r.set_format(fmt);
        } else {
            r.detect();
        }
        r
    } else {
        reader
    };

    match parse_reader.parse() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("parse error: {e}");
            process::exit(1);
        }
    }
}

fn build_overrides(cli: &Cli) -> CliOverrides {
    CliOverrides {
        output_mode: cli_output_to_mode(cli.output.as_ref()).or({
            if cli.raw {
                Some(OutputMode::Raw)
            } else if cli.compact {
                Some(OutputMode::Compact)
            } else {
                None
            }
        }),
        token_budget: cli.tokens,
        sample_size: if cli.sample_size != 5 {
            Some(cli.sample_size)
        } else {
            None
        },
        input_format: cli_input_to_cfg(cli.input.as_ref()),
        schema_format: cli.schema_format.as_ref().map(|f| match f {
            cli::SchemaFormat::JsonSchema => config::SchemaFormat::JsonSchema,
            cli::SchemaFormat::Typescript => config::SchemaFormat::Typescript,
            cli::SchemaFormat::Zod => config::SchemaFormat::Zod,
            cli::SchemaFormat::Pydantic => config::SchemaFormat::Pydantic,
        }),
        pretty: if cli.pretty { Some(true) } else { None },
    }
}

fn cli_input_to_internal(fmt: Option<&CliInputFormat>) -> Option<InputFormat> {
    fmt.map(|f| match f {
        CliInputFormat::Auto => InputFormat::Auto,
        CliInputFormat::Json => InputFormat::Json,
        CliInputFormat::Yaml => InputFormat::Yaml,
        CliInputFormat::Toml => InputFormat::Toml,
        CliInputFormat::Csv => InputFormat::Csv,
    })
}

fn cli_input_to_cfg(fmt: Option<&CliInputFormat>) -> Option<CfgInputFormat> {
    fmt.map(|f| match f {
        CliInputFormat::Auto => CfgInputFormat::Auto,
        CliInputFormat::Json => CfgInputFormat::Json,
        CliInputFormat::Yaml => CfgInputFormat::Yaml,
        CliInputFormat::Toml => CfgInputFormat::Toml,
        CliInputFormat::Csv => CfgInputFormat::Csv,
    })
}

fn cli_output_to_mode(fmt: Option<&CliOutputFormat>) -> Option<OutputMode> {
    fmt.map(|f| match f {
        CliOutputFormat::Schema => OutputMode::Schema,
        CliOutputFormat::Raw => OutputMode::Raw,
        CliOutputFormat::Compact => OutputMode::Compact,
    })
}
