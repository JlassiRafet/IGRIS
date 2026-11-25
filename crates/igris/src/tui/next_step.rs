//! The "Next:" line: one sentence saying what to do now, a pure function of the state.
use super::state::State;
use crate::catalog::Group;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    FixBlocker(String),
    /// A group of the Tools screen is not settled: the first one, in the order the step walks them.
    ChooseTools(Group),
    /// A configured install, opened again: check the tools before anything else.
    Recheck,
    /// The chosen tools have not all passed their live check.
    VerifyTools,
    SetCheckCommand,
    ReviewAndApply,
    /// A check on the Test screen fails: its id and its own fix text.
    FixCheck(String),
    RunTest,
    Done,
}

/// Before the first scan there is nothing to settle, so the line does not nag about it.
fn unsettled(s: &State) -> Option<Group> {
    match (&s.snapshot, &s.pipeline.form) {
        (Some(snap), Some(form)) => super::tools_screen::first_unsettled(&s.tools, form, snap),
        _ => None,
    }
}

pub fn next_step(s: &State) -> Step {
    if let Some(b) = &s.blocker {
        Step::FixBlocker(b.clone())
    } else if s.snapshot.is_some() && s.applied && !s.dirty && s.tools.checks.is_empty() {
        // Run again: the file is there and nothing was checked in this session.
        Step::Recheck
    } else if let Some(group) = unsettled(s) {
        Step::ChooseTools(group)
    } else if !s.command_set {
        Step::SetCheckCommand
    } else if !super::verify_screen::verify_ready(s) {
        Step::VerifyTools
    } else if s.dirty || !s.applied {
        Step::ReviewAndApply
    } else if let Some(c) = s
        .test
        .checks
        .as_ref()
        .and_then(|c| c.iter().find(|c| !c.ok))
    {
        Step::FixCheck(format!("fix the {} check (key 6): {}", c.id, c.fix))
    } else if !s.tested {
        Step::RunTest
    } else {
        Step::Done
    }
}

fn capital(s: &str) -> String {
    let mut c = s.chars();
    c.next().map_or(String::new(), |f| {
        f.to_uppercase().collect::<String>() + c.as_str()
    })
}

