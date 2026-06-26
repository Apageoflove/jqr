use std::fs;
use std::io::{self, Write};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use serde_json::Value;

use crate::cli::Cli;
use crate::config::JqrConfig;
use crate::input::{InputFormat, InputReader};
use crate::interactive::render::render_frame;
use crate::interactive::{run_pipeline, InteractiveState, OutputMode, TerminalGuard, Language, InputMode, Turn};

/// Extract the `tokens_used` field value from an envelope JSON output string.
///
/// Returns 0 if the field is not found (e.g., Raw / Compact modes).
fn extract_token_count(output: &str) -> usize {
    let marker = "\"tokens_used\":";
    if let Some(pos) = output.find(marker) {
        let rest = &output[pos + marker.len()..];
        let num_str: String = rest
            .trim_start()
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        if let Ok(n) = num_str.parse::<usize>() {
            return n;
        }
    }
    0
}

/// The interactive session drives the terminal event loop.
///
/// Architecture: transcript-style REPL.
/// - A persistent input bar at the bottom accepts filter expressions.
/// - Each Enter pushes a (filter, result) Turn onto the transcript.
/// - The transcript scrolls above the input bar.
/// - Keyboard shortcuts (Ctrl+X) trigger actions without leaving the input bar.
pub struct InteractiveSession {
    value: Value,
    config: JqrConfig,
    cli: Cli,
    state: InteractiveState,
    raw_content: String,
}

impl InteractiveSession {
    /// Create a new interactive session.
    pub fn new(
        value: Value,
        config: JqrConfig,
        cli: Cli,
        initial_mode: OutputMode,
        filter_str: String,
        has_data: bool,
        raw_content: String,
    ) -> Self {
        let mut state = InteractiveState::new(initial_mode, filter_str);
        state.has_data = has_data;
        state.token_budget = cli
            .tokens
            .unwrap_or(config.output.default_token_budget);
        InteractiveSession {
            value,
            config,
            cli,
            state,
            raw_content,
        }
    }

    /// Run the interactive event loop.
    pub fn run(&mut self) {
        let guard = match TerminalGuard::enter() {
            Ok(g) => g,
            Err(e) => {
                eprintln!("failed to enter raw mode: {e}");
                return;
            }
        };

        // Auto-execute the initial filter if we have data.
        if self.state.has_data && !self.state.input_is_empty() {
            self.execute_filter();
        } else {
            self.render();
        }

        let mut stdout = io::stdout();

        loop {
            if !event::poll(Duration::from_millis(50)).unwrap_or(false) {
                continue;
            }

            match event::read() {
                Ok(Event::Key(key)) => {
                    if !self.handle_key(key) {
                        break;
                    }
                }
                Ok(Event::Resize(_, _)) => {
                    self.render();
                }
                Err(_) => {
                    break;
                }
                _ => {}
            }
        }

        drop(guard);

        let _ = writeln!(stdout);
        let _ = stdout.flush();
    }

    /// Execute the current input as a filter, push a Turn, scroll to bottom.
    fn execute_filter(&mut self) {
        let filter = self.state.input_string();

        if filter.is_empty() {
            self.render();
            return;
        }

        // Record in history (dedup consecutive).
        if self.state.filter_history.last() != Some(&filter) {
            self.state.filter_history.push(filter.clone());
        }
        self.state.history_index = None;

        match run_pipeline(
            &self.value,
            &filter,
            &self.config,
            &self.cli,
            self.state.mode,
        ) {
            Ok(output) => {
                let out_str = output.into_string();
                let count = out_str.lines().filter(|l| !l.trim().is_empty()).count();
                let tokens = extract_token_count(&out_str);
                self.state.token_count = tokens;

                let turn = Turn {
                    filter,
                    output: out_str,
                    mode: self.state.mode,
                    token_count: tokens,
                    result_count: count,
                    is_error: false,
                };
                self.state.turns.push(turn);
            }
            Err(e) => {
                let msg = format!("Error: {e}");
                let turn = Turn {
                    filter,
                    output: msg,
                    mode: self.state.mode,
                    token_count: 0,
                    result_count: 0,
                    is_error: true,
                };
                self.state.turns.push(turn);
            }
        }

        self.state.clear_input();
        self.state.scroll_to_bottom();
        self.render();
    }

