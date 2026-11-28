//! The guided chain (spec 2026-11-26-guided-chain-setup): one question at a time, answered ones
//! above as history. Slice 1: Workspace as one line, then Builder and Reviewer. It is shown only
//! with `IGRIS_CHAIN=1`; the dashboard stays the default until every step is ported.
pub mod draw;
pub mod model;

use super::action::Action;
use igris_detect::Snapshot;
use model::ChainUi;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// The chain is on only for `IGRIS_CHAIN=1`.
pub fn enabled(var: Option<&str>) -> bool {
    var == Some("1")
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
        KeyCode::Enter => {
            ui.enter(snap);
        }
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
