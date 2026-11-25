//! What IGRIS ran: the command, how it ended and how long it took. In memory only, never written
//! anywhere, and never holding a key: an entry has no place for one.

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub argv: Vec<String>,
    pub code: Option<i32>,
    pub secs: f32,
}

// shortcut: the list is unbounded; a session runs a handful of commands, so cap it if that changes.
#[derive(Debug, Default)]
pub struct Activity {
    entries: Vec<Entry>,
}

impl Activity {
    pub fn push(&mut self, e: Entry) {
        self.entries.push(e);
    }

    // Read by the activity view of the last setup step (issue #66); the allow goes then.
    #[allow(dead_code)]
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_keep_what_was_pushed_in_order() {
        let mut a = Activity::default();
        assert!(a.entries().is_empty());
        for (cmd, code) in [("one", Some(0)), ("two", Some(3)), ("three", None)] {
            a.push(Entry {
                argv: vec![cmd.into(), "--flag".into()],
                code,
                secs: 0.5,
            });
        }
        let names: Vec<&str> = a.entries().iter().map(|e| e.argv[0].as_str()).collect();
        assert_eq!(names, ["one", "two", "three"]);
        assert_eq!(a.entries()[1].code, Some(3));
    }
}
