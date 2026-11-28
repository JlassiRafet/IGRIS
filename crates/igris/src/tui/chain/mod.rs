//! The guided chain (spec 2026-11-26-guided-chain-setup): one question at a time, answered ones
//! above as history. Slice 2: Workspace, Builder and Reviewer, then install, login and a live check
//! for an agent that is not ready. It is shown only
//! with `IGRIS_CHAIN=1`; the dashboard stays the default until every step is ported.
pub mod draw;
pub mod model;

use super::action::Action;
use super::connect::start_check;
use super::install::{Offer, install_offer};
use super::state::{Effect, State};
use super::tools_screen::{CheckState, Install};
use crate::catalog::tool;
use igris_detect::Snapshot;
use model::{ChainUi, Want, Work, WorkMap};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::time::Instant;

/// The chain is on only for `IGRIS_CHAIN=1`.
pub fn enabled(var: Option<&str>) -> bool {
    var == Some("1")
}

/// Do what a chain choice asked. Only fixed commands from the catalog run: an install is the
/// tool's verified `install` argv (never for a tool without one), a check its `check` argv, a login
/// the method's own command on the terminal.
pub fn run(s: &mut State, w: Want) -> Effect {
    match w {
        Want::Install(id) => {
            if tool(id).and_then(|t| t.install).is_none()
                || matches!(
                    s.tools.installs.get(id),
                    Some(Install::Running(_) | Install::Done(_))
                )
            {
                return Effect::None;
            }
            s.tools
                .installs
                .insert(id, Install::Running(Instant::now()));
            sync(s);
            Effect::Install(id)
        }
        Want::Copy(id) => match tool(id).map(install_offer) {
            Some(Offer::Copy(cmd)) => {
                s.toast = Some("Copied. Run it yourself, then press r.".into());
                Effect::Copy(cmd)
            }
            _ => Effect::None,
        },
        Want::Check(id) => {
            let effect = start_check(s, id);
            sync(s);
            effect
        }
        Want::Attach(argv) => {
            // The login may fix what the last check found: the next scan decides, not the old result.
            if let Some(k) = s.chain.as_ref().and_then(|c| c.subject.clone()) {
                s.tools.checks.remove(k.as_str());
            }
            Effect::RunAttached(argv)
        }
    }
}

/// What the install and check engines are doing, per agent, for the chain to draw.
pub fn work_of(s: &State) -> WorkMap {
    let mut m = WorkMap::new();
    for (id, c) in &s.tools.checks {
        let w = match c {
            CheckState::Running => Work::Checking {
                secs: s
                    .tools
                    .started
                    .get(id.as_str())
                    .map_or(0, |t| t.elapsed().as_secs()),
            },
            CheckState::Passed { ms } => Work::Passed { ms: *ms },
            CheckState::Failed(f) => Work::CheckFailed {
                reason: f.reason.clone(),
                fix: f.fix.clone(),
            },
            CheckState::Pending | CheckState::Skipped => continue,
        };
        m.insert(id.clone(), w);
    }
    for (id, i) in &s.tools.installs {
        let w = match i {
            Install::Running(since) => Work::Installing {
                secs: since.elapsed().as_secs(),
            },
            Install::Done(_) => Work::InstallDone,
            Install::Failed(f) => Work::InstallFailed {
                reason: f.reason.clone(),
                fix: f.fix.clone(),
            },
        };
        m.insert((*id).to_string(), w);
    }
    m
}

/// Hand the chain what the engines are doing now; when that changed, re-route its questions.
pub fn sync(s: &mut State) {
    let work = work_of(s);
    let (Some(c), Some(snap)) = (s.chain.as_mut(), s.snapshot.as_ref()) else {
        return;
    };
    if c.work != work {
        c.work = work;
        c.resettle(snap, snap);
    }
}

/// A key in the chain. `snap` is `None` during the first scan: only quit works then. The answer is
/// an `Action` only for what the app loop already handles (quit, re-scan); everything else is
/// handled here.
pub fn key(ui: &mut ChainUi, snap: Option<&Snapshot>, k: KeyEvent) -> Option<Action> {
    let ctrl_c = k.code == KeyCode::Char('c') && k.modifiers.contains(KeyModifiers::CONTROL);
    if ctrl_c || k.code == KeyCode::Char('q') {
        return Some(Action::Quit);
    }
    let snap = snap?;
    let n = ui.choices(snap).len();
    match k.code {
        KeyCode::Up if n > 0 => ui.cursor = (ui.cursor + n - 1) % n,
        KeyCode::Down if n > 0 => ui.cursor = (ui.cursor + 1) % n,
        KeyCode::Enter => return ui.enter(snap).map(Action::Chain),
        KeyCode::Left | KeyCode::Esc => ui.back(snap),
        KeyCode::Char('r') => return Some(Action::Rescan),
        _ => {}
    }
    None
}

#[cfg(test)]
mod tests {
    use super::model::Step;
    use super::*;
    use crate::tui::detect_worker::Msg;
    use crate::tui::state::State;
    use crate::tui::tools_screen::tests::{app, snap};
    use igris_detect::Login::LoggedIn;

