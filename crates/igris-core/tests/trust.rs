use igris_core::trust::{match_dialog, names_repo_root};

const CLAUDE: &str = include_str!("fixtures/herdr/agent-read-blocked.txt");
const CODEX_ROOT: &str = include_str!("fixtures/herdr/codex-trust-root.txt");
const CODEX_UPDATE: &str = include_str!("fixtures/herdr/codex-update.txt");

#[test]
fn claudes_folder_trust_dialog_matches_and_needs_down_then_enter() {
    let d = match_dialog("claude", CLAUDE).expect("a recorded dialog");
    assert_eq!(d.keys, ["down", "enter"]);
    assert_eq!(d.checked_on, "2.1.332");
    assert!(!names_repo_root(CLAUDE));
}

#[test]
fn codexs_worktree_dialog_matches_with_enter_and_names_the_repository_root() {
    let d = match_dialog("codex", CODEX_ROOT).expect("a recorded dialog");
    assert_eq!(d.keys, ["enter"]);
    assert_eq!(d.checked_on, "0.171.0");
    assert!(names_repo_root(CODEX_ROOT));
}

#[test]
fn codexs_plain_dialog_without_the_repository_note_matches_too() {
    let plain = "  Trust this folder? Codex can read, edit, and run files here, subject to your\n\n› 1. Trust and continue\n  2. Back to Agent Command Center\n\n  enter continue · esc back\n";
    assert!(match_dialog("codex", plain).is_some());
    assert!(!names_repo_root(plain));
}

#[test]
fn an_update_prompt_never_matches_even_with_a_trust_dialog_in_the_text() {
    assert!(match_dialog("codex", CODEX_UPDATE).is_none());
    let both = format!("{CODEX_UPDATE}\n{CODEX_ROOT}");
    assert!(match_dialog("codex", &both).is_none());
}

#[test]
fn an_unknown_screen_or_agent_never_matches() {
    assert!(
        match_dialog(
            "claude",
            "Do you want to run `rm -rf build`?\n❯ 1. Yes\n  2. No"
        )
        .is_none()
    );
    assert!(match_dialog("gemini", CLAUDE).is_none());
    assert!(
        match_dialog("claude", CODEX_ROOT).is_none(),
        "another agent's dialog"
    );
    assert!(
        match_dialog("codex", CLAUDE).is_none(),
        "another agent's dialog"
    );
}

const ANOTHER_QUESTION: &str = "Do you want to run `rm -rf build`?\n❯ 1. Yes\n  2. No\n";

#[test]
fn a_dialog_quoted_above_another_question_does_not_match() {
    // The trust text is still in the visible text, but the active question is another one.
    assert!(match_dialog("codex", &format!("{CODEX_ROOT}\n{ANOTHER_QUESTION}")).is_none());
    assert!(match_dialog("claude", &format!("{CLAUDE}\n{ANOTHER_QUESTION}")).is_none());
}

#[test]
fn two_dialogs_with_the_same_footer_are_ambiguous_and_do_not_match() {
    let second =
        "Do you want to proceed?\n❯ 1. Yes\n  2. No\n\n Enter to confirm · Esc to cancel\n";
    assert!(match_dialog("claude", &format!("{CLAUDE}\n{second}")).is_none());
}
