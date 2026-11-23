//! Clickable areas, filled while drawing so a click and its key cannot drift apart.
use super::action::Action;
use ratatui::layout::Rect;

#[derive(Default)]
pub struct HitMap(Vec<(Rect, Action)>);

impl HitMap {
    pub fn clear(&mut self) {
        self.0.clear();
    }

    pub fn add(&mut self, area: Rect, action: Action) {
        self.0.push((area, action));
    }

    /// The topmost area under the pointer, with its action: the one added last.
    pub fn rect_at(&self, x: u16, y: u16) -> Option<(Rect, Action)> {
        self.0
            .iter()
            .rev()
            .find(|(r, _)| x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height)
            .copied()
    }

    pub fn at(&self, x: u16, y: u16) -> Option<Action> {
        self.rect_at(x, y).map(|(_, a)| a)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_last_added_area_wins_and_edges_are_exclusive() {
        let mut h = HitMap::default();
        h.add(Rect::new(0, 0, 10, 3), Action::Help);
        h.add(Rect::new(2, 1, 3, 1), Action::Quit);
        assert_eq!(h.at(3, 1), Some(Action::Quit));
        assert_eq!(
            h.at(5, 1),
            Some(Action::Help),
            "x = 2 + 3 is outside the inner area"
        );
        assert_eq!(h.at(10, 0), None);
        h.clear();
        assert_eq!(h.at(3, 1), None);
    }
}
