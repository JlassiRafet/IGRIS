//! Putting the terminal back. Every exit path (quit, error, panic, signal) goes through `restore`.
use ratatui::crossterm::event::{
    DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
};
use ratatui::crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::crossterm::{cursor::Show, execute};
use std::io::{self, Write};

/// Turn off everything the app turned on. Safe to call twice and when nothing was turned on.
pub fn restore(out: &mut impl Write) -> io::Result<()> {
    execute!(
        out,
        DisableBracketedPaste,
        DisableMouseCapture,
        Show,
        LeaveAlternateScreen
    )
}

/// Hand the terminal to a child process: the shell's own state, not ours.
pub fn suspend() -> io::Result<()> {
    restore(&mut io::stdout())?;
    disable_raw_mode()
}

/// Take the terminal back after `suspend`.
pub fn resume(mouse: bool) -> io::Result<()> {
    enable_raw_mode()?;
    let mut out = io::stdout();
    execute!(out, EnterAlternateScreen, EnableBracketedPaste)?;
    if mouse {
        execute!(out, EnableMouseCapture)?;
    }
    Ok(())
}

/// Enters raw mode and the alternate screen; restores both when dropped.
pub struct TermGuard;

impl TermGuard {
    pub fn enter(mouse: bool) -> io::Result<TermGuard> {
        enable_raw_mode()?;
        let guard = TermGuard; // from here a failure below still restores
        let mut out = io::stdout();
        execute!(out, EnterAlternateScreen, EnableBracketedPaste)?;
        if mouse {
            execute!(out, EnableMouseCapture)?;
        }
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            // Leave the alternate screen first, or the message is lost with it.
            let _ = restore(&mut io::stdout());
            let _ = disable_raw_mode();
            previous(info);
        }));
        Ok(guard)
    }
}

impl Drop for TermGuard {
    fn drop(&mut self) {
        let _ = restore(&mut io::stdout());
        let _ = disable_raw_mode();
    }
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use super::*;

    #[cfg(unix)] // on Windows these switches are console calls, not bytes
    #[test]
    fn restore_writes_every_off_switch() {
        let mut out = Vec::new();
        restore(&mut out).unwrap();
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("\x1b[?2004l"), "bracketed paste off: {s:?}");
        assert!(
            s.contains("\x1b[?1006l") && s.contains("\x1b[?1003l"),
            "mouse capture off: {s:?}"
        );
        assert!(s.contains("\x1b[?25h"), "cursor shown: {s:?}");
        assert!(s.contains("\x1b[?1049l"), "alternate screen left: {s:?}");
    }
}