    fn k(c: KeyCode) -> KeyEvent {
        KeyEvent::new(c, KeyModifiers::NONE)
    }

    fn ready() -> Snapshot {
        snap(&[("claude", true, LoggedIn), ("codex", true, LoggedIn)])
    }

    fn started(s: &Snapshot) -> ChainUi {
        let mut u = ChainUi::new();
        u.settle(s);
        u
    }

    #[test]
    fn up_down_wrap_and_enter_advances() {
        let s = ready();
        let mut u = started(&s);
        let n = u.choices(&s).len();
        assert_eq!(key(&mut u, Some(&s), k(KeyCode::Up)), None);
        assert_eq!(u.cursor, n - 1);
        key(&mut u, Some(&s), k(KeyCode::Down));
        assert_eq!(u.cursor, 0);
        assert_eq!(key(&mut u, Some(&s), k(KeyCode::Enter)), None);
        assert_eq!(u.step, Step::Reviewer);
    }

    #[test]
    fn left_goes_back() {
        let s = ready();
        let mut u = started(&s);
        key(&mut u, Some(&s), k(KeyCode::Enter));
        assert_eq!(u.step, Step::Reviewer);
        key(&mut u, Some(&s), k(KeyCode::Left));
        assert_eq!(u.step, Step::Builder);
    }

    #[test]
    fn esc_goes_back_like_left_and_never_quits() {
        let s = ready();
        let mut u = started(&s);
        key(&mut u, Some(&s), k(KeyCode::Enter));
        assert_eq!(u.step, Step::Reviewer);
        assert_eq!(key(&mut u, Some(&s), k(KeyCode::Esc)), None);
        assert_eq!(u.step, Step::Builder);
        assert_eq!(
            key(&mut u, Some(&s), k(KeyCode::Esc)),
            None,
            "on the first question Esc stays put"
        );
        assert_eq!(u.step, Step::Builder);
    }

    #[test]
    fn r_asks_for_a_rescan() {
        let s = ready();
        let mut u = started(&s);
        assert_eq!(
            key(&mut u, Some(&s), k(KeyCode::Char('r'))),
            Some(Action::Rescan)
        );
    }

    #[test]
    fn q_and_ctrl_c_quit() {
        let s = ready();
        let mut u = started(&s);
        assert_eq!(
            key(&mut u, Some(&s), k(KeyCode::Char('q'))),
            Some(Action::Quit)
        );
        let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(key(&mut u, Some(&s), ctrl_c), Some(Action::Quit));
    }

    #[test]
    fn during_the_first_scan_only_quit_works() {
        let mut u = ChainUi::new();
        assert_eq!(key(&mut u, None, k(KeyCode::Enter)), None);
        assert_eq!(key(&mut u, None, k(KeyCode::Char('r'))), None);
        assert_eq!(u, ChainUi::new());
        assert_eq!(key(&mut u, None, k(KeyCode::Char('q'))), Some(Action::Quit));
    }

    #[test]
    fn the_flag_is_on_only_for_1_and_the_default_state_has_no_chain() {
        assert!(enabled(Some("1")));
        for off in [None, Some(""), Some("0"), Some("true")] {
            assert!(!enabled(off), "{off:?}");
        }
        assert!(State::new((100, 30)).chain.is_none());
    }

    use crate::catalog::TOOLS;
    use crate::job::{JobEnd, JobId};
    use crate::tui::state::update;
    use igris_detect::Login::LoggedOut;
    use model::Work;

    fn job_of(id: &str) -> JobId {
        JobId(TOOLS.iter().position(|t| t.id == id).unwrap() as u32)
    }

    fn ended(code: Option<i32>, lines: &[&str]) -> JobEnd {
        JobEnd {
            code,
            tail: lines.iter().map(|l| (*l).to_string()).collect(),
            timed_out: false,
            started: true,
        }
    }

    /// The chain with claude installed but logged out and codex ready, both answered.
    fn connecting() -> State {
        let s = snap(&[("claude", true, LoggedOut), ("codex", true, LoggedIn)]);
        let mut st = app(s.clone(), None);
        let mut u = ChainUi::new();
        u.builder = Some("claude".into());
        u.reviewer = Some("codex".into());
        u.step = Step::End;
        st.chain = Some(u);
        st.apply(Msg::Detected(Box::new(s)));
        assert_eq!(st.chain.as_ref().unwrap().step, Step::Connect);
        st
    }

    #[test]
    fn choosing_install_starts_one_install_and_a_second_choice_does_not_start_another() {
        let mut s = ready();
        s.herdr.found = false;
        let mut st = app(s.clone(), None);
        st.chain = Some(ChainUi::new());
        st.apply(Msg::Detected(Box::new(s)));
        assert_eq!(
            update(&mut st, Action::Chain(Want::Install("herdr"))),
            Effect::Install("herdr")
        );
        assert!(matches!(
            st.tools.installs.get("herdr"),
            Some(Install::Running(_))
        ));
        assert_eq!(
            update(&mut st, Action::Chain(Want::Install("herdr"))),
            Effect::None
        );
        assert!(matches!(
            work_of(&st).get("herdr"),
            Some(Work::Installing { .. })
        ));
    }

