use std::io;
use std::panic;

use crossterm::terminal::{disable_raw_mode, enable_raw_mode};

pub struct TerminalGuard;

impl TerminalGuard {
    pub fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        let prev_hook = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            let _ = disable_raw_mode();
            prev_hook(info);
        }));
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::IsTerminal;

    fn have_tty() -> bool {
        std::io::stdout().is_terminal()
    }

    #[test]
    fn test_terminal_guard_enter_ok() {
        if !have_tty() {
            return;
        }
        let guard = TerminalGuard::enter();
        assert!(guard.is_ok());
        drop(guard);
    }

    #[test]
    fn test_terminal_guard_drop_no_panic() {
        if !have_tty() {
            return;
        }
        let guard = TerminalGuard::enter().unwrap();
        drop(guard);
    }

    #[test]
    fn test_terminal_guard_double_drop_no_panic() {
        if !have_tty() {
            return;
        }
        let guard = TerminalGuard::enter().unwrap();
        drop(guard);
        let _ = disable_raw_mode();
    }
}
