//! The "Next:" line: one sentence saying what to do now, a pure function of the state.
use super::state::State;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    FixBlocker(String),
    /// Herdr, the agents or the assistant are not settled on the Tools screen.
    ChooseTools,
    SetCheckCommand,
    ReviewAndApply,
    /// A check on the Test screen fails: its id and its own fix text.
    FixCheck(String),
    RunTest,
    Done,
}

/// Before the first scan there is nothing to settle, so the line does not nag about it.
fn tools_settled(s: &State) -> bool {
    match (&s.snapshot, &s.pipeline.form) {
        (Some(snap), Some(form)) => super::tools_screen::tools_done(&s.tools, form, snap),
        _ => true,
    }
}

pub fn next_step(s: &State) -> Step {
    if let Some(b) = &s.blocker {
        Step::FixBlocker(b.clone())
    } else if !tools_settled(s) {
        Step::ChooseTools
    } else if !s.command_set {
        Step::SetCheckCommand
    } else if s.dirty || !s.applied {
        Step::ReviewAndApply
    } else if let Some(c) = s
        .test
        .checks
        .as_ref()
        .and_then(|c| c.iter().find(|c| !c.ok))
    {
        Step::FixCheck(format!("fix the {} check (key 5): {}", c.id, c.fix))
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
        Step::ChooseTools => "Choose your tools (key 2, Tools)".into(),
        Step::SetCheckCommand => "Set the check command (key 3, Pipeline)".into(),
        Step::ReviewAndApply => "Review and apply the change (key 4, Review)".into(),
        Step::RunTest => "Run the test (key 5, Test)".into(),
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
            Step::ChooseTools,
            "the assistant is unanswered"
        );
        assert_eq!(
            sentence(&Step::ChooseTools),
            "Choose your tools (key 2, Tools)"
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
            Step::FixCheck("fix the clean check (key 5): commit or stash them".into())
        );
        assert_eq!(
            sentence(&step),
            "Fix the clean check (key 5): commit or stash them"
        );
        s.test.checks.as_mut().unwrap()[1].ok = true;
        assert_eq!(next_step(&s), Step::Done);
    }

    #[test]
    fn each_step_reads_as_a_sentence() {
        assert_eq!(sentence(&Step::FixBlocker("log in".into())), "Log in");
        assert!(sentence(&Step::Done).starts_with("Setup is done."));
        assert!(sentence(&Step::RunTest).contains("key 5"));
        assert_eq!(
            sentence(&Step::SetCheckCommand),
            "Set the check command (key 3, Pipeline)"
        );
    }
}
