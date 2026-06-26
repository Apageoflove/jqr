use crossterm::cursor;
use crossterm::execute;
use crossterm::style::{
    Attribute, Color, Print, ResetColor, SetAttribute, SetBackgroundColor, SetForegroundColor,
};
use crossterm::terminal::{Clear, ClearType, BeginSynchronizedUpdate, EndSynchronizedUpdate};
use std::io::{self, Write};

use crate::interactive::highlight::highlight_json;
use crate::interactive::{InteractiveState, OutputMode, Language, InputMode, Turn};

// ── Color palette (teal/amber dark theme) ──
const PRIMARY: Color = Color::Rgb { r: 84, g: 184, b: 171 };
const SECONDARY: Color = Color::Rgb { r: 230, g: 165, b: 100 };
const MUTED: Color = Color::Rgb { r: 120, g: 128, b: 138 };
const DARK_MUTED: Color = Color::Rgb { r: 55, g: 62, b: 70 };
const BRIGHT: Color = Color::Rgb { r: 240, g: 243, b: 248 };
const SEPARATOR: Color = Color::Rgb { r: 48, g: 54, b: 61 };
const BG_BAR: Color = Color::Rgb { r: 22, g: 27, b: 34 };
const ERROR_COLOR: Color = Color::Rgb { r: 235, g: 95, b: 95 };
const SUCCESS_COLOR: Color = Color::Rgb { r: 130, g: 200, b: 120 };

/// Render the full interactive frame.
pub fn render_frame(state: &InteractiveState, language: Language, stdout: &mut impl Write) -> io::Result<()> {
    let (cols, rows) = crossterm::terminal::size().unwrap_or((80, 24));
    let w = cols as usize;
    let h = rows as usize;

    // Anti-tearing: buffer the entire frame. Hide cursor during rendering
    // to prevent flicker; render_input_bar will re-show it at the end.
    execute!(stdout, BeginSynchronizedUpdate, cursor::Hide)?;

    if state.show_help {
        render_help_overlay(language, cols, rows, stdout)?;
        execute!(stdout, EndSynchronizedUpdate, cursor::Hide)?;
        return stdout.flush();
    }

    if !state.has_data && state.turns.is_empty() && state.input_mode != InputMode::FilePath {
        render_welcome_screen(state, language, w, h, stdout)?;
        execute!(stdout, EndSynchronizedUpdate)?;
        return stdout.flush();
    }

    // Normal transcript layout:
    // Row 0:       header
    // Row 1:       separator
    // Row 2..h-4:  transcript body
    // Row h-3:     separator
    // Row h-2:     input bar (cursor lives here)
    // Row h-1:     footer hints

    render_header_bar(state, language, w, stdout)?;
    render_sep(1, w, stdout)?;
    render_transcript_body(state, w, h, stdout)?;
    render_sep(h.saturating_sub(3), w, stdout)?;
    render_footer_bar(language, w, h, stdout)?;
    render_input_bar(state, language, w, h, stdout)?;  // MUST be last — cursor lives here

    // Do NOT hide cursor here — render_input_bar has positioned and shown it.
    execute!(stdout, EndSynchronizedUpdate)?;
    stdout.flush()
}

// ═══════════════════════════════════════════════════════════════════════
// Header bar
// ═══════════════════════════════════════════════════════════════════════

