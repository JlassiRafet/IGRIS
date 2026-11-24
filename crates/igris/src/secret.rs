//! A key typed into IGRIS (decision 0013, method B). It goes to the stdin of a tool's own
//! store-the-key command and nowhere else: never argv (the argv type is `'static`, so a runtime
//! value cannot be in it), never a file, never the log, never the output shown back.
use crate::activity::{Activity, Entry};
use crate::clean::last_lines;
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Instant;

pub struct Secret(String);

impl Secret {
    pub fn new(s: String) -> Secret {
        Secret(s)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(***)")
    }
}

impl Drop for Secret {
    fn drop(&mut self) {
        // Overwrite the bytes where they are (no copy); volatile writes cannot be optimised away.
        let mut bytes = std::mem::take(&mut self.0).into_bytes();
        for b in &mut bytes {
            // SAFETY: `b` is a valid, aligned, exclusive reference to one byte.
            unsafe { std::ptr::write_volatile(b, 0) };
        }
        std::hint::black_box(&bytes);
    }
}

/// How a command ended: its exit code (`None` when it did not start or was killed) and the last
/// 20 lines it printed, with the key replaced by `***` and every escape sequence removed.
pub struct Outcome {
    pub code: Option<i32>,
    pub tail: Vec<String>,
}

/// Run `argv` with the key on its stdin (newline-terminated, then closed) and log the command.
pub fn run_with_stdin(
    argv: &'static [&'static str],
    secret: Secret,
    log: &mut Activity,
) -> Outcome {
    let started = Instant::now();
    let (code, text) = match argv.split_first() {
        Some((program, args)) => run(program, args, &secret),
        None => (None, "no command to run".to_string()),
    };
    log.push(Entry {
        argv: argv.iter().map(|a| (*a).to_string()).collect(),
        code,
        secs: started.elapsed().as_secs_f32(),
    });
    Outcome {
        code,
        tail: tail(&text, &secret),
    }
}

fn run(program: &str, args: &[&str], secret: &Secret) -> (Option<i32>, String) {
    let mut child = match Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return (None, format!("could not start {program}: {e}")),
    };
    if let Some(mut stdin) = child.stdin.take() {
        // A tool that exits before reading closes the pipe; that is its answer, not our error.
        let _ = stdin
            .write_all(secret.as_str().as_bytes())
            .and_then(|()| stdin.write_all(b"\n"));
    }
    match child.wait_with_output() {
        Ok(out) => {
            let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
            text.push_str(&String::from_utf8_lossy(&out.stderr));
            (out.status.code(), text)
        }
        Err(e) => (None, format!("could not read the output of {program}: {e}")),
    }
}

fn tail(text: &str, secret: &Secret) -> Vec<String> {
    let redacted = if secret.is_empty() {
        text.to_string()
    } else {
        text.replace(secret.as_str(), "***")
    };
    last_lines(&redacted, 20)
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "sk-test-123";

    /// Prints how many characters arrived on stdin, then the key itself on stdout and stderr.
    #[cfg(unix)]
    const ECHO: &[&str] = &[
        "sh",
        "-c",
        "read k; echo \"len=${#k}\"; echo \"out=$k\"; echo \"err=$k\" >&2; exit 3",
    ];

    fn secret() -> Secret {
        Secret::new(KEY.into())
    }

    #[test]
    fn debug_of_a_secret_is_masked() {
        assert_eq!(format!("{:?}", secret()), "Secret(***)");
        assert!(!secret().is_empty());
    }

    #[test]
    fn a_panic_message_with_a_secret_in_scope_hides_it() {
        let s = secret();
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            panic!("failed with {s:?}");
        }))
        .unwrap_err();
        let msg = caught.downcast_ref::<String>().cloned().unwrap_or_default();
        assert!(msg.contains("Secret(***)") && !msg.contains(KEY), "{msg}");
    }

    #[cfg(unix)]
    #[test]
    fn the_key_reaches_stdin_and_the_exit_code_is_reported() {
        let mut log = Activity::default();
        let out = run_with_stdin(ECHO, secret(), &mut log);
        assert_eq!(out.code, Some(3));
        assert!(
            out.tail.contains(&format!("len={}", KEY.len())),
            "{:?}",
            out.tail
        );
    }

    #[cfg(unix)]
    #[test]
    fn the_tail_replaces_the_key_with_stars_on_both_streams() {
        let mut log = Activity::default();
        let out = run_with_stdin(ECHO, secret(), &mut log);
        assert!(out.tail.contains(&"out=***".to_string()), "{:?}", out.tail);
        assert!(out.tail.contains(&"err=***".to_string()), "{:?}", out.tail);
        assert!(out.tail.iter().all(|l| !l.contains(KEY)));
    }

    #[cfg(unix)]
    #[test]
    fn no_activity_entry_contains_the_key() {
        let mut log = Activity::default();
        run_with_stdin(ECHO, secret(), &mut log);
        let e = &log.entries()[0];
        assert_eq!(e.argv[0], "sh");
        assert_eq!(e.code, Some(3));
        assert!(!format!("{:?}", log.entries()).contains(KEY));
    }

    #[test]
    fn a_command_that_cannot_start_is_reported_and_logged() {
        const MISSING: &[&str] = &["igris-no-such-tool-for-tests"];
        let mut log = Activity::default();
        let out = run_with_stdin(MISSING, secret(), &mut log);
        assert_eq!(out.code, None);
        assert!(out.tail[0].starts_with("could not start igris-no-such-tool-for-tests"));
        assert_eq!(log.entries().len(), 1);
        assert_eq!(log.entries()[0].code, None);
    }
}