    /// Cycle output mode. Re-executes ALL turns in the new mode so the
    /// entire transcript reflects the current output format.
    fn cycle_mode(&mut self) {
        self.state.mode = self.state.mode.next();
        let mode = self.state.mode;

        // Collect filter strings; preserve system-turn outputs (:open, :save, etc.)
        let turn_data: Vec<(String, Option<String>)> = self
            .state
            .turns
            .iter()
            .map(|t| {
                if t.filter.starts_with(':') {
                    (t.filter.clone(), Some(t.output.clone()))
                } else {
                    (t.filter.clone(), None)
                }
            })
            .collect();

        self.state.turns.clear();

        for (filter, preserved) in turn_data {
            if let Some(output) = preserved {
                // System notification — keep text, update mode badge
                self.state.turns.push(Turn {
                    filter,
                    output,
                    mode,
                    token_count: 0,
                    result_count: 1,
                    is_error: false,
                });
                continue;
            }

            match run_pipeline(&self.value, &filter, &self.config, &self.cli, mode) {
                Ok(output) => {
                    let out_str = output.into_string();
                    let count = out_str.lines().filter(|l| !l.trim().is_empty()).count();
                    let tokens = extract_token_count(&out_str);
                    self.state.token_count = tokens;
                    self.state.turns.push(Turn {
                        filter,
                        output: out_str,
                        mode,
                        token_count: tokens,
                        result_count: count,
                        is_error: false,
                    });
                }
                Err(e) => {
                    self.state.turns.push(Turn {
                        filter,
                        output: format!("Error: {e}"),
                        mode,
                        token_count: 0,
                        result_count: 0,
                        is_error: true,
                    });
                }
            }
        }

        self.state.scroll_to_bottom();
        self.render();
    }

    /// Cycle input format and re-parse raw_content.
    fn cycle_input_format(&mut self) {
        self.state.input_format = match self.state.input_format {
            InputFormat::Auto => InputFormat::Json,
            InputFormat::Json => InputFormat::Yaml,
            InputFormat::Yaml => InputFormat::Toml,
            InputFormat::Toml => InputFormat::Csv,
            InputFormat::Csv => InputFormat::Auto,
        };
        self.state.mark_format_used(self.state.input_format);

        if self.state.has_data && !self.raw_content.is_empty() {
            let mut reader = InputReader::from_str(&self.raw_content);
            if self.state.input_format == InputFormat::Auto {
                reader.detect();
            } else {
                reader.set_format(self.state.input_format);
            }
            if let Ok(v) = reader.parse() {
                self.value = v;
                // Re-execute all turns with the new data.
                self.reexecute_all_turns();
            }
        }

        self.render();
    }

    /// Re-execute all turns with the current data (used after format change
    /// or file reload).
    fn reexecute_all_turns(&mut self) {
        let filters: Vec<String> = self
            .state
            .turns
            .iter()
            .map(|t| t.filter.clone())
            .collect();
        let modes: Vec<OutputMode> = self
            .state
            .turns
            .iter()
            .map(|t| t.mode)
            .collect();

        self.state.turns.clear();

        for (filter, mode) in filters.into_iter().zip(modes) {
            match run_pipeline(
                &self.value,
                &filter,
                &self.config,
                &self.cli,
                mode,
            ) {
                Ok(output) => {
                    let out_str = output.into_string();
                    let count = out_str.lines().filter(|l| !l.trim().is_empty()).count();
                    let tokens = extract_token_count(&out_str);
                    self.state.token_count = tokens;
                    self.state.turns.push(Turn {
                        filter,
                        output: out_str,
                        mode,
                        token_count: tokens,
                        result_count: count,
                        is_error: false,
                    });
                }
                Err(e) => {
                    self.state.turns.push(Turn {
                        filter,
                        output: format!("Error: {e}"),
                        mode,
                        token_count: 0,
                        result_count: 0,
                        is_error: true,
                    });
                }
            }
        }
        self.state.scroll_to_bottom();
    }

    /// Save the latest turn's output to a file.
    fn save_output(&mut self) {
        let output = match self.state.turns.last() {
            Some(t) if !t.is_error => t.output.clone(),
            _ => return,
        };

        if output.is_empty() {
            return;
        }

        let ext = match self.state.input_format {
            InputFormat::Auto | InputFormat::Json => "json",
            InputFormat::Yaml => "yaml",
            InputFormat::Toml => "toml",
            InputFormat::Csv => "csv",
        };

        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let filename = format!("jqr_output_{ts}.{ext}");

        match fs::write(&filename, &output) {
            Ok(_) => {
                let msg = format!("Saved to {filename}");
                let turn = Turn {
                    filter: ":save".into(),
                    output: msg,
                    mode: self.state.mode,
                    token_count: 0,
                    result_count: 1,
                    is_error: false,
                };
                self.state.turns.push(turn);
                self.state.scroll_to_bottom();
            }
            Err(e) => {
                let turn = Turn {
                    filter: ":save".into(),
                    output: format!("Save error: {e}"),
                    mode: self.state.mode,
                    token_count: 0,
                    result_count: 0,
                    is_error: true,
                };
                self.state.turns.push(turn);
                self.state.scroll_to_bottom();
            }
        }

        self.render();
    }

    /// Enter file-path input mode (triggered by Ctrl+O).
    fn enter_file_open_mode(&mut self) {
        self.state.input_mode = InputMode::FilePath;
        self.state.clear_input();
        self.render();
    }

