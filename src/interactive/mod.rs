pub mod pipeline;
pub mod render;
pub mod session;
pub mod terminal;
pub mod highlight;

pub use self::pipeline::run_pipeline;
pub use self::terminal::TerminalGuard;

use crate::input::InputFormat;

/// Interactive output mode — cycles on Tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    Envelope,
    SchemaOnly,
    Raw,
    Compact,
    Pretty,
}

impl OutputMode {
    /// Cycle to the next mode.
    pub fn next(self) -> Self {
        match self {
            OutputMode::Envelope => OutputMode::SchemaOnly,
            OutputMode::SchemaOnly => OutputMode::Raw,
            OutputMode::Raw => OutputMode::Compact,
            OutputMode::Compact => OutputMode::Pretty,
            OutputMode::Pretty => OutputMode::Envelope,
        }
    }

    /// Human-readable label for the status bar.
    pub fn label(self) -> &'static str {
        match self {
            OutputMode::Envelope => "Envelope",
            OutputMode::SchemaOnly => "Schema",
            OutputMode::Raw => "Raw",
            OutputMode::Compact => "Compact",
            OutputMode::Pretty => "Pretty",
        }
    }
}

/// UI language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    En,
    Zh,
}

/// What the bottom input bar is accepting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    /// Normal: user is typing a jq filter expression.
    Filter,
    /// File-open: user is typing a file path (triggered by Ctrl+O).
    FilePath,
    /// Export: user is typing a save path prefix (triggered by Ctrl+E).
    ExportPath,
}

/// A single query → result entry in the transcript.
#[derive(Debug, Clone)]
pub struct Turn {
    /// The filter expression the user typed.
    pub filter: String,
    /// Rendered output text (JSON, error message, etc.).
    pub output: String,
    /// Output mode used for this turn.
    pub mode: OutputMode,
    /// Estimated token count of the output (0 for Raw/Compact).
    pub token_count: usize,
    /// Number of non-empty result lines (metadata, not currently displayed).
    #[allow(dead_code)]
    pub result_count: usize,
    /// Whether the filter produced an error.
    pub is_error: bool,
}

impl Turn {
    /// Total vertical lines this turn occupies in the transcript
    /// (filter line + output lines + 1 blank separator).
    pub fn line_count(&self) -> usize {
        let out_lines = if self.output.is_empty() { 0 } else { self.output.lines().count() };
        1 + out_lines + 1
    }
}

/// Mutable state for the interactive session.
pub struct InteractiveState {
    /// Chronological list of completed query turns.
    pub turns: Vec<Turn>,
    /// Current input buffer (char vector for cursor editing).
    pub current_input: Vec<char>,
    /// Cursor position within `current_input` (char offset).
    pub cursor: usize,
    /// Active output mode.
    pub mode: OutputMode,
    /// Current input format for parsing/re-parsing.
    pub input_format: InputFormat,
    /// UI language (En / Zh).
    pub language: Language,
    /// What the input bar is currently accepting.
    pub input_mode: InputMode,
    /// Current vertical scroll offset (0 = top of transcript).
    pub scroll_offset: usize,
    /// History of submitted filter strings (most recent last).
    pub filter_history: Vec<String>,
    /// Current position in history navigation (None = not browsing).
    pub history_index: Option<usize>,
    /// Whether we have parsed data to query.
    pub has_data: bool,
    /// Whether the help overlay is visible.
    pub show_help: bool,
    /// Running token count (from the latest turn).
    pub token_count: usize,
    /// Token budget (0 = no limit).
    pub token_budget: usize,
    /// Tracks which input formats have been used during this session.
    pub formats_used: Vec<InputFormat>,
}

impl InteractiveState {
    pub fn new(initial_mode: OutputMode, filter_str: String) -> Self {
        let input: Vec<char> = filter_str.chars().collect();
        let cursor = input.len();
        InteractiveState {
            turns: Vec::new(),
            current_input: input,
            cursor,
            mode: initial_mode,
            input_format: InputFormat::Auto,
            language: Language::En,
            input_mode: InputMode::Filter,
            scroll_offset: 0,
            filter_history: Vec::new(),
            history_index: None,
            has_data: false,
            show_help: false,
            token_count: 0,
            token_budget: 0,
            formats_used: Vec::new(),
        }
    }

    /// Record a format as used (deduped).
    pub fn mark_format_used(&mut self, fmt: InputFormat) {
        if !self.formats_used.contains(&fmt) {
            self.formats_used.push(fmt);
        }
    }

    /// Total number of lines across all turns (for scroll bounds).
    pub fn total_transcript_lines(&self) -> usize {
        self.turns.iter().map(|t| t.line_count()).sum()
    }

