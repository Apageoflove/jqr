// allow: SIZE_OK — config module with 5 structs, 3 enums, Default impls,
// serde attrs, merge logic, and tests; the definitions are pure data tables
// and the merge logic is inherently coupled to the field declarations.

pub mod agent;

use std::fmt;
use std::path::PathBuf;

use anyhow::Context;
use serde::Deserialize;

// ─── Default value factories ───

const fn default_output_mode() -> OutputMode {
    OutputMode::Schema
}

const fn default_token_budget() -> usize {
    4096
}

const fn default_sample_size() -> usize {
    100
}

const fn default_true() -> bool {
    true
}

const fn default_csv_delimiter() -> char {
    ','
}

const fn default_max_depth() -> usize {
    10
}

// ─── OutputMode ───

#[derive(Debug, Clone, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum OutputMode {
    #[default]
    Schema,
    Raw,
    Compact,
}


impl fmt::Display for OutputMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OutputMode::Schema => write!(f, "schema"),
            OutputMode::Raw => write!(f, "raw"),
            OutputMode::Compact => write!(f, "compact"),
        }
    }
}

// ─── InputFormat ───

#[derive(Debug, Clone, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum InputFormat {
    #[default]
    Auto,
    Json,
    Yaml,
    Toml,
    Csv,
}


impl fmt::Display for InputFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InputFormat::Auto => write!(f, "auto"),
            InputFormat::Json => write!(f, "json"),
            InputFormat::Yaml => write!(f, "yaml"),
            InputFormat::Toml => write!(f, "toml"),
            InputFormat::Csv => write!(f, "csv"),
        }
    }
}

// ─── SchemaFormat ───

#[derive(Debug, Clone, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SchemaFormat {
    #[default]
    JsonSchema,
    Typescript,
    Zod,
    Pydantic,
}


// ─── Config structs ───

