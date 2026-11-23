//! Every thing the user can ask for. Keys and mouse clicks both become one of these.
use super::state::Screen;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Quit,
    Help,
    Palette,
    Goto(Screen),
    NextScreen,
    PrevScreen,
    Rescan,
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
    NextField,
    PrevField,
    Activate,
    Toggle,
    Cancel,
    Yes,
    No,
}

impl Action {
    pub const ALL: &[Action] = &[
        Action::Quit,
        Action::Help,
        Action::Palette,
        Action::Goto(Screen::Check),
        Action::Goto(Screen::Pipeline),
        Action::Goto(Screen::Review),
        Action::Goto(Screen::Test),
        Action::NextScreen,
        Action::PrevScreen,
        Action::Rescan,
        Action::Up,
        Action::Down,
        Action::PageUp,
        Action::PageDown,
        Action::Home,
        Action::End,
        Action::NextField,
        Action::PrevField,
        Action::Activate,
        Action::Toggle,
        Action::Cancel,
        Action::Yes,
        Action::No,
    ];
}