/// The sentence after the "Next" marker; for `Done` the whole closing line.
pub fn sentence(step: &Step) -> String {
    match step {
        Step::FixBlocker(what) | Step::FixCheck(what) => capital(what),
        Step::ChooseTools(group) => {
            let what = match group {
                Group::Workspace => "workspace manager",
                Group::Builder => "builder agent",
                Group::Reviewer => "reviewer agent",
                Group::Decision => "decision layer",
                Group::Assistant => "assistant",
            };
            format!("Choose your {what} (key 2, Tools)")
        }
        Step::Recheck => "Re-check your tools (key 4, Verify)".into(),
        Step::VerifyTools => "Verify your tools (key 4, Verify)".into(),
        Step::SetCheckCommand => "Set the check command (key 3, Pipeline)".into(),
        Step::ReviewAndApply => "Review and apply the change (key 5, Review)".into(),
        Step::RunTest => "Run the test (key 6, Test)".into(),
        Step::Done => "Setup is done. Run a task: igris run --task \"...\"".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_unfinished_thing_wins() {
        let mut s = State::new((100, 30));
        s.blocker = Some("log in to Claude Code (Enter on the row)".into());
        assert_eq!(
            next_step(&s),
            Step::FixBlocker("log in to Claude Code (Enter on the row)".into())
        );
        s.blocker = None;
        assert_eq!(next_step(&s), Step::SetCheckCommand);
        s.command_set = true;
        assert_eq!(next_step(&s), Step::ReviewAndApply, "nothing written yet");
        s.applied = true;
        s.dirty = true;
        assert_eq!(next_step(&s), Step::ReviewAndApply, "unsaved changes");
        s.dirty = false;
        assert_eq!(next_step(&s), Step::RunTest);
        s.tested = true;
        assert_eq!(next_step(&s), Step::Done);
    }

    #[test]
    fn unsettled_tools_come_before_the_check_command() {
        use crate::tui::detect_worker::Msg;
        let mut s = State::new((100, 30));
        s.repo = std::env::temp_dir().join("igris-next-none");
        s.apply(Msg::Detected(Box::new(crate::check::view::tests::real())));
        assert_eq!(
            next_step(&s),
            Step::ChooseTools(Group::Assistant),
            "the assistant is unanswered"
        );
        assert_eq!(
            sentence(&Step::ChooseTools(Group::Assistant)),
            "Choose your assistant (key 2, Tools)"
        );
        s.tools.assistant = crate::tui::tools_screen::AssistantChoice::CliOnly;
        assert_eq!(next_step(&s), Step::SetCheckCommand);
    }

    #[test]
    fn a_failing_check_blocks_done_and_says_how_to_fix_it() {
        use crate::doctor::Check;
        let mut s = State::new((100, 30));
        (s.command_set, s.applied, s.tested) = (true, true, true);
        s.test.checks = Some(vec![
            Check {
                id: "config",
                ok: true,
                detail: String::new(),
                fix: String::new(),
            },
            Check {
                id: "clean",
                ok: false,
                detail: "dirty".into(),
                fix: "commit or stash them".into(),
            },
        ]);
        let step = next_step(&s);
        assert_eq!(
            step,
            Step::FixCheck("fix the clean check (key 6): commit or stash them".into())
        );
        assert_eq!(
            sentence(&step),
            "Fix the clean check (key 6): commit or stash them"
        );
        s.test.checks.as_mut().unwrap()[1].ok = true;
        assert_eq!(next_step(&s), Step::Done);
    }

    #[test]
    fn each_step_reads_as_a_sentence() {
        assert_eq!(sentence(&Step::FixBlocker("log in".into())), "Log in");
        assert!(sentence(&Step::Done).starts_with("Setup is done."));
        assert!(sentence(&Step::RunTest).contains("key 6"));
        assert_eq!(
            sentence(&Step::SetCheckCommand),
            "Set the check command (key 3, Pipeline)"
        );
    }

    #[test]
    fn next_step_points_at_the_first_unfinished_group() {
        use crate::tui::tools_screen::tests::{app, both, snap};
        use igris_detect::Login;
        // No Herdr: the workspace comes first, whatever else is open.
        let mut no_herdr = both();
        no_herdr.herdr.found = false;
        let mut s = app(no_herdr, None);
        assert!(
            matches!(next_step(&s), Step::FixBlocker(_)),
            "a blocker still comes first"
        );
        s.blocker = None;
        assert_eq!(next_step(&s), Step::ChooseTools(Group::Workspace));
        assert_eq!(
            sentence(&next_step(&s)),
            "Choose your workspace manager (key 2, Tools)"
        );
        // Herdr, no agents on the machine: the builder.
        let mut s = app(snap(&[]), None);
        s.blocker = None;
        assert_eq!(next_step(&s), Step::ChooseTools(Group::Builder));
        // Everything but the assistant answered.
        let mut s = app(both(), None);
        s.blocker = None;
        assert_eq!(next_step(&s), Step::ChooseTools(Group::Assistant));
        s.tools.assistant = crate::tui::tools_screen::AssistantChoice::CliOnly;
        s.command_set = true;
        assert_eq!(
            next_step(&s),
            Step::VerifyTools,
            "the tools are chosen but not checked"
        );
        assert_eq!(
            sentence(&Step::VerifyTools),
            "Verify your tools (key 4, Verify)"
        );
        crate::tui::verify_screen::clear_all(&mut s);
        assert_eq!(next_step(&s), Step::ReviewAndApply);
        let _ = Login::LoggedIn;
    }

    #[test]
    fn on_a_configured_install_the_first_action_is_recheck_and_finished_steps_are_collapsed() {
        use crate::tui::rail::{StepState, step_state};
        use crate::tui::state::Screen;
        use crate::tui::tools_screen::tests::{app, both};
        let mut s = app(both(), None);
        s.blocker = None;
        s.applied = true; // an igris.toml that matches the form is on disk
        s.command_set = true;
        s.pipeline.errors.clear();
        assert_eq!(
            next_step(&s),
            Step::Recheck,
            "before the unanswered assistant"
        );
        assert_eq!(
            sentence(&Step::Recheck),
            "Re-check your tools (key 4, Verify)"
        );
        // The finished steps show as done on the rail; Verify is the one still to do.
        assert_eq!(step_state(&s, Screen::Pipeline), StepState::Done);
        assert_eq!(step_state(&s, Screen::Verify), StepState::Todo);
        // Once something was checked in this session the line moves on.
        s.tools.checks.insert(
            "herdr".into(),
            crate::tui::tools_screen::CheckState::Passed { ms: 1 },
        );
        assert_eq!(next_step(&s), Step::ChooseTools(Group::Assistant));
    }
}
