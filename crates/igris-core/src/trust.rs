//! The folder-trust dialogs IGRIS knows how to answer for the user.
//!
//! Each entry records the agent version the dialog was checked on, the keys that accept it, and
//! the phrases that must all be in the last lines of the screen, with the dialog's footer as its
//! last line. Anything else (an update prompt, a command permission,
//! a dialog from another version or another agent) is not matched, and IGRIS presses nothing.
//! Matching reads the visible screen, whose lines may be wrapped, so the phrases are short.

pub struct Dialog {
    /// The agent kind Herdr reports for the pane.
    pub agent: &'static str,
    /// The name shown to the user.
    pub label: &'static str,
    /// The agent version the dialog was captured on.
    pub checked_on: &'static str,
    /// The keys that accept it, one `herdr agent send-keys` call each.
    pub keys: &'static [&'static str],
    /// Phrases that must all appear in the last few lines of the screen.
    pub must_contain: &'static [&'static str],
    /// The hint line a dialog ends with. It must be the screen's last line and appear once: a
    /// dialog quoted above another question, or two dialogs, are not "the" dialog.
    pub footer: &'static str,
}

pub const DIALOGS: &[Dialog] = &[
    Dialog {
        agent: "claude",
        label: "Claude Code",
        checked_on: "2.1.332",
        // "No, exit" is highlighted at first: down moves to "Yes, I trust this folder".
        keys: &["down", "enter"],
        must_contain: &["Yes, I trust this folder"],
        footer: "Enter to confirm",
    },
    Dialog {
        agent: "codex",
        label: "Codex",
        checked_on: "0.171.0",
        // "1. Trust and continue" is highlighted at first.
        keys: &["enter"],
        must_contain: &["Trust this folder?", "1. Trust and continue"],
        footer: "enter continue",
    },
];

/// A screen that shows one of these is an update prompt, whose default row runs an installer:
/// never matched, even when a trust dialog is also in the text.
const NEVER_WITH: &[&str] = &["Update available", "Update now"];

/// How many of the screen's last non-empty lines a dialog may span.
const WINDOW: usize = 12;

/// The recorded dialog that `screen` shows for `agent` as its active question, if any.
pub fn match_dialog(agent: &str, screen: &str) -> Option<&'static Dialog> {
    if NEVER_WITH.iter().any(|p| screen.contains(p)) {
        return None;
    }
    let lines: Vec<&str> = screen
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let last = lines.last()?;
    let tail = &lines[lines.len().saturating_sub(WINDOW)..];
    DIALOGS.iter().find(|d| {
        d.agent == agent
            && last.contains(d.footer)
            && screen.matches(d.footer).count() == 1
            && d.must_contain
                .iter()
                .all(|p| tail.iter().any(|l| l.contains(p)))
    })
}

/// Codex says so when, in a git worktree, trusting applies to the whole repository.
pub fn names_repo_root(screen: &str) -> bool {
    screen.contains("repository root")
}
