//! Form widgets built on `Input`: a picker, a number field and a list editor.
use super::input::{Input, InputEvent};
use ratatui::crossterm::event::{KeyCode, KeyEvent};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectItem {
    pub label: String,
    pub enabled: bool,
    /// Shown beside a disabled item, for example "Soon".
    pub note: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Select {
    pub items: Vec<SelectItem>,
    pub index: usize,
}

impl Select {
    pub fn new(items: Vec<SelectItem>, index: usize) -> Select {
        Select { items, index }
    }

    /// Choose item `i`. A disabled or missing item is refused and the choice stays where it was.
    pub fn choose(&mut self, i: usize) -> bool {
        if self.items.get(i).is_some_and(|it| it.enabled) {
            self.index = i;
            true
        } else {
            false
        }
    }

    /// Move to the next enabled item in a direction, stopping at the ends.
    pub fn step(&mut self, down: bool) {
        let mut i = self.index;
        loop {
            let next = if down { i + 1 } else { i.wrapping_sub(1) };
            if next >= self.items.len() {
                return;
            }
            i = next;
            if self.items[i].enabled {
                self.index = i;
                return;
            }
        }
    }

    pub fn current(&self) -> Option<&SelectItem> {
        self.items.get(self.index)
    }

    /// Up and Down move; nothing else is reported. Enter and Esc belong to the caller.
    pub fn handle(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => self.step(true),
            KeyCode::Up | KeyCode::Char('k') => self.step(false),
            _ => return false,
        }
        true
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Whole,
    Decimal,
}

/// A text field that only takes digits (and one dot for a decimal).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NumberInput {
    kind: Kind,
    input: Input,
}

impl NumberInput {
    pub fn whole(v: u64) -> NumberInput {
        NumberInput {
            kind: Kind::Whole,
            input: Input::new(&v.to_string()),
        }
    }

    pub fn decimal(v: f64) -> NumberInput {
        NumberInput {
            kind: Kind::Decimal,
            input: Input::new(&v.to_string()),
        }
    }

    pub fn text(&self) -> &str {
        self.input.value()
    }

    pub fn as_u64(&self) -> Option<u64> {
        self.input.value().parse().ok()
    }

    pub fn as_f64(&self) -> Option<f64> {
        self.input.value().parse().ok()
    }

    fn accepts(&self, c: char) -> bool {
        c.is_ascii_digit()
            || (self.kind == Kind::Decimal && c == '.' && !self.input.value().contains('.'))
    }

    pub fn handle(&mut self, key: KeyEvent) -> InputEvent {
        if matches!(key.code, KeyCode::Char(c) if key.modifiers.is_empty() && !self.accepts(c)) {
            return InputEvent::None;
        }
        self.input.handle(key)
    }

    pub fn paste(&mut self, s: &str) {
        for c in s.chars() {
            if self.accepts(c) {
                self.input.insert(c);
            }
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ListEditor {
    pub items: Vec<String>,
}

impl ListEditor {
    pub fn add(&mut self, s: &str) {
        self.items.push(s.to_string());
    }

    pub fn remove(&mut self, i: usize) -> bool {
        if i < self.items.len() {
            self.items.remove(i);
            true
        } else {
            false
        }
    }

    pub fn edit(&mut self, i: usize, s: &str) -> bool {
        self.items
            .get_mut(i)
            .map(|it| *it = s.to_string())
            .is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::KeyModifiers;

    fn k(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    fn item(label: &str, enabled: bool) -> SelectItem {
        SelectItem {
            label: label.into(),
            enabled,
            note: (!enabled).then(|| "Soon".into()),
        }
    }

    #[test]
    fn a_disabled_item_cannot_be_chosen_or_stepped_onto() {
        let mut s = Select::new(
            vec![item("Rules", true), item("Jev", false), item("Laya", false)],
            0,
        );
        assert!(!s.choose(1));
        assert_eq!(s.index, 0, "the choice stays put");
        assert!(
            !s.handle(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            "Enter is the caller's"
        );
        s.step(true);
        assert_eq!(s.index, 0, "no enabled item below");
        assert!(!s.choose(9), "a missing item is refused too");
    }

    #[test]
    fn stepping_skips_disabled_items_in_both_directions() {
        let mut s = Select::new(vec![item("a", true), item("b", false), item("c", true)], 0);
        s.step(true);
        assert_eq!(s.index, 2);
        assert!(s.handle(k('k')));
        assert_eq!(s.index, 0);
        assert_eq!(s.current().unwrap().label, "a");
    }

    #[test]
    fn a_number_field_takes_only_digits() {
        let mut n = NumberInput::whole(30);
        assert_eq!(n.handle(k('x')), InputEvent::None);
        assert_eq!(n.handle(k('7')), InputEvent::Changed);
        assert_eq!(n.text(), "307");
        assert_eq!(n.as_u64(), Some(307));
        n.paste("1a2\n3");
        assert_eq!(n.text(), "307123");
        assert_eq!(
            n.handle(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE)),
            InputEvent::Changed
        );
    }

    #[test]
    fn a_decimal_takes_one_dot() {
        let mut n = NumberInput::decimal(0.5);
        assert_eq!(n.handle(k('.')), InputEvent::None, "already has a dot");
        n.handle(k('2'));
        assert_eq!(n.as_f64(), Some(0.52));
        let mut empty = NumberInput::whole(0);
        empty.handle(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        assert_eq!(empty.as_u64(), None, "an empty field is not a number");
    }

    #[test]
    fn the_list_editor_keeps_order_and_ignores_bad_indexes() {
        let mut l = ListEditor::default();
        for s in ["a", "b", "c"] {
            l.add(s);
        }
        assert!(l.edit(1, "B"));
        assert!(l.remove(0));
        assert_eq!(l.items, ["B", "c"]);
        assert!(!l.remove(5) && !l.edit(5, "x"));
        assert_eq!(l.items, ["B", "c"]);
    }
}