    /// Load a file from the given path, replacing current data.
    fn load_file(&mut self, path: &str) {
        let path = path.trim();
        if path.is_empty() {
            self.state.input_mode = InputMode::Filter;
            self.state.clear_input();
            self.render();
            return;
        }

        match std::fs::read_to_string(path) {
            Ok(content) => {
                let mut reader = InputReader::from_str(&content);
                if self.state.input_format == InputFormat::Auto {
                    reader.detect();
                } else {
                    reader.set_format(self.state.input_format);
                }
                match reader.parse() {
                    Ok(v) => {
                        self.value = v;
                        self.raw_content = content;
                        self.state.has_data = true;
                        self.state.mark_format_used(reader.format);
                        self.state.turns.clear();
                        self.state.scroll_offset = 0;
                        self.state.input_mode = InputMode::Filter;
                        self.state.clear_input();

                        // Notification: file loaded successfully
                        let turn = Turn {
                            filter: format!(":open {path}"),
                            output: format!("Loaded {}", path),
                            mode: self.state.mode,
                            token_count: 0,
                            result_count: 1,
                            is_error: false,
                        };
                        self.state.turns.push(turn);

                        // Auto-execute "." to show the loaded data.
                        self.state.current_input = vec!['.'];
                        self.state.cursor = 1;
                        self.execute_filter();
                    }
                    Err(e) => {
                        let turn = Turn {
                            filter: format!(":open {path}"),
                            output: format!("Parse error: {e}"),
                            mode: self.state.mode,
                            token_count: 0,
                            result_count: 0,
                            is_error: true,
                        };
                        self.state.turns.push(turn);
                        self.state.scroll_to_bottom();
                        self.state.input_mode = InputMode::Filter;
                        self.state.clear_input();
                        self.render();
                    }
                }
            }
            Err(e) => {
                let turn = Turn {
                    filter: format!(":open {path}"),
                    output: format!("Cannot open {path}: {e}"),
                    mode: self.state.mode,
                    token_count: 0,
                    result_count: 0,
                    is_error: true,
                };
                self.state.turns.push(turn);
                self.state.scroll_to_bottom();
                self.state.input_mode = InputMode::Filter;
                self.state.clear_input();
                self.render();
            }
        }
    }

    /// Reload the input data (re-parse raw_content).
    fn reload_data(&mut self) {
        if !self.state.has_data || self.raw_content.is_empty() {
            self.render();
            return;
        }

        let mut reader = InputReader::from_str(&self.raw_content);
        if self.state.input_format == InputFormat::Auto {
            reader.detect();
        } else {
            reader.set_format(self.state.input_format);
        }

        match reader.parse() {
            Ok(v) => {
                self.value = v;
                self.reexecute_all_turns();
            }
            Err(e) => {
                let turn = Turn {
                    filter: ":reload".into(),
                    output: format!("Reload error: {e}"),
                    mode: self.state.mode,
                    token_count: 0,
                    result_count: 0,
                    is_error: true,
                };
                self.state.turns.push(turn);
                self.state.scroll_to_bottom();
            }
        }

        self.render();
    }

    /// Clear the transcript history.
    fn clear_transcript(&mut self) {
        self.state.turns.clear();
        self.state.scroll_offset = 0;
        self.render();
    }

    /// Toggle UI language.
    fn toggle_language(&mut self) {
        self.state.language = match self.state.language {
            Language::En => Language::Zh,
            Language::Zh => Language::En,
        };
        self.render();
    }

    /// Enter export-path input mode (triggered by Ctrl+E).
    fn enter_export_mode(&mut self) {
        self.state.input_mode = InputMode::ExportPath;
        self.state.clear_input();
        // Pre-fill with a default path prefix.
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let default = format!("jqr_export_{ts}");
        self.state.current_input = default.chars().collect();
        self.state.cursor = self.state.current_input.len();
        self.render();
    }

    /// Export the current data in all formats that have been used.
    fn export_data(&mut self, path_prefix: &str) {
        let path_prefix = path_prefix.trim();
        if path_prefix.is_empty() {
            self.state.input_mode = InputMode::Filter;
            self.state.clear_input();
            self.render();
            return;
        }

        // Determine which formats to export.
        let formats: Vec<InputFormat> = if self.state.formats_used.is_empty() {
            vec![InputFormat::Json]
        } else {
            self.state.formats_used.clone()
        };

        let mut saved: Vec<String> = Vec::new();
        let mut errors: Vec<String> = Vec::new();

        for fmt in &formats {
            let ext = match fmt {
                InputFormat::Json | InputFormat::Auto => "json",
                InputFormat::Yaml => "yaml",
                InputFormat::Toml => "toml",
                InputFormat::Csv => "csv",
            };
            let filename = format!("{path_prefix}.{ext}");

            let result = match fmt {
                InputFormat::Json | InputFormat::Auto => {
                    serde_json::to_string_pretty(&self.value)
                        .map_err(|e| e.to_string())
                        .and_then(|c| fs::write(&filename, &c).map_err(|e| e.to_string()))
                        .map(|_| filename)
                }
                InputFormat::Yaml => {
                    serde_yaml::to_string(&self.value)
                        .map_err(|e| e.to_string())
                        .and_then(|c| fs::write(&filename, &c).map_err(|e| e.to_string()))
                        .map(|_| filename)
                }
                InputFormat::Toml => {
                    toml::to_string_pretty(&self.value)
                        .map_err(|e| e.to_string())
                        .and_then(|c| fs::write(&filename, &c).map_err(|e| e.to_string()))
                        .map(|_| filename)
                }
                InputFormat::Csv => {
                    self.value_to_csv()
                        .and_then(|c| fs::write(&filename, &c).map_err(|e| e.to_string()))
                        .map(|_| filename)
                }
            };

            match result {
                Ok(f) => saved.push(f),
                Err(e) => errors.push(format!(".{ext}: {e}")),
            }
        }

        // Build result message.
        let msg = if saved.is_empty() {
            format!("Export failed: {}", errors.join("; "))
        } else {
            let label = match self.state.language {
                Language::En => "Exported",
                Language::Zh => "已导出",
            };
            format!("{}: {}", label, saved.join(", "))
        };

        let turn = Turn {
            filter: ":export".into(),
            output: msg,
            mode: self.state.mode,
            token_count: 0,
            result_count: saved.len(),
            is_error: saved.is_empty(),
        };
        self.state.turns.push(turn);
        self.state.scroll_to_bottom();
        self.state.input_mode = InputMode::Filter;
        self.state.clear_input();
        self.render();
    }

