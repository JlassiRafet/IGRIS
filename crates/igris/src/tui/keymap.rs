//! The one keymap. Help, the footer and the palette are generated from `BINDINGS`.
use super::action::Action;
use super::state::Screen;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Context {
    /// A screen or dialog list: letters are commands.
    List,
    /// A text field has focus: plain letters are text.
    Input,
    Dialog,
}

#[derive(Clone, Copy)]
pub enum Scope {
    /// Every context, even typing in a field.
    Always,
    /// Everywhere except a text field and a dialog.
    Anywhere,
    /// Moving about in a screen or overlay.
    Browse,
    Dialog,
}

impl Scope {
    fn covers(self, ctx: Context) -> bool {
        match self {
            Scope::Always => true,
            Scope::Anywhere | Scope::Browse => ctx == Context::List,
            Scope::Dialog => ctx == Context::Dialog,
        }
    }
}

pub struct Binding {
    pub scope: Scope,
    pub code: KeyCode,
    pub mods: KeyModifiers,
    pub action: Action,
}

const NONE: KeyModifiers = KeyModifiers::NONE;
const CTRL: KeyModifiers = KeyModifiers::CONTROL;

const fn b(scope: Scope, code: KeyCode, mods: KeyModifiers, action: Action) -> Binding {
    Binding {
        scope,
        code,
        mods,
        action,
    }
}

pub const BINDINGS: &[Binding] = &[
    b(Scope::Anywhere, KeyCode::Char('q'), NONE, Action::Quit),
    b(Scope::Always, KeyCode::Char('c'), CTRL, Action::Quit),
    b(Scope::Anywhere, KeyCode::Char('?'), NONE, Action::Help),
    b(Scope::Anywhere, KeyCode::F(1), NONE, Action::Help),
    b(Scope::Anywhere, KeyCode::Char('/'), NONE, Action::Palette),
    b(Scope::Anywhere, KeyCode::Char('k'), CTRL, Action::Palette),
    b(
        Scope::Anywhere,
        KeyCode::Char('1'),
        NONE,
        Action::Goto(Screen::Check),
    ),
    b(
        Scope::Anywhere,
        KeyCode::Char('2'),
        NONE,
        Action::Goto(Screen::Tools),
    ),
    b(
        Scope::Anywhere,
        KeyCode::Char('3'),
        NONE,
        Action::Goto(Screen::Pipeline),
    ),
    b(
        Scope::Anywhere,
        KeyCode::Char('4'),
        NONE,
        Action::Goto(Screen::Review),
    ),
    b(
        Scope::Anywhere,
        KeyCode::Char('5'),
        NONE,
        Action::Goto(Screen::Test),
    ),
    b(
        Scope::Anywhere,
        KeyCode::Char(']'),
        NONE,
        Action::NextScreen,
    ),
    b(
        Scope::Anywhere,
        KeyCode::Char('['),
        NONE,
        Action::PrevScreen,
    ),
    b(Scope::Anywhere, KeyCode::Char('r'), NONE, Action::Rescan),
    b(Scope::Browse, KeyCode::Down, NONE, Action::Down),
    b(Scope::Browse, KeyCode::Char('j'), NONE, Action::Down),
    b(Scope::Browse, KeyCode::Up, NONE, Action::Up),
    b(Scope::Browse, KeyCode::Char('k'), NONE, Action::Up),
    b(Scope::Browse, KeyCode::Tab, NONE, Action::NextField),
    b(Scope::Browse, KeyCode::BackTab, NONE, Action::PrevField),
    b(Scope::Browse, KeyCode::Home, NONE, Action::Home),
    b(Scope::Browse, KeyCode::End, NONE, Action::End),
    b(Scope::Browse, KeyCode::PageUp, NONE, Action::PageUp),
    b(Scope::Browse, KeyCode::PageDown, NONE, Action::PageDown),
    b(Scope::Browse, KeyCode::Enter, NONE, Action::Activate),
    b(Scope::Browse, KeyCode::Char(' '), NONE, Action::Toggle),
    b(Scope::Browse, KeyCode::Char('f'), NONE, Action::Filter),
    b(Scope::Browse, KeyCode::Char('c'), NONE, Action::Connect),
    b(Scope::Browse, KeyCode::Char('s'), NONE, Action::Skip),
    b(Scope::Browse, KeyCode::Esc, NONE, Action::Cancel),
    b(Scope::Dialog, KeyCode::Char('y'), NONE, Action::Yes),
    b(Scope::Dialog, KeyCode::Enter, NONE, Action::Yes),
    b(Scope::Dialog, KeyCode::Char('n'), NONE, Action::No),
    b(Scope::Dialog, KeyCode::Esc, NONE, Action::Cancel),
];

/// Actions that have no key, only a click.
#[cfg(test)]
pub const MOUSE_ONLY: &[Action] = &[Action::Stay, Action::Select(0)];

