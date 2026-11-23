//! The quick-actions palette: type to filter the actions, Enter runs the chosen one.
use super::action::Action;
use super::state::Screen;

const RUNNABLE: [Action; 9] = [
    Action::Goto(Screen::Check),
    Action::Goto(Screen::Pipeline),
    Action::Goto(Screen::Review),
    Action::Goto(Screen::Test),
    Action::NextScreen,
    Action::PrevScreen,
    Action::Rescan,
    Action::Help,
    Action::Quit,
];

pub fn entries(query: &str) -> Vec<(Action, String)> {
    let q = query.to_lowercase();
    RUNNABLE
        .iter()
        .map(|a| (*a, a.describe()))
        .filter(|(_, name)| name.to_lowercase().contains(&q))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_query_filters_by_name_ignoring_case() {
        let r = entries("REVIEW");
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].0, Action::Goto(Screen::Review));
        assert_eq!(entries("").len(), RUNNABLE.len());
        assert!(entries("zzz").is_empty());
    }

    #[test]
    fn screens_can_be_reached_by_name() {
        assert_eq!(entries("go to").len(), 4);
    }
}