    /// Convert the current value to CSV (only works for arrays of flat objects).
    fn value_to_csv(&self) -> Result<String, String> {
        let arr = match &self.value {
            Value::Array(a) => a,
            _ => return Err("CSV requires array data".into()),
        };

        // Collect all field names.
        let mut fields: Vec<String> = Vec::new();
        for item in arr {
            if let Value::Object(obj) = item {
                for key in obj.keys() {
                    if !fields.contains(key) {
                        fields.push(key.clone());
                    }
                }
            }
        }
        if fields.is_empty() {
            return Err("CSV requires objects with fields".into());
        }

        let mut wtr = csv::Writer::from_writer(Vec::new());
        wtr.write_record(&fields).map_err(|e| e.to_string())?;

        for item in arr {
            if let Value::Object(obj) = item {
                let row: Vec<String> = fields
                    .iter()
                    .map(|f| {
                        obj.get(f)
                            .map(|v| match v {
                                Value::String(s) => s.clone(),
                                Value::Number(n) => n.to_string(),
                                Value::Bool(b) => b.to_string(),
                                Value::Null => String::new(),
                                _ => v.to_string(),
                            })
                            .unwrap_or_default()
                    })
                    .collect();
                wtr.write_record(&row).map_err(|e| e.to_string())?;
            }
        }

        let data = wtr.into_inner().map_err(|e| e.to_string())?;
        String::from_utf8(data).map_err(|e| e.to_string())
    }

    /// Recall a history entry into the input buffer.
    fn recall_history(&mut self, direction: HistoryDirection) {
        if self.state.filter_history.is_empty() {
            return;
        }

        let new_idx = match direction {
            HistoryDirection::Up => match self.state.history_index {
                None => Some(self.state.filter_history.len() - 1),
                Some(0) => Some(0),
                Some(i) => Some(i - 1),
            },
            HistoryDirection::Down => match self.state.history_index {
                None => None,
                Some(i) => {
                    if i + 1 < self.state.filter_history.len() {
                        Some(i + 1)
                    } else {
                        None
                    }
                }
            },
        };

        if let Some(idx) = new_idx {
            self.state.current_input = self.state.filter_history[idx].chars().collect();
            self.state.cursor = self.state.current_input.len();
        } else {
            self.state.clear_input();
        }
        self.state.history_index = new_idx;
        self.render();
    }

    /// Render full frame to stdout.
    fn render(&self) {
        let mut stdout = io::stdout();
        if let Err(e) = render_frame(&self.state, self.state.language, &mut stdout) {
            let _ = writeln!(io::stderr(), "render error: {e}");
        }
    }

    /// Handle a single key event.
    ///
    /// Returns `false` when the event loop should exit.
    pub fn handle_key(&mut self, key: event::KeyEvent) -> bool {
        // ── Help overlay takes priority ──
        if self.state.show_help {
            return self.handle_help_key(key);
        }

        // ── File path input mode ──
        if self.state.input_mode == InputMode::FilePath {
            return self.handle_filepath_key(key);
        }

        // ── Export path input mode ──
        if self.state.input_mode == InputMode::ExportPath {
            return self.handle_exportpath_key(key);
        }

        // ── Normal filter input mode ──
        match key.code {
            // ── Quit ──
            KeyCode::Esc => {
                if self.state.input_is_empty() {
                    return false;
                }
                self.state.clear_input();
                self.state.history_index = None;
                self.render();
                true
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => false,

            // ── Execute filter ──
            KeyCode::Enter => {
                self.execute_filter();
                true
            }

            // ── Cycle output mode ──
            KeyCode::Tab => {
                self.cycle_mode();
                true
            }

            // ── History recall ──
            KeyCode::Up if key.modifiers.is_empty() => {
                self.recall_history(HistoryDirection::Up);
                true
            }
            KeyCode::Down if key.modifiers.is_empty() => {
                self.recall_history(HistoryDirection::Down);
                true
            }

            // ── Scroll transcript ──
            KeyCode::PageUp => {
                self.state.scroll_up(10);
                self.render();
                true
            }
            KeyCode::PageDown => {
                self.state.scroll_down(10);
                self.render();
                true
            }
            KeyCode::Up if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.state.scroll_up(1);
                self.render();
                true
            }
            KeyCode::Down if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.state.scroll_down(1);
                self.render();
                true
            }

            // ── Ctrl+S: save output ──
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.save_output();
                true
            }

