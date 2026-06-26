//! Agent detection — identifies which AI agent is invoking jqr.
//!
//! Detection is stateless and reads environment variables set by
//! well-known agentic CLIs. Used to pick agent-optimized defaults
//! for token budget and sample size.

use std::env;

/// Known AI agents that may invoke jqr.
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)]
pub enum Agent {
    ClaudeCode,
    OpenCode,
    Cursor,
    CodexCli,
    GeminiCli,
    Unknown,
}

/// Stateless detector — call [`AgentDetector::detect`] to identify the
/// calling agent from its marker environment variable.
#[allow(dead_code)]
pub struct AgentDetector;

impl Default for AgentDetector {
    fn default() -> Self {
        AgentDetector
    }
}

impl AgentDetector {
    /// Create a new detector. The struct is stateless, so this is a
    /// trivial constructor; exists for API symmetry.
    #[allow(dead_code)]
    pub fn new() -> Self {
        AgentDetector
    }

    /// Detect the calling agent by inspecting marker environment
    /// variables. The first match in priority order wins:
    ///
    /// 1. `CLAUDE_CODE`  → [`Agent::ClaudeCode`]
    /// 2. `OPENCODE`     → [`Agent::OpenCode`]
    /// 3. `CURSOR_TRACE` → [`Agent::Cursor`]
    /// 4. `CODEX_CLI`    → [`Agent::CodexCli`]
    /// 5. `GEMINI_CLI`   → [`Agent::GeminiCli`]
    ///
    /// Returns `None` when no marker env var is present.
    #[allow(dead_code)]
    pub fn detect() -> Option<Agent> {
        if env::var("CLAUDE_CODE").is_ok() {
            return Some(Agent::ClaudeCode);
        }
        if env::var("OPENCODE").is_ok() {
            return Some(Agent::OpenCode);
        }
        if env::var("CURSOR_TRACE").is_ok() {
            return Some(Agent::Cursor);
        }
        if env::var("CODEX_CLI").is_ok() {
            return Some(Agent::CodexCli);
        }
        if env::var("GEMINI_CLI").is_ok() {
            return Some(Agent::GeminiCli);
        }
        None
    }

    /// Recommended default token budget for the given agent.
    #[allow(dead_code)]
    pub fn default_token_budget_for(agent: Agent) -> usize {
        match agent {
            Agent::ClaudeCode => 4096,
            Agent::OpenCode => 8192,
            Agent::Cursor => 4096,
            Agent::CodexCli => 8192,
            Agent::GeminiCli => 8192,
            Agent::Unknown => 4096,
        }
    }

    /// Recommended default sample size for the given agent.
    #[allow(dead_code)]
    pub fn default_sample_size_for(agent: Agent) -> usize {
        match agent {
            Agent::ClaudeCode => 3,
            Agent::OpenCode => 5,
            Agent::Cursor => 3,
            Agent::CodexCli => 5,
            Agent::GeminiCli => 5,
            Agent::Unknown => 5,
        }
    }
}

// ─── Env-var test lock (shared by unit + integration tests) ───

use std::sync::Mutex;

/// Global lock to serialise env-var tests.  Rust runs tests in
/// parallel by default, and `env::set_var` / `env::remove_var`
/// are process-global — concurrent env-var tests will race.
#[allow(dead_code)]
pub static ENV_LOCK: Mutex<()> = Mutex::new(());

// ─── Unit tests ───

#[cfg(test)]
mod tests {
    use super::*;

    /// Run `f` with env var `name` set to `value`, restoring the
    /// previous state (whatever it was) afterward.  Acquires
    /// [`ENV_LOCK`] so only one env-var test runs at a time.
    fn with_env_var<F: FnOnce()>(name: &str, value: &str, f: F) {
        let _guard = ENV_LOCK.lock().unwrap();
        let old = env::var(name).ok();
        env::set_var(name, value);
        f();
        if let Some(v) = old {
            env::set_var(name, v);
        } else {
            env::remove_var(name);
        }
    }

