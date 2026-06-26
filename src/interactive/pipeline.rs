use serde_json::Value;

use crate::cli::Cli;
use crate::config::JqrConfig;
use crate::error::JqrError;
use crate::filter::FilterEngine;
use crate::output::format::format_raw;

use crate::interactive::OutputMode;

/// The result of running a pipeline in a given mode.
pub enum PipelineOutput {
    Raw(String),
    Compact(String),
    Envelope(String),
    SchemaOnly(String),
}

impl PipelineOutput {
    /// Extract the string content regardless of variant.
    pub fn into_string(self) -> String {
        match self {
            PipelineOutput::Raw(s)
            | PipelineOutput::Compact(s)
            | PipelineOutput::Envelope(s)
            | PipelineOutput::SchemaOnly(s) => s,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipeline_output_into_string_raw() {
        let out = PipelineOutput::Raw("hello".to_string());
        assert_eq!(out.into_string(), "hello");
    }

    #[test]
    fn test_pipeline_output_into_string_compact() {
        let out = PipelineOutput::Compact("compact".to_string());
        assert_eq!(out.into_string(), "compact");
    }

    #[test]
    fn test_pipeline_output_into_string_envelope() {
        let out = PipelineOutput::Envelope("env".to_string());
        assert_eq!(out.into_string(), "env");
    }

    #[test]
    fn test_pipeline_output_into_string_schema_only() {
        let out = PipelineOutput::SchemaOnly("schema".to_string());
        assert_eq!(out.into_string(), "schema");
    }
}

/// Convert filter results to a single `Value` for schema inference.
///
/// Mirrors the unwrapping logic in [`format_raw`]: single-element arrays are
/// unwrapped to their sole value so the schema matches the user's mental model
/// of the output shape.
fn results_to_value(results: &[Value]) -> Value {
    match results.len() {
        0 => Value::Null,
        1 => results[0].clone(),
        _ => Value::Array(results.to_vec()),
    }
}

/// Run the jaq filter pipeline and format output according to `mode`.
///
/// Returns a [`PipelineOutput`] variant matching the requested mode.
pub fn run_pipeline(
    value: &Value,
    filter_str: &str,
    config: &JqrConfig,
    cli: &Cli,
    mode: OutputMode,
) -> Result<PipelineOutput, JqrError> {
    let engine = FilterEngine::compile(filter_str)?;
    let results = engine.run(value)?;

    match mode {
        OutputMode::Raw => {
            let out = format_raw(&results, false);
            Ok(PipelineOutput::Raw(out))
        }
        OutputMode::Compact => {
            let out = format_raw(&results, true);
            Ok(PipelineOutput::Compact(out))
        }
        OutputMode::Pretty => {
            use crate::output::envelope::Envelope;
            use crate::output::truncate::Truncator;
            use crate::schema::SchemaInferrer;

            let token_budget = cli
                .tokens
                .unwrap_or(config.output.default_token_budget);
            let sample_size = cli.sample_size.max(1);

            let truncator = Truncator::new(token_budget, sample_size);
            let truncation = truncator.truncate(&results);
            let inferrer = SchemaInferrer::new(config.schema.max_depth);
            let schema = inferrer.infer(&results_to_value(&results));

            let envelope = Envelope::from_results(
                &results,
                schema,
                truncation,
                config.schema.format.clone(),
            );

            let out = envelope.to_json(true, false);
            Ok(PipelineOutput::Envelope(out))
        }
        OutputMode::Envelope | OutputMode::SchemaOnly => {
            use crate::output::envelope::Envelope;
            use crate::output::truncate::Truncator;
            use crate::schema::SchemaInferrer;

            let token_budget = cli
                .tokens
                .unwrap_or(config.output.default_token_budget);
            let sample_size = cli.sample_size.max(1);

            let truncator = Truncator::new(token_budget, sample_size);
            let truncation = truncator.truncate(&results);
            let inferrer = SchemaInferrer::new(config.schema.max_depth);
            let schema = inferrer.infer(&results_to_value(&results));

            let envelope = Envelope::from_results(
                &results,
                schema,
                truncation,
                config.schema.format.clone(),
            );

            let schema_only = matches!(mode, OutputMode::SchemaOnly);
            let pretty = cli.pretty;
            let out = envelope.to_json(pretty, schema_only);

            if schema_only {
                Ok(PipelineOutput::SchemaOnly(out))
            } else {
                Ok(PipelineOutput::Envelope(out))
            }
        }
    }
}