            // ── Ctrl+O: open file ──
            KeyCode::Char('o') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.enter_file_open_mode();
                true
            }

            // ── Ctrl+E: export data ──
            KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if self.state.has_data {
                    self.enter_export_mode();
                }
                true
            }

            // ── Ctrl+R: reload data ──
            KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.reload_data();
                true
            }

            // ── Ctrl+L: clear transcript ──
            KeyCode::Char('l') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.clear_transcript();
                true
            }

            // ── Ctrl+F: cycle input format ──
            KeyCode::Char('f') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.cycle_input_format();
                true
            }

            // ── Ctrl+G: toggle language ──
            KeyCode::Char('g') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.toggle_language();
                true
            }

            // ── ?: toggle help (only when input is empty) ──
            KeyCode::Char('?') if key.modifiers.is_empty() && self.state.input_is_empty() => {
                self.state.show_help = true;
                self.render();
                true
            }

            // ── Input editing ──
            KeyCode::Backspace => {
                self.state.backspace();
                self.state.history_index = None;
                self.render();
                true
            }
            KeyCode::Left if key.modifiers.is_empty() => {
                self.state.cursor_left();
                self.render();
                true
            }
            KeyCode::Right if key.modifiers.is_empty() => {
                self.state.cursor_right();
                self.render();
                true
            }
            KeyCode::Home => {
                self.state.cursor_to_start();
                self.render();
                true
            }
            KeyCode::End => {
                self.state.cursor_to_end();
                self.render();
                true
            }
            KeyCode::Char(c) if key.modifiers.is_empty() || key.modifiers.contains(KeyModifiers::SHIFT) => {
                self.state.insert_char(c);
                self.state.history_index = None;
                self.render();
                true
            }

            _ => true,
        }
    }

    /// Handle keys when the help overlay is open.
    fn handle_help_key(&mut self, key: event::KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc | KeyCode::Char('?') => {
                self.state.show_help = false;
                self.render();
                true
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => false,
            _ => true,
        }
    }

    /// Handle keys when in file-path input mode.
    fn handle_filepath_key(&mut self, key: event::KeyEvent) -> bool {
        match key.code {
            KeyCode::Enter => {
                let path = self.state.input_string();
                self.load_file(&path);
                true
            }
            KeyCode::Esc => {
                self.state.input_mode = InputMode::Filter;
                self.state.clear_input();
                self.render();
                true
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => false,
            KeyCode::Backspace => {
                self.state.backspace();
                self.render();
                true
            }
            KeyCode::Left if key.modifiers.is_empty() => {
                self.state.cursor_left();
                self.render();
                true
            }
            KeyCode::Right if key.modifiers.is_empty() => {
                self.state.cursor_right();
                self.render();
                true
            }
            KeyCode::Home => {
                self.state.cursor_to_start();
                self.render();
                true
            }
            KeyCode::End => {
                self.state.cursor_to_end();
                self.render();
                true
            }
            KeyCode::Char(c) if key.modifiers.is_empty() || key.modifiers.contains(KeyModifiers::SHIFT) => {
                self.state.insert_char(c);
                self.render();
                true
            }
            KeyCode::Tab => {
                // Tab in file-path mode = autocomplete (future feature).
                // For now, just cycle mode.
                self.cycle_mode();
                true
            }
            _ => true,
        }
    }

    /// Handle keys when in export-path input mode.
    fn handle_exportpath_key(&mut self, key: event::KeyEvent) -> bool {
        match key.code {
            KeyCode::Enter => {
                let path = self.state.input_string();
                self.export_data(&path);
                true
            }
            KeyCode::Esc => {
                self.state.input_mode = InputMode::Filter;
                self.state.clear_input();
                self.render();
                true
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => false,
            KeyCode::Backspace => {
                self.state.backspace();
                self.render();
                true
            }
            KeyCode::Left if key.modifiers.is_empty() => {
                self.state.cursor_left();
                self.render();
                true
            }
            KeyCode::Right if key.modifiers.is_empty() => {
                self.state.cursor_right();
                self.render();
                true
            }
            KeyCode::Home => {
                self.state.cursor_to_start();
                self.render();
                true
            }
            KeyCode::End => {
                self.state.cursor_to_end();
                self.render();
                true
            }
            KeyCode::Char(c) if key.modifiers.is_empty() || key.modifiers.contains(KeyModifiers::SHIFT) => {
                self.state.insert_char(c);
                self.render();
                true
            }
            _ => true,
        }
    }
}

enum HistoryDirection {
    Up,
    Down,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyModifiers};
    use serde_json::json;

    fn test_cli() -> Cli {
        use clap::Parser;
        Cli::try_parse_from(["jqr"]).unwrap()
    }

    fn test_config() -> JqrConfig {
        JqrConfig::default()
    }

    fn test_value() -> Value {
        json!({"users": [{"name": "Alice", "age": 30}, {"name": "Bob", "age": 25}]})
    }

    fn test_session(mode: OutputMode, filter: &str) -> InteractiveSession {
        InteractiveSession::new(
            test_value(),
            test_config(),
            test_cli(),
            mode,
            filter.to_string(),
            true,
            r#"{"users": [{"name": "Alice", "age": 30}]}"#.to_string(),
        )
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    // ── State construction tests ──

    #[test]
    fn test_session_new_defaults() {
        let session = test_session(OutputMode::Envelope, ".");
        assert_eq!(session.state.mode, OutputMode::Envelope);
        assert_eq!(session.state.input_string(), ".");
        assert!(session.state.turns.is_empty());
        assert!(session.state.has_data);
        assert!(!session.state.show_help);
        assert_eq!(session.state.input_mode, InputMode::Filter);
    }

    // ── Key handling: quit ──

    #[test]
    fn test_handle_key_esc_with_empty_input_exits() {
        let mut session = test_session(OutputMode::Envelope, "");
        let cont = session.handle_key(key(KeyCode::Esc));
        assert!(!cont, "Esc with empty input should exit");
    }

    #[test]
    fn test_handle_key_esc_with_input_clears() {
        let mut session = test_session(OutputMode::Envelope, ".users");
        let cont = session.handle_key(key(KeyCode::Esc));
        assert!(cont, "Esc with non-empty input should clear, not exit");
        assert!(session.state.input_is_empty());
    }

    #[test]
    fn test_handle_key_ctrl_c_exits() {
        let mut session = test_session(OutputMode::Envelope, ".users");
        let cont = session.handle_key(ctrl('c'));
        assert!(!cont, "Ctrl+C should exit");
    }

    // ── Key handling: mode cycling ──

    #[test]
    fn test_handle_key_tab_cycles_mode() {
        let mut session = test_session(OutputMode::Envelope, ".");
        session.execute_filter();
        assert_eq!(session.state.mode, OutputMode::Envelope);

        session.handle_key(key(KeyCode::Tab));
        assert_eq!(session.state.mode, OutputMode::SchemaOnly);

        session.handle_key(key(KeyCode::Tab));
        assert_eq!(session.state.mode, OutputMode::Raw);
    }

    #[test]
    fn test_tab_cycle_full_roundtrip() {
        let mut session = test_session(OutputMode::Envelope, ".");
        session.execute_filter();
        let modes = [
            OutputMode::SchemaOnly,
            OutputMode::Raw,
            OutputMode::Compact,
            OutputMode::Pretty,
            OutputMode::Envelope,
        ];
        for expected in &modes {
            session.handle_key(key(KeyCode::Tab));
            assert_eq!(session.state.mode, *expected);
        }
    }

    #[test]
    fn test_tab_re_executes_last_turn_in_new_mode() {
        let mut session = test_session(OutputMode::Envelope, ".");
        session.execute_filter();
        let envelope_output = session.state.turns[0].output.clone();
        assert!(envelope_output.contains("schema"));

        session.handle_key(key(KeyCode::Tab));
        assert_eq!(session.state.turns[0].mode, OutputMode::SchemaOnly);
        // SchemaOnly output should not contain "sample"
        assert!(!session.state.turns[0].output.contains("sample"));
    }

    // ── Key handling: filter execution ──

    #[test]
    fn test_handle_key_enter_executes_filter() {
        let mut session = test_session(OutputMode::Raw, ".users[0].name");
        session.handle_key(key(KeyCode::Enter));
        assert_eq!(session.state.turns.len(), 1);
        assert!(session.state.turns[0].output.contains("Alice"));
        assert!(session.state.input_is_empty());
    }

    #[test]
    fn test_handle_key_enter_adds_to_history() {
        let mut session = test_session(OutputMode::Raw, ".users[0].name");
        session.handle_key(key(KeyCode::Enter));
        assert_eq!(session.state.filter_history.len(), 1);
        assert_eq!(session.state.filter_history[0], ".users[0].name");
    }

    #[test]
    fn test_execute_filter_error_creates_error_turn() {
        let mut session = test_session(OutputMode::Raw, "!!!invalid");
        session.execute_filter();
        assert_eq!(session.state.turns.len(), 1);
        assert!(session.state.turns[0].is_error);
        assert!(session.state.turns[0].output.contains("Error"));
    }

    // ── Key handling: input editing ──

    #[test]
    fn test_handle_key_char_inserts_into_input() {
        let mut session = test_session(OutputMode::Raw, "");
        session.handle_key(key(KeyCode::Char('.')));
        assert_eq!(session.state.input_string(), ".");
        session.handle_key(key(KeyCode::Char('a')));
        assert_eq!(session.state.input_string(), ".a");
    }

    #[test]
    fn test_handle_key_backspace_deletes() {
        let mut session = test_session(OutputMode::Raw, "abc");
        session.handle_key(key(KeyCode::Backspace));
        assert_eq!(session.state.input_string(), "ab");
    }

    #[test]
    fn test_handle_key_left_moves_cursor() {
        let mut session = test_session(OutputMode::Raw, "abc");
        assert_eq!(session.state.cursor, 3);
        session.handle_key(key(KeyCode::Left));
        assert_eq!(session.state.cursor, 2);
    }

    #[test]
    fn test_handle_key_right_moves_cursor() {
        let mut session = test_session(OutputMode::Raw, "abc");
        session.state.cursor = 0;
        session.handle_key(key(KeyCode::Right));
        assert_eq!(session.state.cursor, 1);
    }

    #[test]
    fn test_handle_key_home_end() {
        let mut session = test_session(OutputMode::Raw, "abc");
        session.handle_key(key(KeyCode::Home));
        assert_eq!(session.state.cursor, 0);
        session.handle_key(key(KeyCode::End));
        assert_eq!(session.state.cursor, 3);
    }

    // ── Key handling: history recall ──

    #[test]
    fn test_handle_key_up_recalls_last_filter() {
        let mut session = test_session(OutputMode::Raw, ".a");
        session.handle_key(key(KeyCode::Enter)); // execute ".a"
        // Now input is empty
        assert!(session.state.input_is_empty());
        // Press Up → recall ".a"
        session.handle_key(key(KeyCode::Up));
        assert_eq!(session.state.input_string(), ".a");
    }

    #[test]
    fn test_handle_key_down_after_up_clears() {
        let mut session = test_session(OutputMode::Raw, ".a");
        session.handle_key(key(KeyCode::Enter));
        session.handle_key(key(KeyCode::Up)); // recall ".a"
        assert_eq!(session.state.input_string(), ".a");
        session.handle_key(key(KeyCode::Down)); // back to empty
        assert!(session.state.input_is_empty());
    }

    // ── Key handling: help overlay ──

    #[test]
    fn test_handle_key_question_when_empty_opens_help() {
        let mut session = test_session(OutputMode::Raw, "");
        session.handle_key(key(KeyCode::Char('?')));
        assert!(session.state.show_help);
    }

    #[test]
    fn test_handle_key_question_when_not_empty_types_char() {
        let mut session = test_session(OutputMode::Raw, ".a");
        session.handle_key(key(KeyCode::Char('?')));
        assert!(!session.state.show_help);
        assert_eq!(session.state.input_string(), ".a?");
    }

    #[test]
    fn test_help_esc_closes() {
        let mut session = test_session(OutputMode::Raw, "");
        session.state.show_help = true;
        session.handle_key(key(KeyCode::Esc));
        assert!(!session.state.show_help);
    }

    // ── Key handling: scroll ──

    #[test]
    fn test_handle_key_pageup_scrolls_up() {
        let mut session = test_session(OutputMode::Raw, ".");
        session.execute_filter();
        session.state.scroll_offset = 5;
        session.handle_key(key(KeyCode::PageUp));
        assert!(session.state.scroll_offset < 5);
    }

    #[test]
    fn test_handle_key_pagedown_scrolls_down() {
        let mut session = test_session(OutputMode::Raw, ".");
        session.execute_filter();
        let initial = session.state.scroll_offset;
        session.handle_key(key(KeyCode::PageDown));
        assert!(session.state.scroll_offset >= initial);
    }

    // ── Key handling: Ctrl shortcuts ──

    #[test]
    fn test_ctrl_l_clears_transcript() {
        let mut session = test_session(OutputMode::Raw, ".");
        session.execute_filter();
        assert!(!session.state.turns.is_empty());
        session.handle_key(ctrl('l'));
        assert!(session.state.turns.is_empty());
    }

    #[test]
    fn test_ctrl_g_toggles_language() {
        let mut session = test_session(OutputMode::Raw, ".");
        assert_eq!(session.state.language, Language::En);
        session.handle_key(ctrl('g'));
        assert_eq!(session.state.language, Language::Zh);
        session.handle_key(ctrl('g'));
        assert_eq!(session.state.language, Language::En);
    }

    #[test]
    fn test_ctrl_f_cycles_input_format() {
        let mut session = test_session(OutputMode::Raw, ".");
        assert_eq!(session.state.input_format, InputFormat::Auto);
        session.handle_key(ctrl('f'));
        assert_eq!(session.state.input_format, InputFormat::Json);
        session.handle_key(ctrl('f'));
        assert_eq!(session.state.input_format, InputFormat::Yaml);
    }

    #[test]
    fn test_ctrl_s_save_continues() {
        let mut session = test_session(OutputMode::Raw, ".");
        session.execute_filter();
        let cont = session.handle_key(ctrl('s'));
        assert!(cont, "Ctrl+S should continue the loop");
        // Should have added a save turn
        assert!(session.state.turns.len() >= 2);
        assert!(session.state.turns.last().unwrap().output.contains("Saved"));
    }

    #[test]
    fn test_ctrl_o_enters_filepath_mode() {
        let mut session = test_session(OutputMode::Raw, ".");
        session.handle_key(ctrl('o'));
        assert_eq!(session.state.input_mode, InputMode::FilePath);
    }

    #[test]
    fn test_ctrl_r_reload_continues() {
        let mut session = test_session(OutputMode::Raw, ".");
        session.execute_filter();
        let cont = session.handle_key(ctrl('r'));
        assert!(cont, "Ctrl+R should continue the loop");
    }

    // ── Key handling: other keys ──

    #[test]
    fn test_handle_key_other_continues() {
        let mut session = test_session(OutputMode::Envelope, "");
        let cont = session.handle_key(key(KeyCode::F(1)));
        assert!(cont, "Other keys should continue the loop");
    }

    // ── Token count extraction ──

    #[test]
    fn test_extract_token_count_from_envelope() {
        let output = r#"{"schema":{},"sample":[],"tokens_used":42}"#;
        assert_eq!(extract_token_count(output), 42);
    }

    #[test]
    fn test_extract_token_count_from_raw_returns_zero() {
        let output = r#""Alice""#;
        assert_eq!(extract_token_count(output), 0);
    }

    #[test]
    fn test_extract_token_count_missing_field() {
        let output = r#"{"schema":{}}"#;
        assert_eq!(extract_token_count(output), 0);
    }

    #[test]
    fn test_extract_token_count_large_number() {
        let output = r#"{"tokens_used": 99999}"#;
        assert_eq!(extract_token_count(output), 99999);
    }

    // ── Execute filter with different modes ──

    #[test]
    fn test_execute_filter_envelope_mode() {
        let mut session = test_session(OutputMode::Envelope, ".");
        session.execute_filter();
        assert_eq!(session.state.turns.len(), 1);
        assert!(session.state.turns[0].output.contains("schema"));
        assert!(!session.state.turns[0].is_error);
    }

    #[test]
    fn test_execute_filter_raw_mode() {
        let mut session = test_session(OutputMode::Raw, ".users[0].name");
        session.execute_filter();
        assert!(session.state.turns[0].output.contains("Alice"));
    }

    #[test]
    fn test_execute_filter_compact_mode() {
        let mut session = test_session(OutputMode::Compact, ".");
        session.execute_filter();
        assert!(!session.state.turns[0].output.contains('\n'));
    }

    #[test]
    fn test_execute_filter_schema_only_mode() {
        let mut session = test_session(OutputMode::SchemaOnly, ".");
        session.execute_filter();
        assert!(session.state.turns[0].output.contains("schema"));
        assert!(!session.state.turns[0].output.contains("sample"));
    }

    #[test]
    fn test_multiple_turns_accumulate() {
        let mut session = test_session(OutputMode::Raw, ".users[0].name");
        session.execute_filter();
        session.state.current_input = ".users | length".chars().collect();
        session.state.cursor = session.state.current_input.len();
        session.execute_filter();
        assert_eq!(session.state.turns.len(), 2);
        assert_eq!(session.state.turns[0].filter, ".users[0].name");
        assert_eq!(session.state.turns[1].filter, ".users | length");
    }

    // ── Cycle input format ──

    #[test]
    fn test_cycle_input_format_full_cycle() {
        let mut session = test_session(OutputMode::Raw, ".");
        assert_eq!(session.state.input_format, InputFormat::Auto);
        session.cycle_input_format();
        assert_eq!(session.state.input_format, InputFormat::Json);
        session.cycle_input_format();
        assert_eq!(session.state.input_format, InputFormat::Yaml);
        session.cycle_input_format();
        assert_eq!(session.state.input_format, InputFormat::Toml);
        session.cycle_input_format();
        assert_eq!(session.state.input_format, InputFormat::Csv);
        session.cycle_input_format();
        assert_eq!(session.state.input_format, InputFormat::Auto);
    }

    // ── File path mode ──

    #[test]
    fn test_filepath_mode_esc_cancels() {
        let mut session = test_session(OutputMode::Raw, ".");
        session.enter_file_open_mode();
        assert_eq!(session.state.input_mode, InputMode::FilePath);
        session.handle_key(key(KeyCode::Esc));
        assert_eq!(session.state.input_mode, InputMode::Filter);
    }

    #[test]
    fn test_filepath_mode_enter_loads_nonexistent() {
        let mut session = test_session(OutputMode::Raw, ".");
        session.enter_file_open_mode();
        session.state.current_input = "/nonexistent/file.json".chars().collect();
        session.state.cursor = session.state.current_input.len();
        session.handle_key(key(KeyCode::Enter));
        assert_eq!(session.state.input_mode, InputMode::Filter);
        assert!(session.state.turns.last().unwrap().is_error);
    }
}