    #[test]
    fn test_detect_claude_code() {
        with_env_var("CLAUDE_CODE", "1", || {
            assert_eq!(AgentDetector::detect(), Some(Agent::ClaudeCode));
        });
    }

    #[test]
    fn test_detect_opencode() {
        with_env_var("OPENCODE", "1", || {
            assert_eq!(AgentDetector::detect(), Some(Agent::OpenCode));
        });
    }

    #[test]
    fn test_detect_cursor() {
        with_env_var("CURSOR_TRACE", "1", || {
            assert_eq!(AgentDetector::detect(), Some(Agent::Cursor));
        });
    }

    #[test]
    fn test_detect_codex_cli() {
        with_env_var("CODEX_CLI", "1", || {
            assert_eq!(AgentDetector::detect(), Some(Agent::CodexCli));
        });
    }

    #[test]
    fn test_detect_gemini_cli() {
        with_env_var("GEMINI_CLI", "1", || {
            assert_eq!(AgentDetector::detect(), Some(Agent::GeminiCli));
        });
    }

    #[test]
    fn test_detect_first_match() {
        let _guard = ENV_LOCK.lock().unwrap();
        let old_cc = env::var("CLAUDE_CODE").ok();
        let old_oc = env::var("OPENCODE").ok();
        env::set_var("CLAUDE_CODE", "1");
        env::set_var("OPENCODE", "1");

        let result = AgentDetector::detect();

        if let Some(v) = old_cc {
            env::set_var("CLAUDE_CODE", v);
        } else {
            env::remove_var("CLAUDE_CODE");
        }
        if let Some(v) = old_oc {
            env::set_var("OPENCODE", v);
        } else {
            env::remove_var("OPENCODE");
        }

        assert_eq!(result, Some(Agent::ClaudeCode));
    }

    #[test]
    fn test_detect_none() {
        let _guard = ENV_LOCK.lock().unwrap();
        let vars = [
            "CLAUDE_CODE",
            "OPENCODE",
            "CURSOR_TRACE",
            "CODEX_CLI",
            "GEMINI_CLI",
        ];
        let saved: Vec<Option<String>> = vars.iter().map(|v| env::var(v).ok()).collect();
        for v in vars {
            env::remove_var(v);
        }

        let result = AgentDetector::detect();

        for (name, original) in vars.iter().zip(saved) {
            if let Some(v) = original {
                env::set_var(name, v);
            } else {
                env::remove_var(name);
            }
        }

        assert_eq!(result, None);
    }

    #[test]
    fn test_token_budget_per_agent() {
        assert_eq!(AgentDetector::default_token_budget_for(Agent::ClaudeCode), 4096);
        assert_eq!(AgentDetector::default_token_budget_for(Agent::OpenCode), 8192);
        assert_eq!(AgentDetector::default_token_budget_for(Agent::Cursor), 4096);
        assert_eq!(AgentDetector::default_token_budget_for(Agent::CodexCli), 8192);
        assert_eq!(AgentDetector::default_token_budget_for(Agent::GeminiCli), 8192);
        assert_eq!(AgentDetector::default_token_budget_for(Agent::Unknown), 4096);
    }

    #[test]
    fn test_sample_size_per_agent() {
        assert_eq!(AgentDetector::default_sample_size_for(Agent::ClaudeCode), 3);
        assert_eq!(AgentDetector::default_sample_size_for(Agent::OpenCode), 5);
        assert_eq!(AgentDetector::default_sample_size_for(Agent::Cursor), 3);
        assert_eq!(AgentDetector::default_sample_size_for(Agent::CodexCli), 5);
        assert_eq!(AgentDetector::default_sample_size_for(Agent::GeminiCli), 5);
        assert_eq!(AgentDetector::default_sample_size_for(Agent::Unknown), 5);
    }
}