/// How a key is written in the help and the footer.
pub fn key_label(code: KeyCode, mods: KeyModifiers) -> String {
    let base = match code {
        KeyCode::Char(' ') => "Space".to_string(),
        KeyCode::Char(c) => c.to_string(),
        KeyCode::Enter => "Enter".into(),
        KeyCode::Esc => "Esc".into(),
        KeyCode::Tab => "Tab".into(),
        KeyCode::BackTab => "Shift+Tab".into(),
        KeyCode::Up => "↑".into(),
        KeyCode::Down => "↓".into(),
        KeyCode::Home => "Home".into(),
        KeyCode::End => "End".into(),
        KeyCode::PageUp => "PgUp".into(),
        KeyCode::PageDown => "PgDn".into(),
        KeyCode::F(n) => format!("F{n}"),
        _ => "?".into(),
    };
    if mods.contains(KeyModifiers::CONTROL) {
        format!("Ctrl+{}", base.to_uppercase())
    } else {
        base
    }
}

/// Every key bound to an action, in table order.
pub fn keys_for(action: Action) -> Vec<String> {
    BINDINGS
        .iter()
        .filter(|b| b.action == action)
        .map(|b| key_label(b.code, b.mods))
        .collect()
}

/// Some terminals add SHIFT to `?` or send `K` for Ctrl+Shift+K; match them like the plain key.
fn normalise(key: KeyEvent) -> (KeyCode, KeyModifiers) {
    let mut mods = key.modifiers;
    let mut code = key.code;
    if let KeyCode::Char(c) = code {
        mods -= KeyModifiers::SHIFT;
        if mods.contains(KeyModifiers::CONTROL) {
            code = KeyCode::Char(c.to_ascii_lowercase());
        }
    } else if code == KeyCode::BackTab {
        mods -= KeyModifiers::SHIFT;
    }
    (code, mods)
}

pub fn key_to_action(ctx: Context, key: KeyEvent) -> Option<Action> {
    let (code, mods) = normalise(key);
    BINDINGS
        .iter()
        .find(|b| b.scope.covers(ctx) && b.code == code && b.mods == mods)
        .map(|b| b.action)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn k(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), NONE)
    }

    #[test]
    fn the_documented_keys() {
        assert_eq!(key_to_action(Context::List, k('?')), Some(Action::Help));
        assert_eq!(key_to_action(Context::List, k('/')), Some(Action::Palette));
        assert_eq!(
            key_to_action(Context::List, KeyEvent::new(KeyCode::Char('k'), CTRL)),
            Some(Action::Palette),
            "Ctrl+K is the palette, plain k is up"
        );
        assert_eq!(key_to_action(Context::List, k('k')), Some(Action::Up));
        assert_eq!(
            key_to_action(Context::List, k('3')),
            Some(Action::Goto(Screen::Pipeline))
        );
        assert_eq!(key_to_action(Context::List, k('q')), Some(Action::Quit));
        assert_eq!(
            key_to_action(Context::List, KeyEvent::new(KeyCode::Char('c'), CTRL)),
            Some(Action::Quit)
        );
    }

    #[test]
    fn terminals_that_report_shift_still_match() {
        let shifted_question = KeyEvent::new(KeyCode::Char('?'), KeyModifiers::SHIFT);
        assert_eq!(
            key_to_action(Context::List, shifted_question),
            Some(Action::Help)
        );
        let shift_tab = KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT);
        assert_eq!(
            key_to_action(Context::List, shift_tab),
            Some(Action::PrevField)
        );
        let upper_ctrl_k = KeyEvent::new(KeyCode::Char('K'), CTRL | KeyModifiers::SHIFT);
        assert_eq!(
            key_to_action(Context::List, upper_ctrl_k),
            Some(Action::Palette)
        );
    }

    #[test]
    fn plain_letters_are_text_inside_an_input() {
        assert_eq!(key_to_action(Context::Input, k('q')), None);
        assert_eq!(key_to_action(Context::Input, k('?')), None);
        assert_eq!(
            key_to_action(Context::Input, KeyEvent::new(KeyCode::Char('c'), CTRL)),
            Some(Action::Quit),
            "Ctrl+C still quits"
        );
    }

    #[test]
    fn a_dialog_only_answers() {
        assert_eq!(key_to_action(Context::Dialog, k('y')), Some(Action::Yes));
        assert_eq!(key_to_action(Context::Dialog, k('n')), Some(Action::No));
        assert_eq!(key_to_action(Context::Dialog, k('q')), None);
        assert_eq!(
            key_to_action(Context::Dialog, KeyEvent::new(KeyCode::Esc, NONE)),
            Some(Action::Cancel)
        );
    }

    #[test]
    fn every_action_has_a_key_or_is_listed_as_mouse_only() {
        for a in Action::ALL {
            let has_key = BINDINGS.iter().any(|b| b.action == *a);
            assert!(
                has_key || MOUSE_ONLY.contains(a),
                "{a:?} cannot be reached by keyboard"
            );
        }
    }

    #[test]
    fn no_two_bindings_share_a_key_in_one_context() {
        for ctx in [Context::List, Context::Input, Context::Dialog] {
            let mut seen: Vec<(KeyCode, KeyModifiers)> = Vec::new();
            for b in BINDINGS.iter().filter(|b| b.scope.covers(ctx)) {
                let id = (b.code, b.mods);
                assert!(!seen.contains(&id), "{ctx:?}: {:?} is bound twice", b.code);
                seen.push(id);
            }
        }
    }
}
