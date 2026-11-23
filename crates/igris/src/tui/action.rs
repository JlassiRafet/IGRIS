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
    /// A click inside an overlay: keeps it open. Mouse only.
    Stay,
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
        Action::Stay,
    ];

    /// The words the help, the palette and the footer use for this action.
    pub fn describe(self) -> String {
        match self {
            Action::Quit => "Quit",
            Action::Help => "Open this help",
            Action::Palette => "Filter and run an action",
            Action::Goto(s) => return format!("Go to {}", s.title()),
            Action::NextScreen => "Next screen",
            Action::PrevScreen => "Previous screen",
            Action::Rescan => "Rescan this machine",
            Action::Up => "Move up",
            Action::Down => "Move down",
            Action::PageUp => "Page up",
            Action::PageDown => "Page down",
            Action::Home => "Jump to the start",
            Action::End => "Jump to the end",
            Action::NextField => "Next field or group",
            Action::PrevField => "Previous field or group",
            Action::Activate => "Open or press the focused item",
            Action::Toggle => "Fold, unfold or toggle",
            Action::Cancel => "Close, cancel or go back",
            Action::Yes => "Yes",
            Action::No => "No",
            Action::Stay => "",
        }
        .to_string()
    }
}
