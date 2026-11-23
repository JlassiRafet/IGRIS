//! Run a fixed command on this terminal: leave the app screen, run it, come back.
use super::term;
use std::io;
use std::process::{Command, ExitStatus};

/// `argv` must be a constant from `fix.rs`. The terminal is handed over for the duration and
/// taken back afterwards, also when the command fails to start.
pub fn run_attached(argv: &'static [&'static str], mouse: bool) -> io::Result<ExitStatus> {
    term::suspend()?;
    let status = Command::new(argv[0]).args(&argv[1..]).status();
    term::resume(mouse)?;
    status
}
