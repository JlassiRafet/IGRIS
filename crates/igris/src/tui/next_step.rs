//! The "Next:" line: one sentence saying what to do now, a pure function of the state.
use super::state::State;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    FixBlocker(String),
    SetCheckCommand,
    ReviewAndApply,
    /// A check on the Test screen fails: its id and its own fix text.
    FixCheck(String),
    RunTest,
    Done,
}

pub fn next_step(s: &State) -> Step {
    if let Some(b) = &s.blocker {
        Step::FixBlocker(b.clone())
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
        Step::FixCheck(format!("fix the {} check (key 4): {}", c.id, c.fix))
    } else if !s.tested {
        Step::RunTest
    } else {
        Step::Done
    }
}

pub fn text(step: &Step) -> String {
    match step {
        Step::FixBlocker(what) => format!("Next: {what}"),
        Step::SetCheckCommand => "Next: set the check command (key 2, Pipeline)".into(),
        Step::ReviewAndApply => "Next: review and apply the change (key 3, Review)".into(),
        Step::FixCheck(what) => format!("Next: {what}"),
        Step::RunTest => "Next: run the test (key 4, Test)".into(),
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
            Step::FixCheck("fix the clean check (key 4): commit or stash them".into())
        );
        assert_eq!(
            text(&step),
            "Next: fix the clean check (key 4): commit or stash them"
        );
        s.test.checks.as_mut().unwrap()[1].ok = true;
        assert_eq!(next_step(&s), Step::Done);
    }

    #[test]
    fn each_step_reads_as_a_sentence() {
        assert_eq!(text(&Step::FixBlocker("log in".into())), "Next: log in");
        assert!(text(&Step::Done).starts_with("Setup is done."));
        assert!(text(&Step::RunTest).contains("key 4"));
    }
}