fn render_header_bar(state: &InteractiveState, language: Language, w: usize, stdout: &mut impl Write) -> io::Result<()> {
    execute!(stdout, cursor::MoveTo(0, 0), Clear(ClearType::CurrentLine))?;
    execute!(stdout, SetBackgroundColor(BG_BAR), Print(" ".repeat(w)))?;
    execute!(stdout, cursor::MoveTo(0, 0))?;

    // Logo
    execute!(
        stdout,
        SetBackgroundColor(BG_BAR),
        SetForegroundColor(PRIMARY),
        SetAttribute(Attribute::Bold),
        Print(" jqr"),
        SetAttribute(Attribute::Reset),
    )?;

    // Mode indicator
    let mode_label = match language {
        Language::En => " mode: ",
        Language::Zh => " 模式: ",
    };
    execute!(stdout, SetBackgroundColor(BG_BAR), SetForegroundColor(MUTED), Print(mode_label))?;
    execute!(stdout, SetBackgroundColor(BG_BAR), SetForegroundColor(SECONDARY), Print(state.mode.label()))?;

    // Input format
    let fmt_label = match language {
        Language::En => "  fmt: ",
        Language::Zh => "  格式: ",
    };
    execute!(stdout, SetBackgroundColor(BG_BAR), SetForegroundColor(MUTED), Print(fmt_label))?;
    let fmt_str = format!("{:?}", state.input_format).to_lowercase();
    execute!(stdout, SetBackgroundColor(BG_BAR), SetForegroundColor(BRIGHT), Print(&fmt_str))?;

    // Token count
    if state.token_count > 0 {
        let tok_label = match language {
            Language::En => "  tokens: ",
            Language::Zh => "  令牌: ",
        };
        execute!(stdout, SetBackgroundColor(BG_BAR), SetForegroundColor(MUTED), Print(tok_label))?;
        let tok_str = if state.token_budget > 0 {
            format!("{}/{}", state.token_count, state.token_budget)
        } else {
            state.token_count.to_string()
        };
        let tok_color = if state.token_budget > 0 && state.token_count > state.token_budget * 4 / 5 {
            ERROR_COLOR
        } else if state.token_budget > 0 && state.token_count > state.token_budget / 2 {
            SECONDARY
        } else {
            PRIMARY
        };
        execute!(stdout, SetBackgroundColor(BG_BAR), SetForegroundColor(tok_color), Print(&tok_str))?;
    }

    // Mode pills (right-aligned)
    let modes = [OutputMode::Envelope, OutputMode::SchemaOnly, OutputMode::Raw, OutputMode::Compact, OutputMode::Pretty];
    let pills: Vec<String> = modes.iter().enumerate().map(|(i, m)| format!(" {}:{} ", i + 1, m.label())).collect();
    let pills_total: usize = pills.iter().map(|p| p.chars().count()).sum();
    let pills_x = w.saturating_sub(pills_total + 2);

    execute!(stdout, cursor::MoveTo(pills_x as u16, 0))?;
    for (i, m) in modes.iter().enumerate() {
        if *m == state.mode {
            execute!(
                stdout,
                SetBackgroundColor(PRIMARY),
                SetForegroundColor(BG_BAR),
                SetAttribute(Attribute::Bold),
                Print(pills[i].as_str()),
                SetAttribute(Attribute::Reset),
            )?;
        } else {
            execute!(
                stdout,
                SetBackgroundColor(DARK_MUTED),
                SetForegroundColor(MUTED),
                Print(pills[i].as_str()),
            )?;
        }
    }

    execute!(stdout, ResetColor)?;
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// Transcript body (scrollable)
// ═══════════════════════════════════════════════════════════════════════

fn render_transcript_body(state: &InteractiveState, w: usize, h: usize, stdout: &mut impl Write) -> io::Result<()> {
    let body_start = 2;
    let body_end = h.saturating_sub(4); // exclusive (sep at h-3, input at h-2, footer at h-1)
    let body_height = body_end.saturating_sub(body_start);

    execute!(stdout, cursor::MoveTo(0, body_start as u16), Clear(ClearType::FromCursorDown))?;

    let total_lines = state.total_transcript_lines();
    let max_scroll = total_lines.saturating_sub(body_height);
    let scroll = state.scroll_offset.min(max_scroll);

    // Iterate through turns, building visible lines.
    let mut line_idx: usize = 0;
    let mut rendered: usize = 0;

    for turn in &state.turns {
        // ── Filter line ──
        if line_idx >= scroll && rendered < body_height {
            let row = (body_start + rendered) as u16;
            render_filter_line(turn, w, row, state.language, stdout)?;
            rendered += 1;
        }
        line_idx += 1;

        // ── Output lines ──
        for out_line in turn.output.lines() {
            if line_idx >= scroll && rendered < body_height {
                let row = (body_start + rendered) as u16;
                render_output_line(out_line, turn.is_error, w, row, stdout)?;
                rendered += 1;
            }
            line_idx += 1;
            if rendered >= body_height {
                break;
            }
        }

        // ── Blank separator ──
        if line_idx >= scroll && rendered < body_height {
            let row = (body_start + rendered) as u16;
            execute!(stdout, cursor::MoveTo(0, row), Clear(ClearType::CurrentLine))?;
            rendered += 1;
        }
        line_idx += 1;

        if rendered >= body_height {
            break;
        }
    }

    // Fill remaining body with blank lines.
    while rendered < body_height {
        let row = (body_start + rendered) as u16;
        execute!(stdout, cursor::MoveTo(0, row), Clear(ClearType::CurrentLine))?;
        rendered += 1;
    }

    Ok(())
}

fn render_filter_line(turn: &Turn, w: usize, row: u16, language: Language, stdout: &mut impl Write) -> io::Result<()> {
    execute!(stdout, cursor::MoveTo(0, row), Clear(ClearType::CurrentLine))?;

    // Localized labels
    let tok_label = match language { Language::En => "tok", Language::Zh => "令牌" };
    let err_label = match language { Language::En => "ERROR", Language::Zh => "错误" };
    let ok_label = match language { Language::En => "OK", Language::Zh => "完成" };

    // Prompt arrow
    execute!(stdout, SetForegroundColor(PRIMARY), Print("  \u{203a} "))?;

    // Filter text (truncated if too long)
    let prefix_len = 5; // "  › "
    let badge_len = turn.mode.label().len() + 4; // "  [Mode]"
    let status_len = if turn.is_error { err_label.len() + 2 } else if turn.filter.starts_with(':') { ok_label.chars().count() + 2 } else { 0 };
    let token_len = if turn.token_count > 0 { format!("  {} {}", turn.token_count, tok_label).chars().count() } else { 0 };
    let max_filter_chars = w.saturating_sub(prefix_len + badge_len + status_len + token_len);
    let filter_display = truncate_at_char_boundary(&turn.filter, max_filter_chars);
    execute!(stdout, SetForegroundColor(BRIGHT), Print(filter_display))?;

    // Mode badge
    let mode_badge = format!("  [{}]", turn.mode.label());
    execute!(stdout, SetForegroundColor(MUTED), Print(&mode_badge))?;

    // Status indicator
    if turn.is_error {
        execute!(stdout, SetForegroundColor(ERROR_COLOR), Print(format!("  {}", err_label)))?;
    } else if turn.filter.starts_with(':') {
        execute!(stdout, SetForegroundColor(SUCCESS_COLOR), Print(format!("  {}", ok_label)))?;
    }

    // Token count (per-turn)
    if turn.token_count > 0 {
        execute!(stdout, SetForegroundColor(MUTED), Print(format!("  {} {}", turn.token_count, tok_label)))?;
    }

    execute!(stdout, ResetColor)?;
    Ok(())
}

fn render_output_line(line: &str, is_error: bool, w: usize, row: u16, stdout: &mut impl Write) -> io::Result<()> {
    execute!(stdout, cursor::MoveTo(0, row), Clear(ClearType::CurrentLine))?;

    // Indent
    execute!(stdout, Print("  "))?;

    let max_chars = w.saturating_sub(4);

    // Truncate at char boundary
    let display = truncate_at_char_boundary(line, max_chars);

    if is_error {
        execute!(stdout, SetForegroundColor(ERROR_COLOR), Print(&display), ResetColor)?;
    } else {
        let segments = highlight_json(display);
        for seg in &segments {
            execute!(stdout, SetForegroundColor(seg.color), Print(&seg.text))?;
        }
        execute!(stdout, ResetColor)?;
    }

    Ok(())
}

fn truncate_at_char_boundary(s: &str, max_chars: usize) -> &str {
    if max_chars == 0 {
        return s;
    }
    if s.chars().count() <= max_chars {
        return s;
    }
    // Return the first `max_chars` characters by finding the byte index
    // of the (max_chars)th character (0-indexed) and slicing up to it.
    s.char_indices()
        .nth(max_chars)
        .map(|(idx, _)| &s[..idx])
        .unwrap_or(s)
}

// ═══════════════════════════════════════════════════════════════════════
// Input bar (always at bottom, always focused)
// ═══════════════════════════════════════════════════════════════════════

fn render_input_bar(state: &InteractiveState, language: Language, w: usize, h: usize, stdout: &mut impl Write) -> io::Result<()> {
    let row = (h.saturating_sub(2)) as u16;
    execute!(stdout, cursor::MoveTo(0, row), Clear(ClearType::CurrentLine))?;
    execute!(stdout, SetBackgroundColor(BG_BAR), Print(" ".repeat(w)))?;
    execute!(stdout, cursor::MoveTo(0, row))?;

    match state.input_mode {
        InputMode::Filter => {
            let prompt = match language {
                Language::En => " \u{203a} ",
                Language::Zh => " \u{203a} ",
            };
            execute!(stdout, SetBackgroundColor(BG_BAR), SetForegroundColor(PRIMARY), SetAttribute(Attribute::Bold), Print(prompt), SetAttribute(Attribute::Reset))?;

            let input_str = state.input_string();
            execute!(stdout, SetBackgroundColor(BG_BAR), SetForegroundColor(BRIGHT), Print(&input_str))?;

            // Position cursor
            let prompt_len = 3; // " › "
            let cursor_x = (prompt_len + state.cursor) as u16;
            execute!(stdout, cursor::MoveTo(cursor_x, row), cursor::Show)?;
        }
        InputMode::FilePath => {
            let prompt = match language {
                Language::En => " open file: ",
                Language::Zh => " 打开文件: ",
            };
            execute!(stdout, SetBackgroundColor(BG_BAR), SetForegroundColor(SECONDARY), SetAttribute(Attribute::Bold), Print(prompt), SetAttribute(Attribute::Reset))?;

            let input_str = state.input_string();
            execute!(stdout, SetBackgroundColor(BG_BAR), SetForegroundColor(BRIGHT), Print(&input_str))?;

            let prompt_len = prompt.chars().count();
            let cursor_x = (prompt_len + state.cursor) as u16;
            execute!(stdout, cursor::MoveTo(cursor_x, row), cursor::Show)?;
        }
        InputMode::ExportPath => {
            let prompt = match language {
                Language::En => " export to: ",
                Language::Zh => " 导出至: ",
            };
            execute!(stdout, SetBackgroundColor(BG_BAR), SetForegroundColor(SECONDARY), SetAttribute(Attribute::Bold), Print(prompt), SetAttribute(Attribute::Reset))?;

            let input_str = state.input_string();
            let hint = match language {
                Language::En => "  (.json/.yaml/.toml/.csv will be appended)",
                Language::Zh => "  (自动添加 .json/.yaml/.toml/.csv)",
            };
            execute!(stdout, SetBackgroundColor(BG_BAR), SetForegroundColor(BRIGHT), Print(&input_str))?;
            execute!(stdout, SetBackgroundColor(BG_BAR), SetForegroundColor(MUTED), Print(hint))?;

            let prompt_len = prompt.chars().count();
            let cursor_x = (prompt_len + state.cursor) as u16;
            execute!(stdout, cursor::MoveTo(cursor_x, row), cursor::Show)?;
        }
    }

    execute!(stdout, ResetColor)?;
    stdout.flush()
}

// ═══════════════════════════════════════════════════════════════════════
// Footer bar (keybinding hints)
// ═══════════════════════════════════════════════════════════════════════

fn render_footer_bar(language: Language, w: usize, h: usize, stdout: &mut impl Write) -> io::Result<()> {
    let row = (h.saturating_sub(1)) as u16;
    execute!(stdout, cursor::MoveTo(0, row), Clear(ClearType::CurrentLine))?;
    execute!(stdout, SetBackgroundColor(BG_BAR), Print(" ".repeat(w)))?;
    execute!(stdout, cursor::MoveTo(0, row))?;
    let text = match language {
        Language::En => " Enter run  Tab mode  Ctrl+O open  Ctrl+E export  Ctrl+G lang  ? help  Esc quit ",
        Language::Zh => " Enter 运行  Tab 模式  Ctrl+O 打开  Ctrl+E 导出  Ctrl+G 语言  ? 帮助  Esc 退出 ",
    };
    execute!(
        stdout,
        SetBackgroundColor(BG_BAR),
        SetForegroundColor(MUTED),
        Print(text),
        ResetColor,
    )
}

// ═══════════════════════════════════════════════════════════════════════
// Separator
// ═══════════════════════════════════════════════════════════════════════

fn render_sep(row: usize, w: usize, stdout: &mut impl Write) -> io::Result<()> {
    execute!(
        stdout,
        cursor::MoveTo(0, row as u16),
        Clear(ClearType::CurrentLine),
        SetForegroundColor(SEPARATOR),
        Print("\u{2500}".repeat(w)),
        ResetColor,
    )
}

// ═══════════════════════════════════════════════════════════════════════
// Welcome screen (no data)
// ═══════════════════════════════════════════════════════════════════════

fn render_welcome_screen(state: &InteractiveState, language: Language, w: usize, h: usize, stdout: &mut impl Write) -> io::Result<()> {
    // Header
    render_header_bar(state, language, w, stdout)?;
    render_sep(1, w, stdout)?;

    // Welcome message centered in body
    let lines: Vec<&str> = match language {
        Language::En => vec![
            "",
            "  jqr — JSON Query Studio",
            "",
            "  Pipe data in or open a file:",
            "",
            "    echo '{\"users\":[{\"name\":\"Alice\"}]}' | jqr",
            "",
            "  ── Keybindings ──────────────────────────",
            "  Ctrl+O   open file (JSON/YAML/TOML/CSV)",
            "  Ctrl+F   cycle input format",
            "  Ctrl+E   export data to multiple formats",
            "  Ctrl+S   save current output",
            "  Ctrl+G   toggle English / 中文",
            "  Tab      cycle output mode",
            "  ?        help    Esc  quit",
            "",
        ],
        Language::Zh => vec![
            "",
            "  jqr — JSON 查询工作室",
            "",
            "  传入数据或打开文件开始使用：",
            "",
            "    echo '{\"users\":[{\"name\":\"Alice\"}]}' | jqr",
            "",
            "  ── 快捷键 ────────────────────────────────",
            "  Ctrl+O   打开文件 (JSON/YAML/TOML/CSV)",
            "  Ctrl+F   切换输入格式",
            "  Ctrl+E   导出数据为多种格式",
            "  Ctrl+S   保存当前输出",
            "  Ctrl+G   切换 中文 / English",
            "  Tab      切换输出模式",
            "  ?        帮助    Esc  退出",
            "",
        ],
    };

    let body_start = 2;
    let body_end = h.saturating_sub(4);
    let body_height = body_end.saturating_sub(body_start);
    let start_row = body_start + body_height.saturating_sub(lines.len()) / 2;

    execute!(stdout, cursor::MoveTo(0, body_start as u16), Clear(ClearType::FromCursorDown))?;

    for (i, line) in lines.iter().enumerate() {
        let row = (start_row + i) as u16;
        let color = if line.contains("jqr") { PRIMARY } else if line.starts_with("    ") { SECONDARY } else { BRIGHT };
        execute!(stdout, cursor::MoveTo(0, row), SetForegroundColor(color), Print(line), ResetColor)?;
    }

    render_sep(h.saturating_sub(3), w, stdout)?;

    // Input bar (still active)
    let row = (h.saturating_sub(2)) as u16;
    execute!(stdout, cursor::MoveTo(0, row), Clear(ClearType::CurrentLine))?;
    execute!(stdout, SetBackgroundColor(BG_BAR), Print(" ".repeat(w)))?;
    execute!(stdout, cursor::MoveTo(0, row))?;
    let prompt = match language {
        Language::En => " \u{203a} ",
        Language::Zh => " \u{203a} ",
    };
    execute!(stdout, SetBackgroundColor(BG_BAR), SetForegroundColor(MUTED), Print(prompt))?;
    let hint = match language {
        Language::En => "(type a filter or Ctrl+O to open a file)",
        Language::Zh => "(输入过滤器或按 Ctrl+O 打开文件)",
    };
    execute!(stdout, SetBackgroundColor(BG_BAR), SetForegroundColor(MUTED), Print(hint), ResetColor)?;

    // Footer
    render_footer_bar(language, w, h, stdout)?;

    // Show cursor at the input prompt position.
    let cursor_x = 3u16; // " › "
    execute!(stdout, cursor::MoveTo(cursor_x, row), cursor::Show)?;

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// Help overlay
// ═══════════════════════════════════════════════════════════════════════

fn render_help_overlay(language: Language, w: u16, h: u16, stdout: &mut impl Write) -> io::Result<()> {
    if w < 40 || h < 20 {
        let msg = match language {
            Language::En => "Terminal too small for help",
            Language::Zh => "终端太小，无法显示帮助",
        };
        execute!(stdout, cursor::MoveTo(0, 0), Clear(ClearType::All), Print(msg))?;
        return Ok(());
    }

    let max_width = 62u16.min(w.saturating_sub(4));
    let inside_width = max_width.saturating_sub(2) as usize;
    let overlay_x = (w.saturating_sub(max_width)) / 2;
    let overlay_height: u16 = 24;
    let overlay_y = (h.saturating_sub(overlay_height)) / 2;

    fn border_line(stdout: &mut impl Write, x: u16, row: u16, iw: usize, left: &str, mid: &str, right: &str) -> io::Result<()> {
        execute!(
            stdout,
            cursor::MoveTo(x, row),
            SetBackgroundColor(BG_BAR),
            SetForegroundColor(SEPARATOR),
            Print(left),
            Print(mid.repeat(iw)),
            Print(right),
        )
    }

    fn empty_line(stdout: &mut impl Write, x: u16, row: u16, iw: usize) -> io::Result<()> {
        execute!(
            stdout,
            cursor::MoveTo(x, row),
            SetBackgroundColor(BG_BAR),
            SetForegroundColor(SEPARATOR),
            Print("\u{2502}"),
            Print(" ".repeat(iw)),
            Print("\u{2502}"),
        )
    }

    fn title_line(stdout: &mut impl Write, x: u16, row: u16, iw: usize, text: &str) -> io::Result<()> {
        let text_len = text.chars().count() as u16;
        let total_pad = (iw as u16).saturating_sub(text_len);
        let left_pad = total_pad / 2;
        let right_pad = total_pad - left_pad;
        execute!(
            stdout,
            cursor::MoveTo(x, row),
            SetBackgroundColor(BG_BAR),
            SetForegroundColor(SEPARATOR),
            Print("\u{2502}"),
            Print(" ".repeat(left_pad as usize)),
            SetForegroundColor(PRIMARY),
            SetAttribute(Attribute::Bold),
            Print(text),
            SetAttribute(Attribute::Reset),
            SetBackgroundColor(BG_BAR),
            SetForegroundColor(SEPARATOR),
            Print(" ".repeat(right_pad as usize)),
            Print("\u{2502}"),
        )
    }

    fn section_header(stdout: &mut impl Write, x: u16, row: u16, iw: usize, text: &str) -> io::Result<()> {
        let text_len = text.chars().count() as u16;
        let trailing = (iw as u16).saturating_sub(text_len).saturating_sub(2);
        execute!(
            stdout,
            cursor::MoveTo(x, row),
            SetBackgroundColor(BG_BAR),
            SetForegroundColor(SEPARATOR),
            Print("\u{2502}"),
            Print("  "),
            SetForegroundColor(BRIGHT),
            SetAttribute(Attribute::Bold),
            Print(text),
            SetAttribute(Attribute::Reset),
            SetBackgroundColor(BG_BAR),
            SetForegroundColor(SEPARATOR),
            Print(" ".repeat(trailing as usize)),
            Print("\u{2502}"),
        )
    }

    fn shortcut_row(stdout: &mut impl Write, x: u16, row: u16, iw: usize, key: &str, desc: &str) -> io::Result<()> {
        let key_len = key.chars().count() as u16;
        let desc_len = desc.chars().count() as u16;
        let desc_start: u16 = 18;
        let trailing = (iw as u16).saturating_sub(desc_start).saturating_sub(desc_len);
        let key_gap = desc_start.saturating_sub(4).saturating_sub(key_len);
        execute!(
            stdout,
            cursor::MoveTo(x, row),
            SetBackgroundColor(BG_BAR),
            SetForegroundColor(SEPARATOR),
            Print("\u{2502}"),
            Print("    "),
            SetForegroundColor(MUTED),
            Print(key),
            Print(" ".repeat(key_gap as usize)),
            Print(desc),
            Print(" ".repeat(trailing as usize)),
            SetForegroundColor(SEPARATOR),
            Print("\u{2502}"),
        )
    }

    border_line(stdout, overlay_x, overlay_y, inside_width, "\u{250C}", "\u{2500}", "\u{2510}")?;

    let title = match language {
        Language::En => "jqr — Keyboard Reference",
        Language::Zh => "jqr — 快捷键参考",
    };
    title_line(stdout, overlay_x, overlay_y + 1, inside_width, title)?;
    empty_line(stdout, overlay_x, overlay_y + 2, inside_width)?;

    let mut row = overlay_y + 3;

    match language {
        Language::En => {
            section_header(stdout, overlay_x, row, inside_width, "Input")?;
            row += 1;
            shortcut_row(stdout, overlay_x, row, inside_width, "Enter", "execute filter")?;
            row += 1;
            shortcut_row(stdout, overlay_x, row, inside_width, "Tab", "cycle output mode")?;
            row += 1;
            shortcut_row(stdout, overlay_x, row, inside_width, "Backspace", "delete char")?;
            row += 1;
            shortcut_row(stdout, overlay_x, row, inside_width, "Left/Right", "move cursor")?;
            row += 1;
            shortcut_row(stdout, overlay_x, row, inside_width, "Home/End", "line start/end")?;
            row += 1;
            empty_line(stdout, overlay_x, row, inside_width)?;
            row += 1;

            section_header(stdout, overlay_x, row, inside_width, "History & Scroll")?;
            row += 1;
            shortcut_row(stdout, overlay_x, row, inside_width, "Up/Down", "recall filter history")?;
            row += 1;
            shortcut_row(stdout, overlay_x, row, inside_width, "PgUp/PgDn", "scroll transcript")?;
            row += 1;
            shortcut_row(stdout, overlay_x, row, inside_width, "Ctrl+Up/Dn", "scroll 1 line")?;
            row += 1;
            empty_line(stdout, overlay_x, row, inside_width)?;
            row += 1;

            section_header(stdout, overlay_x, row, inside_width, "Actions")?;
            row += 1;
            shortcut_row(stdout, overlay_x, row, inside_width, "Ctrl+S", "save output to file")?;
            row += 1;
            shortcut_row(stdout, overlay_x, row, inside_width, "Ctrl+O", "open file")?;
            row += 1;
            shortcut_row(stdout, overlay_x, row, inside_width, "Ctrl+R", "reload input data")?;
            row += 1;
            shortcut_row(stdout, overlay_x, row, inside_width, "Ctrl+L", "clear transcript")?;
            row += 1;
            shortcut_row(stdout, overlay_x, row, inside_width, "Ctrl+F", "cycle input format")?;
            row += 1;
            shortcut_row(stdout, overlay_x, row, inside_width, "Ctrl+G", "toggle language")?;
            row += 1;
            shortcut_row(stdout, overlay_x, row, inside_width, "?", "toggle this help")?;
            row += 1;
            shortcut_row(stdout, overlay_x, row, inside_width, "Esc", "quit (or cancel)")?;
            row += 1;
            shortcut_row(stdout, overlay_x, row, inside_width, "Ctrl+C", "force quit")?;
        }
        Language::Zh => {
            section_header(stdout, overlay_x, row, inside_width, "输入")?;
            row += 1;
            shortcut_row(stdout, overlay_x, row, inside_width, "Enter", "执行过滤器")?;
            row += 1;
            shortcut_row(stdout, overlay_x, row, inside_width, "Tab", "切换输出模式")?;
            row += 1;
            shortcut_row(stdout, overlay_x, row, inside_width, "Backspace", "删除字符")?;
            row += 1;
            shortcut_row(stdout, overlay_x, row, inside_width, "Left/Right", "移动光标")?;
            row += 1;
            shortcut_row(stdout, overlay_x, row, inside_width, "Home/End", "行首/行尾")?;
            row += 1;
            empty_line(stdout, overlay_x, row, inside_width)?;
            row += 1;

            section_header(stdout, overlay_x, row, inside_width, "历史与滚动")?;
            row += 1;
            shortcut_row(stdout, overlay_x, row, inside_width, "Up/Down", "召回过滤历史")?;
            row += 1;
            shortcut_row(stdout, overlay_x, row, inside_width, "PgUp/PgDn", "滚动记录")?;
            row += 1;
            shortcut_row(stdout, overlay_x, row, inside_width, "Ctrl+Up/Dn", "滚动 1 行")?;
            row += 1;
            empty_line(stdout, overlay_x, row, inside_width)?;
            row += 1;

            section_header(stdout, overlay_x, row, inside_width, "操作")?;
            row += 1;
            shortcut_row(stdout, overlay_x, row, inside_width, "Ctrl+S", "保存输出到文件")?;
            row += 1;
            shortcut_row(stdout, overlay_x, row, inside_width, "Ctrl+O", "打开文件")?;
            row += 1;
            shortcut_row(stdout, overlay_x, row, inside_width, "Ctrl+R", "重新加载数据")?;
            row += 1;
            shortcut_row(stdout, overlay_x, row, inside_width, "Ctrl+L", "清空记录")?;
            row += 1;
            shortcut_row(stdout, overlay_x, row, inside_width, "Ctrl+F", "切换输入格式")?;
            row += 1;
            shortcut_row(stdout, overlay_x, row, inside_width, "Ctrl+G", "切换语言")?;
            row += 1;
            shortcut_row(stdout, overlay_x, row, inside_width, "?", "切换此帮助")?;
            row += 1;
            shortcut_row(stdout, overlay_x, row, inside_width, "Esc", "退出 (或取消)")?;
            row += 1;
            shortcut_row(stdout, overlay_x, row, inside_width, "Ctrl+C", "强制退出")?;
        }
    }

    border_line(stdout, overlay_x, row + 1, inside_width, "\u{2514}", "\u{2500}", "\u{2518}")?;

    execute!(stdout, ResetColor)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interactive::{InteractiveState, OutputMode, Turn};

    fn make_turn(filter: &str, output: &str, is_error: bool) -> Turn {
        Turn {
            filter: filter.to_string(),
            output: output.to_string(),
            mode: OutputMode::Envelope,
            token_count: 42,
            result_count: output.lines().count(),
            is_error,
        }
    }

    #[test]
    fn test_truncate_short_string() {
        assert_eq!(truncate_at_char_boundary("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_exact_length() {
        assert_eq!(truncate_at_char_boundary("hello", 5), "hello");
    }

    #[test]
    fn test_truncate_long_string() {
        assert_eq!(truncate_at_char_boundary("hello world", 5), "hello");
    }

    #[test]
    fn test_truncate_cjk() {
        // CJK chars: each is 1 char (but display width is 2)
        let s = "中文测试";
        assert_eq!(truncate_at_char_boundary(s, 2), "中文");
    }

    #[test]
    fn test_truncate_empty() {
        assert_eq!(truncate_at_char_boundary("", 5), "");
    }

    #[test]
    fn test_truncate_zero_max() {
        assert_eq!(truncate_at_char_boundary("hello", 0), "hello");
    }

    #[test]
    fn test_turn_line_count_basic() {
        let turn = make_turn(".users", "line1\nline2\nline3", false);
        // 1 (filter) + 3 (output) + 1 (blank) = 5
        assert_eq!(turn.line_count(), 5);
    }

    #[test]
    fn test_turn_line_count_empty_output() {
        let turn = make_turn(".", "", false);
        // 1 (filter) + 0 (output) + 1 (blank) = 2
        assert_eq!(turn.line_count(), 2);
    }

    #[test]
    fn test_state_total_lines_empty() {
        let state = InteractiveState::new(OutputMode::Raw, ".".to_string());
        assert_eq!(state.total_transcript_lines(), 0);
    }

    #[test]
    fn test_state_total_lines_with_turns() {
        let mut state = InteractiveState::new(OutputMode::Raw, ".".to_string());
        state.turns.push(make_turn(".a", "1\n2", false));
        state.turns.push(make_turn(".b", "3", false));
        // turn1: 1+2+1=4, turn2: 1+1+1=3, total=7
        assert_eq!(state.total_transcript_lines(), 7);
    }
}