#[derive(Debug, Clone, Deserialize)]
pub struct OutputConfig {
    #[serde(default = "default_output_mode")]
    pub mode: OutputMode,
    #[serde(default = "default_token_budget")]
    pub default_token_budget: usize,
    #[serde(default = "default_sample_size")]
    pub sample_size: usize,
    #[serde(default = "default_true")]
    pub pretty: bool,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            mode: default_output_mode(),
            default_token_budget: default_token_budget(),
            sample_size: default_sample_size(),
            pretty: default_true(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct InputConfig {
    #[serde(default)]
    pub format: InputFormat,
    #[serde(default = "default_csv_delimiter")]
    pub csv_delimiter: char,
}

impl Default for InputConfig {
    fn default() -> Self {
        Self {
            format: InputFormat::default(),
            csv_delimiter: default_csv_delimiter(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct SchemaConfig {
    #[serde(default = "default_max_depth")]
    pub max_depth: usize,
    #[serde(default)]
    pub format: SchemaFormat,
}

impl Default for SchemaConfig {
    fn default() -> Self {
        Self {
            max_depth: default_max_depth(),
            format: SchemaFormat::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct AgentsConfig {
    #[serde(default = "default_true")]
    pub auto_detect: bool,
    #[serde(default)]
    pub env_triggers: Vec<String>,
}

impl Default for AgentsConfig {
    fn default() -> Self {
        Self {
            auto_detect: default_true(),
            env_triggers: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct JqrConfig {
    #[serde(default)]
    pub output: OutputConfig,
    #[serde(default)]
    pub input: InputConfig,
    #[serde(default)]
    pub schema: SchemaConfig,
    #[serde(default)]
    pub agents: AgentsConfig,
}


// ─── CliOverrides ───

pub struct CliOverrides {
    pub output_mode: Option<OutputMode>,
    pub token_budget: Option<usize>,
    pub sample_size: Option<usize>,
    pub input_format: Option<InputFormat>,
    pub schema_format: Option<SchemaFormat>,
    pub pretty: Option<bool>,
}

// ─── Merge logic ───

impl JqrConfig {
    /// Merge non-default fields from `other` into `self`.
    ///
    /// A field is considered "set" if it differs from the built-in default.
    /// Fields that match the default are left unchanged in `self`, preserving
    /// values merged from higher-priority sources.
    fn merge_from(&mut self, other: &Self) {
        let default = Self::default();

        if other.output.mode != default.output.mode {
            self.output.mode = other.output.mode.clone();
        }
        if other.output.default_token_budget != default.output.default_token_budget {
            self.output.default_token_budget = other.output.default_token_budget;
        }
        if other.output.sample_size != default.output.sample_size {
            self.output.sample_size = other.output.sample_size;
        }
        if other.output.pretty != default.output.pretty {
            self.output.pretty = other.output.pretty;
        }

        if other.input.format != default.input.format {
            self.input.format = other.input.format.clone();
        }
        if other.input.csv_delimiter != default.input.csv_delimiter {
            self.input.csv_delimiter = other.input.csv_delimiter;
        }

        if other.schema.max_depth != default.schema.max_depth {
            self.schema.max_depth = other.schema.max_depth;
        }
        if other.schema.format != default.schema.format {
            self.schema.format = other.schema.format.clone();
        }

        if other.agents.auto_detect != default.agents.auto_detect {
            self.agents.auto_detect = other.agents.auto_detect;
        }
        if other.agents.env_triggers != default.agents.env_triggers {
            self.agents.env_triggers = other.agents.env_triggers.clone();
        }
    }

    /// Apply CLI overrides, which always take precedence over file-based config.
    fn apply_overrides(&mut self, overrides: &CliOverrides) {
        if let Some(ref mode) = overrides.output_mode {
            self.output.mode = mode.clone();
        }
        if let Some(budget) = overrides.token_budget {
            self.output.default_token_budget = budget;
        }
        if let Some(size) = overrides.sample_size {
            self.output.sample_size = size;
        }
        if let Some(pretty) = overrides.pretty {
            self.output.pretty = pretty;
        }
        if let Some(ref format) = overrides.input_format {
            self.input.format = format.clone();
        }
        if let Some(ref format) = overrides.schema_format {
            self.schema.format = format.clone();
        }
    }

    /// Resolve the home directory from `$HOME`.
    fn home_dir() -> Option<PathBuf> {
        std::env::var("HOME").ok().map(PathBuf::from)
    }

    /// Load configuration from standard locations, merging in priority order.
    ///
    /// Priority (lowest to highest):
    /// 1. Built-in defaults
    /// 2. `~/.jqrrc` (JSON)
    /// 3. `~/.config/jqr/config.toml` (TOML)
    /// 4. `.jqrrc` (project-local JSON)
    /// 5. CLI overrides
    ///
    /// Missing config files are silently ignored — the function returns
    /// defaults when no config files exist.
    ///
    /// # Errors
    /// Returns an error if a config file exists but cannot be read or parsed.
    pub fn load(cli_overrides: Option<&CliOverrides>) -> anyhow::Result<Self> {
        let mut config = Self::default();

        if let Some(home) = Self::home_dir() {
            // ~/.jqrrc (JSON)
            let global_json = home.join(".jqrrc");
            if global_json.exists() {
                let content = std::fs::read_to_string(&global_json)
                    .with_context(|| format!("failed to read {}", global_json.display()))?;
                let parsed: JqrConfig = serde_json::from_str(&content)
                    .with_context(|| format!("failed to parse {}", global_json.display()))?;
                config.merge_from(&parsed);
            }

            // ~/.config/jqr/config.toml (TOML)
            let global_toml = home.join(".config/jqr/config.toml");
            if global_toml.exists() {
                let content = std::fs::read_to_string(&global_toml)
                    .with_context(|| format!("failed to read {}", global_toml.display()))?;
                let parsed: JqrConfig = toml::from_str(&content)
                    .with_context(|| format!("failed to parse {}", global_toml.display()))?;
                config.merge_from(&parsed);
            }
        }

        // .jqrrc (project-local JSON)
        let local_json = PathBuf::from(".jqrrc");
        if local_json.exists() {
            let content = std::fs::read_to_string(&local_json)
                .with_context(|| format!("failed to read {}", local_json.display()))?;
            let parsed: JqrConfig = serde_json::from_str(&content)
                .with_context(|| format!("failed to parse {}", local_json.display()))?;
            config.merge_from(&parsed);
        }

        if let Some(overrides) = cli_overrides {
            config.apply_overrides(overrides);
        }

        Ok(config)
    }
}

// ─── Tests ───

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_mode_default() {
        assert_eq!(OutputMode::default(), OutputMode::Schema);
    }

    #[test]
    fn test_input_format_default() {
        assert_eq!(InputFormat::default(), InputFormat::Auto);
    }

    #[test]
    fn test_default_config() {
        let config = JqrConfig::default();
        assert_eq!(config.output.mode, OutputMode::Schema);
        assert_eq!(config.output.default_token_budget, 4096);
        assert_eq!(config.output.sample_size, 100);
        assert!(config.output.pretty);
        assert_eq!(config.input.format, InputFormat::Auto);
        assert_eq!(config.input.csv_delimiter, ',');
        assert_eq!(config.schema.max_depth, 10);
        assert_eq!(config.schema.format, SchemaFormat::JsonSchema);
        assert!(config.agents.auto_detect);
        assert!(config.agents.env_triggers.is_empty());
    }

    /// Verify `load()` returns defaults when no config files exist anywhere.
    #[test]
    fn test_load_no_files() {
        let temp = tempfile::tempdir().expect("failed to create temp dir");
        let saved_home = std::env::var("HOME").ok();
        let saved_dir = std::env::current_dir().ok();

        // Point HOME at an empty temp directory (no ~/.jqrrc, no ~/.config/jqr/)
        std::env::set_var("HOME", temp.path());
        // Change CWD to the temp dir so .jqrrc doesn't exist either
        std::env::set_current_dir(temp.path()).expect("failed to change dir");

        let result = JqrConfig::load(None);

        // Restore environment
        if let Some(home) = saved_home {
            std::env::set_var("HOME", home);
        } else {
            std::env::remove_var("HOME");
        }
        if let Some(dir) = saved_dir {
            let _ = std::env::set_current_dir(dir);
        }

        let config = result.expect("load should succeed with no config files");
        assert_eq!(config.output.mode, OutputMode::Schema);
        assert_eq!(config.output.default_token_budget, 4096);
    }

    /// Verify CLI overrides take highest priority.
    #[test]
    fn test_cli_overrides() {
        let overrides = CliOverrides {
            output_mode: Some(OutputMode::Raw),
            token_budget: Some(1000),
            sample_size: Some(50),
            input_format: Some(InputFormat::Yaml),
            schema_format: Some(SchemaFormat::Typescript),
            pretty: Some(false),
        };

        let temp = tempfile::tempdir().expect("failed to create temp dir");
        let saved_home = std::env::var("HOME").ok();
        let saved_dir = std::env::current_dir().ok();

        std::env::set_var("HOME", temp.path());
        std::env::set_current_dir(temp.path()).expect("failed to change dir");

        let result = JqrConfig::load(Some(&overrides));

        // Restore environment
        if let Some(home) = saved_home {
            std::env::set_var("HOME", home);
        } else {
            std::env::remove_var("HOME");
        }
        if let Some(dir) = saved_dir {
            let _ = std::env::set_current_dir(dir);
        }

        let config = result.expect("load should succeed");
        assert_eq!(config.output.mode, OutputMode::Raw);
        assert_eq!(config.output.default_token_budget, 1000);
        assert_eq!(config.output.sample_size, 50);
        assert!(!config.output.pretty);
        assert_eq!(config.input.format, InputFormat::Yaml);
        assert_eq!(config.schema.format, SchemaFormat::Typescript);
    }
}