    pub fn scroll_up(&mut self, amount: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(amount);
    }

    pub fn scroll_down(&mut self, amount: usize) {
        let max = self.total_transcript_lines();
        self.scroll_offset = (self.scroll_offset + amount).min(max);
    }

    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = self.total_transcript_lines();
    }

    /// Get the current input buffer as a String.
    pub fn input_string(&self) -> String {
        self.current_input.iter().collect()
    }

    /// Insert a character at the cursor position.
    pub fn insert_char(&mut self, c: char) {
        self.current_input.insert(self.cursor, c);
        self.cursor += 1;
    }

    /// Delete the character before the cursor (backspace).
    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            self.current_input.remove(self.cursor - 1);
            self.cursor -= 1;
        }
    }

    /// Move cursor left by one character.
    pub fn cursor_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    /// Move cursor right by one character.
    pub fn cursor_right(&mut self) {
        if self.cursor < self.current_input.len() {
            self.cursor += 1;
        }
    }

    /// Move cursor to the start of the input.
    pub fn cursor_to_start(&mut self) {
        self.cursor = 0;
    }

    /// Move cursor to the end of the input.
    pub fn cursor_to_end(&mut self) {
        self.cursor = self.current_input.len();
    }

    /// Clear the input buffer.
    pub fn clear_input(&mut self) {
        self.current_input.clear();
        self.cursor = 0;
    }

    /// Is the input buffer currently empty?
    pub fn input_is_empty(&self) -> bool {
        self.current_input.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_mode_next_full_cycle() {
        assert_eq!(OutputMode::Envelope.next(), OutputMode::SchemaOnly);
        assert_eq!(OutputMode::SchemaOnly.next(), OutputMode::Raw);
        assert_eq!(OutputMode::Raw.next(), OutputMode::Compact);
        assert_eq!(OutputMode::Compact.next(), OutputMode::Pretty);
        assert_eq!(OutputMode::Pretty.next(), OutputMode::Envelope);
    }

    #[test]
    fn test_output_mode_next_idempotent_cycle() {
        let mut mode = OutputMode::Envelope;
        for _ in 0..5 {
            mode = mode.next();
        }
        assert_eq!(mode, OutputMode::Envelope);
    }

    #[test]
    fn test_output_mode_label_all_variants() {
        assert_eq!(OutputMode::Envelope.label(), "Envelope");
        assert_eq!(OutputMode::SchemaOnly.label(), "Schema");
        assert_eq!(OutputMode::Raw.label(), "Raw");
        assert_eq!(OutputMode::Compact.label(), "Compact");
        assert_eq!(OutputMode::Pretty.label(), "Pretty");
    }

    #[test]
    fn test_interactive_state_new_defaults() {
        let state = InteractiveState::new(OutputMode::Raw, ".".to_string());
        assert_eq!(state.mode, OutputMode::Raw);
        assert_eq!(state.input_string(), ".");
        assert_eq!(state.cursor, 1);
        assert!(state.turns.is_empty());
        assert!(state.filter_history.is_empty());
        assert!(state.history_index.is_none());
        assert_eq!(state.input_format, InputFormat::Auto);
        assert_eq!(state.language, Language::En);
        assert_eq!(state.input_mode, InputMode::Filter);
        assert!(!state.has_data);
        assert!(!state.show_help);
        assert_eq!(state.token_count, 0);
        assert_eq!(state.token_budget, 0);
    }

    #[test]
    fn test_scroll_offset_default_zero() {
        let state = InteractiveState::new(OutputMode::Raw, ".".to_string());
        assert_eq!(state.scroll_offset, 0);
    }

    #[test]
    fn test_scroll_down_within_bounds() {
        let mut state = InteractiveState::new(OutputMode::Raw, ".".to_string());
        state.turns.push(Turn {
            filter: ".".into(),
            output: "line1\nline2\nline3".into(),
            mode: OutputMode::Raw,
            token_count: 0,
            result_count: 3,
            is_error: false,
        });
        // total lines = 1 (filter) + 3 (output) + 1 (blank) = 5
        state.scroll_down(2, );
        assert_eq!(state.scroll_offset, 2);
        state.scroll_down(10);
        assert_eq!(state.scroll_offset, 5);
    }

    #[test]
    fn test_scroll_up_clamped_at_zero() {
        let mut state = InteractiveState::new(OutputMode::Raw, ".".to_string());
        state.scroll_offset = 3;
        state.scroll_up(10);
        assert_eq!(state.scroll_offset, 0);
    }

    #[test]
    fn test_scroll_to_bottom() {
        let mut state = InteractiveState::new(OutputMode::Raw, ".".to_string());
        state.turns.push(Turn {
            filter: ".".into(),
            output: "a\nb".into(),
            mode: OutputMode::Raw,
            token_count: 0,
            result_count: 2,
            is_error: false,
        });
        // total = 1 + 2 + 1 = 4
        state.scroll_to_bottom();
        assert_eq!(state.scroll_offset, 4);
    }

    #[test]
    fn test_total_transcript_lines_empty() {
        let state = InteractiveState::new(OutputMode::Raw, ".".to_string());
        assert_eq!(state.total_transcript_lines(), 0);
    }

    #[test]
    fn test_total_transcript_lines_multiple_turns() {
        let mut state = InteractiveState::new(OutputMode::Raw, ".".to_string());
        state.turns.push(Turn {
            filter: ".a".into(),
            output: "1\n2".into(),
            mode: OutputMode::Raw,
            token_count: 0,
            result_count: 2,
            is_error: false,
        });
        state.turns.push(Turn {
            filter: ".b".into(),
            output: "3".into(),
            mode: OutputMode::Raw,
            token_count: 0,
            result_count: 1,
            is_error: false,
        });
        // turn1: 1 + 2 + 1 = 4, turn2: 1 + 1 + 1 = 3, total = 7
        assert_eq!(state.total_transcript_lines(), 7);
    }

    #[test]
    fn test_insert_char_at_cursor() {
        let mut state = InteractiveState::new(OutputMode::Raw, "ab".to_string());
        state.cursor = 1;
        state.insert_char('X');
        assert_eq!(state.input_string(), "aXb");
        assert_eq!(state.cursor, 2);
    }

    #[test]
    fn test_backspace_before_cursor() {
        let mut state = InteractiveState::new(OutputMode::Raw, "abc".to_string());
        state.cursor = 2;
        state.backspace();
        assert_eq!(state.input_string(), "ac");
        assert_eq!(state.cursor, 1);
    }

    #[test]
    fn test_backspace_at_zero_noop() {
        let mut state = InteractiveState::new(OutputMode::Raw, "abc".to_string());
        state.cursor = 0;
        state.backspace();
        assert_eq!(state.input_string(), "abc");
        assert_eq!(state.cursor, 0);
    }

    #[test]
    fn test_cursor_left_at_zero() {
        let mut state = InteractiveState::new(OutputMode::Raw, "abc".to_string());
        state.cursor = 0;
        state.cursor_left();
        assert_eq!(state.cursor, 0);
    }

    #[test]
    fn test_cursor_right_at_end() {
        let mut state = InteractiveState::new(OutputMode::Raw, "abc".to_string());
        state.cursor = 3;
        state.cursor_right();
        assert_eq!(state.cursor, 3);
    }

    #[test]
    fn test_cursor_to_start_and_end() {
        let mut state = InteractiveState::new(OutputMode::Raw, "abc".to_string());
        state.cursor_to_start();
        assert_eq!(state.cursor, 0);
        state.cursor_to_end();
        assert_eq!(state.cursor, 3);
    }

    #[test]
    fn test_clear_input() {
        let mut state = InteractiveState::new(OutputMode::Raw, "abc".to_string());
        state.clear_input();
        assert!(state.input_is_empty());
        assert_eq!(state.cursor, 0);
    }

    #[test]
    fn test_input_is_empty() {
        let state = InteractiveState::new(OutputMode::Raw, "".to_string());
        assert!(state.input_is_empty());
    }

    #[test]
    fn test_turn_line_count() {
        let turn = Turn {
            filter: ".users".into(),
            output: "line1\nline2\nline3".into(),
            mode: OutputMode::Envelope,
            token_count: 42,
            result_count: 3,
            is_error: false,
        };
        // 1 (filter) + 3 (output) + 1 (blank) = 5
        assert_eq!(turn.line_count(), 5);
    }

    #[test]
    fn test_turn_line_count_empty_output() {
        let turn = Turn {
            filter: ".".into(),
            output: "".into(),
            mode: OutputMode::Raw,
            token_count: 0,
            result_count: 0,
            is_error: false,
        };
        // 1 (filter) + 0 (output) + 1 (blank) = 2
        assert_eq!(turn.line_count(), 2);
    }

    #[test]
    fn test_token_count_default_zero() {
        let state = InteractiveState::new(OutputMode::Raw, ".".to_string());
        assert_eq!(state.token_count, 0);
    }

    #[test]
    fn test_show_help_default_false() {
        let state = InteractiveState::new(OutputMode::Raw, ".".to_string());
        assert!(!state.show_help);
    }

    #[test]
    fn test_input_mode_default_filter() {
        let state = InteractiveState::new(OutputMode::Raw, ".".to_string());
        assert_eq!(state.input_mode, InputMode::Filter);
    }
}
