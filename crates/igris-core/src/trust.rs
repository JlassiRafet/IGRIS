//! The folder-trust dialogs IGRIS knows how to answer for the user.
//!
//! Each entry records the agent version the dialog was checked on, the keys that accept it, and
//! the phrases that must all be on screen. Anything else (an update prompt, a command permission,
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
    /// Phrases that must all appear on the screen.
    pub must_contain: &'static [&'static str],
}

pub const DIALOGS: &[Dialog] = &[
    Dialog {
        agent: "claude",
        label: "Claude Code",
        checked_on: "2.1.332",
        // "No, exit" is highlighted at first: down moves to "Yes, I trust this folder".
        keys: &["down", "enter"],
        must_contain: &["Yes, I trust this folder", "Enter to confirm"],
    },
    Dialog {
        agent: "codex",
        label: "Codex",
        checked_on: "0.171.0",
        // "1. Trust and continue" is highlighted at first.
        keys: &["enter"],
        must_contain: &["Trust this folder?", "1. Trust and continue"],
    },
];

/// A screen that shows one of these is an update prompt, whose default row runs an installer:
/// never matched, even when a trust dialog is also in the text.
const NEVER_WITH: &[&str] = &["Update available", "Update now"];

/// The recorded dialog that `screen` shows for `agent`, if any.
pub fn match_dialog(agent: &str, screen: &str) -> Option<&'static Dialog> {
    if NEVER_WITH.iter().any(|p| screen.contains(p)) {
        return None;
    }
    DIALOGS
        .iter()
        .find(|d| d.agent == agent && d.must_contain.iter().all(|p| screen.contains(p)))
}

/// Codex says so when, in a git worktree, trusting applies to the whole repository.
pub fn names_repo_root(screen: &str) -> bool {
    screen.contains("repository root")
}