    #[test]
    fn nothing_is_run_for_a_tool_without_a_verified_install_command() {
        let mut st = connecting();
        assert_eq!(
            update(&mut st, Action::Chain(Want::Install("claude"))),
            Effect::None
        );
        assert!(st.tools.installs.is_empty());
        assert_eq!(
            update(&mut st, Action::Chain(Want::Copy("herdr"))),
            Effect::None,
            "a command that can run is never only copied"
        );
    }

    #[test]
    fn a_check_result_reaches_the_chain_and_passes_the_step() {
        let mut st = connecting();
        assert_eq!(
            update(&mut st, Action::Chain(Want::Check("claude"))),
            Effect::Check("claude")
        );
        assert!(matches!(
            work_of(&st).get("claude"),
            Some(Work::Checking { .. })
        ));
        st.apply(Msg::Job(job_of("claude"), ended(Some(0), &["ok"])));
        let u = st.chain.as_ref().unwrap();
        assert_eq!(u.step, Step::End, "{:?}", u.work);
        assert!(
            u.history(st.snapshot.as_ref().unwrap())
                .iter()
                .any(|h| h.2.starts_with("replied in"))
        );
    }

    #[test]
    fn a_failed_check_keeps_the_chain_on_connect_with_the_reason_and_the_choice_unchanged() {
        let mut st = connecting();
        update(&mut st, Action::Chain(Want::Check("claude")));
        st.apply(Msg::Job(
            job_of("claude"),
            ended(Some(1), &["Not logged in"]),
        ));
        let u = st.chain.as_ref().unwrap();
        assert_eq!(
            (u.step, u.builder.as_deref()),
            (Step::Connect, Some("claude"))
        );
        assert!(
            matches!(u.work.get("claude"), Some(Work::CheckFailed { .. })),
            "{:?}",
            u.work
        );
    }

    #[test]
    fn log_in_clears_the_old_result_and_asks_the_loop_to_run_the_login_on_the_terminal() {
        let mut st = connecting();
        update(&mut st, Action::Chain(Want::Check("claude")));
        st.apply(Msg::Job(
            job_of("claude"),
            ended(Some(1), &["Not logged in"]),
        ));
        let argv: &'static [&'static str] = &["claude", "auth", "login"];
        assert_eq!(
            update(&mut st, Action::Chain(Want::Attach(argv))),
            Effect::RunAttached(argv)
        );
        assert!(
            !st.tools.checks.contains_key("claude"),
            "the next scan decides"
        );
    }

    #[test]
    fn a_rescan_during_an_install_keeps_the_question() {
        let s = snap(&[("claude", false, LoggedIn), ("codex", true, LoggedIn)]);
        let mut st = app(s.clone(), None);
        let mut u = ChainUi::new();
        u.builder = Some("claude".into());
        u.reviewer = Some("codex".into());
        u.step = Step::End;
        st.chain = Some(u);
        st.tools
            .installs
            .insert("claude", Install::Running(std::time::Instant::now()));
        st.apply(Msg::Detected(Box::new(s.clone())));
        st.apply(Msg::Detected(Box::new(s)));
        let u = st.chain.as_ref().unwrap();
        assert_eq!(
            (u.step, u.subject.as_deref(), u.builder.as_deref()),
            (Step::Connect, Some("claude"), Some("claude"))
        );
    }

    #[test]
    fn a_scan_settles_the_chain() {
        let mut st = app(ready(), None);
        st.chain = Some(ChainUi::new());
        st.apply(Msg::Detected(Box::new(ready())));
        assert_eq!(st.chain.as_ref().unwrap().step, Step::Builder);
    }

    #[test]
    fn a_second_scan_keeps_the_cursor_where_the_user_moved_it() {
        let mut st = app(ready(), None);
        st.chain = Some(ChainUi::new());
        st.apply(Msg::Detected(Box::new(ready())));
        let s = st.snapshot.clone().unwrap();
        key(st.chain.as_mut().unwrap(), Some(&s), k(KeyCode::Down));
        let moved = st.chain.as_ref().unwrap().cursor;
        assert_ne!(moved, 0, "the cursor moved off the default");
        st.apply(Msg::Detected(Box::new(ready())));
        assert_eq!(st.chain.as_ref().unwrap().cursor, moved);
    }

    #[test]
    fn ten_keypresses_or_fewer_reach_the_end_on_a_ready_machine() {
        let s = ready();
        let mut u = started(&s);
        let mut keys = 0;
        while u.step != Step::End && keys < 10 {
            key(&mut u, Some(&s), k(KeyCode::Enter));
            keys += 1;
        }
        assert_eq!(u.step, Step::End);
        assert_eq!(keys, 2, "B1: one Enter per question");
    }
}
