//! The whole app state, and the one function that changes it.
use super::action::Action;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Check,
    Pipeline,
    Review,
    Test,
}

impl Screen {
    pub const ALL: [Screen; 4] = [
        Screen::Check,
        Screen::Pipeline,
        Screen::Review,
        Screen::Test,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Screen::Check => "Check",
            Screen::Pipeline => "Pipeline",
            Screen::Review => "Review",
            Screen::Test => "Test",
        }
    }

    fn step(self, by: usize) -> Screen {
        let i = Screen::ALL.iter().position(|s| *s == self).unwrap_or(0);
        Screen::ALL[(i + by) % Screen::ALL.len()]
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Confirm {
    Discard,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Overlay {
    Help,
    Palette,
    Confirm(Confirm),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct State {
    pub screen: Screen,
    pub overlay: Option<Overlay>,
    /// Unsaved changes: quitting asks first.
    pub dirty: bool,
    pub detecting: bool,
    pub size: (u16, u16),
    /// What blocks running tasks, in words, from the Check screen (None when nothing does).
    pub blocker: Option<String>,
    /// `checks.command` is set in the form.
    pub command_set: bool,
    /// An `igris.toml` that matches the form exists on disk.
    pub applied: bool,
    pub tested: bool,
    /// What is typed in the help filter or the palette.
    pub query: String,
    /// The palette's chosen row, or the help's scroll offset.
    pub cursor: usize,
}

impl State {
    pub fn new(size: (u16, u16)) -> State {
        State {
            screen: Screen::Check,
            overlay: None,
            dirty: false,
            detecting: false,
            size,
            blocker: None,
            command_set: false,
            applied: false,
            tested: false,
            query: String::new(),
            cursor: 0,
        }
    }
}

/// What the loop must do after an update.
#[derive(Debug, PartialEq, Eq)]
pub enum Effect {
    None,
    Quit,
    Rescan,
}

pub fn update(s: &mut State, a: Action) -> Effect {
    let discard = Some(Overlay::Confirm(Confirm::Discard));
    match a {
        Action::Quit if s.overlay == discard => Effect::Quit,
        Action::Quit if s.dirty => {
            s.overlay = discard;
            Effect::None
        }
        Action::Quit => Effect::Quit,
        Action::Yes if s.overlay == discard => Effect::Quit,
        Action::No | Action::Cancel if s.overlay.is_some() => {
            s.overlay = None;
            Effect::None
        }
        // An open overlay is modal: only the answers above and Quit get through.
        _ if s.overlay.is_some() => Effect::None,
        Action::Help => open(s, Overlay::Help),
        Action::Palette => open(s, Overlay::Palette),
        Action::Goto(screen) => {
            s.screen = screen;
            Effect::None
        }
        Action::NextScreen => {
            s.screen = s.screen.step(1);
            Effect::None
        }
        Action::PrevScreen => {
            s.screen = s.screen.step(Screen::ALL.len() - 1);
            Effect::None
        }
        Action::Rescan if !s.detecting => {
            s.detecting = true;
            Effect::Rescan
        }
        _ => Effect::None,
    }
}

fn open(s: &mut State, o: Overlay) -> Effect {
    s.overlay = Some(o);
    s.query.clear();
    s.cursor = 0;
    Effect::None
}

/// Keys typed into the help filter or the palette. Returns an action when one is chosen.
pub fn type_key(s: &mut State, key: KeyEvent) -> Option<Action> {
    let palette = match s.overlay {
        Some(Overlay::Palette) => true,
        Some(Overlay::Help) => false,
        _ => return None,
    };
    let last = if palette {
        super::palette::entries(&s.query).len().saturating_sub(1)
    } else {
        super::help::lines(&s.query).len().saturating_sub(1)
    };
    match key.code {
        KeyCode::Esc => return Some(Action::Cancel),
        KeyCode::Backspace => {
            s.query.pop();
            s.cursor = 0;
        }
        KeyCode::Up => s.cursor = s.cursor.saturating_sub(1),
        KeyCode::Down => s.cursor = (s.cursor + 1).min(last),
        KeyCode::PageUp => s.cursor = s.cursor.saturating_sub(10),
        KeyCode::PageDown => s.cursor = (s.cursor + 10).min(last),
        KeyCode::Home => s.cursor = 0,
        KeyCode::End => s.cursor = last,
        KeyCode::Enter if palette => {
            let chosen = super::palette::entries(&s.query).get(s.cursor).map(|e| e.0);
            if chosen.is_some() {
                s.overlay = None;
            }
            return chosen;
        }
        KeyCode::Char(c)
            if !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
        {
            s.query.push(c);
            s.cursor = 0;
        }
        _ => {}
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> State {
        State::new((100, 30))
    }

    #[test]
    fn quit_asks_first_when_there_are_unsaved_changes() {
        let mut s = state();
        s.dirty = true;
        assert_eq!(update(&mut s, Action::Quit), Effect::None);
        assert_eq!(s.overlay, Some(Overlay::Confirm(Confirm::Discard)));
        assert_eq!(
            update(&mut s, Action::Quit),
            Effect::Quit,
            "a second press quits"
        );
    }

    #[test]
    fn quit_is_immediate_when_clean() {
        let mut s = state();
        assert_eq!(update(&mut s, Action::Quit), Effect::Quit);
    }

    #[test]
    fn the_discard_dialog_answers() {
        let mut s = state();
        s.dirty = true;
        update(&mut s, Action::Quit);
        assert_eq!(update(&mut s, Action::No), Effect::None);
        assert_eq!(s.overlay, None);
        update(&mut s, Action::Quit);
        assert_eq!(update(&mut s, Action::Yes), Effect::Quit);
    }

    #[test]
    fn cancel_closes_an_overlay() {
        let mut s = state();
        update(&mut s, Action::Help);
        assert_eq!(s.overlay, Some(Overlay::Help));
        update(&mut s, Action::Cancel);
        assert_eq!(s.overlay, None);
        update(&mut s, Action::Palette);
        assert_eq!(s.overlay, Some(Overlay::Palette));
    }

    #[test]
    fn screens_wrap_and_jump() {
        let mut s = state();
        update(&mut s, Action::Goto(Screen::Test));
        assert_eq!(s.screen, Screen::Test);
        update(&mut s, Action::NextScreen);
        assert_eq!(s.screen, Screen::Check, "Test wraps to Check");
        update(&mut s, Action::PrevScreen);
        assert_eq!(s.screen, Screen::Test, "Check wraps back to Test");
    }

    #[test]
    fn an_open_overlay_blocks_navigation_but_not_closing() {
        let mut s = state();
        update(&mut s, Action::Help);
        assert_eq!(update(&mut s, Action::Goto(Screen::Review)), Effect::None);
        assert_eq!(s.screen, Screen::Check);
        assert_eq!(update(&mut s, Action::Rescan), Effect::None);
        assert_eq!(update(&mut s, Action::Quit), Effect::Quit);
    }

    #[test]
    fn rescan_does_not_stack() {
        let mut s = state();
        assert_eq!(update(&mut s, Action::Rescan), Effect::Rescan);
        assert!(s.detecting);
        assert_eq!(update(&mut s, Action::Rescan), Effect::None);
    }

    #[test]
    fn opening_help_or_the_palette_starts_with_an_empty_filter() {
        let mut s = state();
        update(&mut s, Action::Palette);
        type_key(
            &mut s,
            KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE),
        );
        assert_eq!(s.query, "r");
        update(&mut s, Action::Cancel);
        update(&mut s, Action::Help);
        assert_eq!((s.query.as_str(), s.cursor), ("", 0));
    }

    #[test]
    fn a_click_inside_an_overlay_keeps_it_open() {
        let mut s = state();
        update(&mut s, Action::Help);
        assert_eq!(update(&mut s, Action::Stay), Effect::None);
        assert_eq!(s.overlay, Some(Overlay::Help));
    }

    #[test]
    fn the_palette_runs_the_chosen_action() {
        let mut s = state();
        update(&mut s, Action::Palette);
        for c in "rescan".chars() {
            assert_eq!(
                type_key(&mut s, KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)),
                None
            );
        }
        let chosen = type_key(&mut s, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(chosen, Some(Action::Rescan));
        assert_eq!(s.overlay, None, "closed before the action runs");
        assert_eq!(update(&mut s, Action::Rescan), Effect::Rescan);
    }

    #[test]
    fn enter_with_no_match_keeps_the_palette_open_and_esc_closes_it() {
        let mut s = state();
        update(&mut s, Action::Palette);
        type_key(
            &mut s,
            KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE),
        );
        assert_eq!(
            type_key(&mut s, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            None
        );
        assert_eq!(s.overlay, Some(Overlay::Palette));
        assert_eq!(
            type_key(&mut s, KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            Some(Action::Cancel)
        );
    }

    #[test]
    fn the_help_scroll_stays_inside_the_text() {
        let mut s = state();
        update(&mut s, Action::Help);
        for _ in 0..500 {
            type_key(&mut s, KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        }
        assert_eq!(s.cursor, crate::tui::help::lines("").len() - 1);
    }
}
