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
    /// Type to filter the focused list (the Tools screen).
    Filter,
    /// Connect or check the focused tool (the Tools screen).
    Connect,
    /// Skip the check of a tool that failed.
    Skip,
    Cancel,
    Yes,
    No,
    /// A click inside an overlay: keeps it open. Mouse only.
    Stay,
    /// A click on an item of the screen or of a dialog: focus it and activate it. Mouse only.
    Select(usize),
    /// What the guided chain asks the app to run (its install, check and login commands).
    Chain(super::chain::model::Want),
}

impl Action {
    pub const ALL: &[Action] = &[
        Action::Quit,
        Action::Help,
        Action::Palette,
        Action::Goto(Screen::Check),
        Action::Goto(Screen::Tools),
        Action::Goto(Screen::Pipeline),
        Action::Goto(Screen::Verify),
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
        Action::Filter,
        Action::Connect,
        Action::Skip,
        Action::Cancel,
        Action::Yes,
        Action::No,
        Action::Stay,
        Action::Select(0),
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
            Action::Filter => "Type to filter the list",
            Action::Connect => "Connect or check the focused tool",
            Action::Skip => "Skip the check of the tool that failed",
            Action::Cancel => "Close, cancel or go back",
            Action::Yes => "Yes",
            Action::No => "No",
            Action::Stay | Action::Select(_) | Action::Chain(_) => "",
        }
        .to_string()
    }
}
