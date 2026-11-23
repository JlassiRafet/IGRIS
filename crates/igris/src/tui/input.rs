//! A single-line text field with Herdr's editing keys.
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputEvent {
    None,
    Changed,
    Commit,
    Cancel,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Input {
    text: String,
    /// In characters, 0..=len.
    cursor: usize,
}

// shortcut: char boundaries, not graphemes; upgrade if combining marks are reported.
impl Input {
    pub fn new(value: &str) -> Input {
        Input {
            text: value.to_string(),
            cursor: value.chars().count(),
        }
    }

    pub fn value(&self) -> &str {
        &self.text
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    fn len(&self) -> usize {
        self.text.chars().count()
    }

    fn byte(&self, char_index: usize) -> usize {
        self.text
            .char_indices()
            .nth(char_index)
            .map_or(self.text.len(), |(b, _)| b)
    }

    fn remove(&mut self, from: usize, to: usize) {
        let (a, b) = (self.byte(from), self.byte(to));
        self.text.replace_range(a..b, "");
        self.cursor = from;
    }

    fn word_back(&self) -> usize {
        let chars: Vec<char> = self.text.chars().collect();
        let mut i = self.cursor;
        while i > 0 && chars[i - 1].is_whitespace() {
            i -= 1;
        }
        while i > 0 && !chars[i - 1].is_whitespace() {
            i -= 1;
        }
        i
    }

    fn word_forward(&self) -> usize {
        let chars: Vec<char> = self.text.chars().collect();
        let mut i = self.cursor;
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        while i < chars.len() && !chars[i].is_whitespace() {
            i += 1;
        }
        i
    }

    pub fn insert(&mut self, c: char) {
        let b = self.byte(self.cursor);
        self.text.insert(b, c);
        self.cursor += 1;
    }

    /// A paste is one line: line breaks become spaces.
    pub fn paste(&mut self, s: &str) {
        for c in s.chars() {
            self.insert(if c == '\n' || c == '\r' { ' ' } else { c });
        }
    }

    pub fn handle(&mut self, key: KeyEvent) -> InputEvent {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        let before = self.text.clone();
        match key.code {
            KeyCode::Esc => return InputEvent::Cancel,
            KeyCode::Enter => return InputEvent::Commit,
            KeyCode::Left if alt => self.cursor = self.word_back(),
            KeyCode::Right if alt => self.cursor = self.word_forward(),
            KeyCode::Left => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Right => self.cursor = (self.cursor + 1).min(self.len()),
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.len(),
            KeyCode::Backspace if self.cursor > 0 => self.remove(self.cursor - 1, self.cursor),
            KeyCode::Delete if self.cursor < self.len() => {
                self.remove(self.cursor, self.cursor + 1)
            }
            KeyCode::Char('a') if ctrl => self.cursor = 0,
            KeyCode::Char('e') if ctrl => self.cursor = self.len(),
            KeyCode::Char('u') if ctrl => self.remove(0, self.cursor),
            KeyCode::Char('k') if ctrl => self.remove(self.cursor, self.len()),
            KeyCode::Char('w') if ctrl => self.remove(self.word_back(), self.cursor),
            KeyCode::Char('b') if alt => self.cursor = self.word_back(),
            KeyCode::Char('f') if alt => self.cursor = self.word_forward(),
            KeyCode::Char(c) if !ctrl && !alt => self.insert(c),
            _ => {}
        }
        if self.text != before {
            InputEvent::Changed
        } else {
            InputEvent::None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, mods)
    }
    fn ctrl(c: char) -> KeyEvent {
        key(KeyCode::Char(c), KeyModifiers::CONTROL)
    }
    fn alt(c: char) -> KeyEvent {
        key(KeyCode::Char(c), KeyModifiers::ALT)
    }
    fn plain(code: KeyCode) -> KeyEvent {
        key(code, KeyModifiers::NONE)
    }

    #[test]
    fn deleting_words_lines_and_characters() {
        let mut i = Input::new("hello world");
        i.handle(ctrl('w'));
        assert_eq!(i.value(), "hello ");
        i.handle(ctrl('u'));
        assert_eq!(i.value(), "");

        let mut i = Input::new("hello world");
        i.handle(ctrl('a'));
        i.handle(ctrl('k'));
        assert_eq!(i.value(), "");

        let mut i = Input::new("hello world");
        i.handle(plain(KeyCode::Left));
        i.handle(plain(KeyCode::Backspace));
        assert_eq!(
            i.value(),
            "hello word",
            "deletes the second-to-last character"
        );
        i.handle(plain(KeyCode::Delete));
        assert_eq!(
            i.value(),
            "hello wor",
            "Delete takes the character under the cursor"
        );
    }

    #[test]
    fn moving_by_words_and_to_the_ends() {
        let mut i = Input::new("hello world");
        i.handle(alt('b'));
        assert_eq!(i.cursor(), 6, "start of `world`");
        i.handle(alt('b'));
        assert_eq!(i.cursor(), 0);
        i.handle(alt('f'));
        assert_eq!(i.cursor(), 5, "end of `hello`");
        i.handle(plain(KeyCode::End));
        assert_eq!(i.cursor(), 11);
        i.handle(plain(KeyCode::Home));
        assert_eq!(i.cursor(), 0);
        i.handle(ctrl('e'));
        assert_eq!(i.cursor(), 11);
    }

    #[test]
    fn typing_inserts_at_the_cursor_and_reports_changes() {
        let mut i = Input::new("ac");
        i.handle(plain(KeyCode::Left));
        assert_eq!(i.handle(plain(KeyCode::Char('b'))), InputEvent::Changed);
        assert_eq!(i.value(), "abc");
        assert_eq!(
            i.handle(plain(KeyCode::Left)),
            InputEvent::None,
            "moving is not a change"
        );
    }

    #[test]
    fn enter_commits_and_esc_cancels() {
        let mut i = Input::new("x");
        assert_eq!(i.handle(plain(KeyCode::Enter)), InputEvent::Commit);
        assert_eq!(i.handle(plain(KeyCode::Esc)), InputEvent::Cancel);
        assert_eq!(i.value(), "x");
    }

    #[test]
    fn a_paste_is_one_line() {
        let mut i = Input::new("");
        i.paste("a\nb");
        assert_eq!(i.value(), "a b");
        assert_eq!(i.cursor(), 3);
    }

    #[test]
    fn multibyte_text_never_splits_a_character() {
        let mut i = Input::new("héllo ünï");
        i.handle(ctrl('w'));
        assert_eq!(i.value(), "héllo ");
        i.handle(plain(KeyCode::Backspace));
        i.handle(plain(KeyCode::Backspace));
        assert_eq!(i.value(), "héll");
    }

    #[test]
    fn keys_at_the_edges_do_nothing_and_do_not_panic() {
        let mut i = Input::new("");
        for code in [
            KeyCode::Backspace,
            KeyCode::Delete,
            KeyCode::Left,
            KeyCode::Right,
            KeyCode::Home,
        ] {
            assert_eq!(i.handle(plain(code)), InputEvent::None);
        }
        i.handle(ctrl('w'));
        i.handle(alt('b'));
        assert_eq!((i.value(), i.cursor()), ("", 0));
    }
}
