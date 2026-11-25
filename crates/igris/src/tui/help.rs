//! The help overlay: how setup works, every key (generated from the keymap), the mouse, the docs.
use super::action::Action;
use super::keymap::keys_for;
use ratatui::style::Stylize;
use ratatui::text::Line;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HelpEntry {
    pub keys: String,
    pub what: String,
}

/// One entry per action that has a key, built from the keymap so help cannot drift from it.
pub fn help_entries() -> Vec<HelpEntry> {
    Action::ALL
        .iter()
        .filter_map(|a| {
            let keys = keys_for(*a);
            (!keys.is_empty()).then(|| HelpEntry {
                keys: keys.join(" or "),
                what: a.describe(),
            })
        })
        .collect()
}

pub fn filter<'a>(entries: &'a [HelpEntry], q: &str) -> Vec<&'a HelpEntry> {
    let q = q.to_lowercase();
    entries
        .iter()
        .filter(|e| e.keys.to_lowercase().contains(&q) || e.what.to_lowercase().contains(&q))
        .collect()
}

const HOW: [&str; 6] = [
    "1 Scan: see what this machine has and fix what is missing.",
    "2 Tools: pick the workspace, builder, reviewer, decision layer and assistant.",
    "3 Pipeline: set the check command that proves a task is done.",
    "4 Verify: one live check of each tool you chose; Review opens when they pass.",
    "5 Review: see the exact change; Apply writes igris.toml.",
    "6 Test: run the checks and, if you like, one sample task.",
];

const MOUSE: [&str; 6] = [
    "Click a tab: switch screen.",
    "Click a group header: fold or unfold.",
    "Click a row, field or button: focus and activate it.",
    "Click a key hint in the footer: run it.",
    "Click outside an overlay: close it.",
    "Wheel: scroll the list.",
];

/// What IGRIS promises. Checked in the polish design doc: no network crate in the build, loopback
/// probes only, fixed commands only.
const ABOUT: [&str; 3] = [
    "IGRIS itself sends nothing anywhere: no telemetry, no analytics.",
    "It stores no logins or keys; each tool keeps its own.",
    "It runs only fixed commands: a tool's own login, the Herdr installer after a yes, and the checks.",
];

/// All lines of the overlay for the current filter.
pub fn lines(query: &str) -> Vec<Line<'static>> {
    let all = help_entries();
    let mut out: Vec<Line<'static>> = vec![Line::from("How setup works").bold()];
    out.extend(HOW.iter().map(|l| Line::from(format!("  {l}"))));
    out.push(Line::from(""));
    let shown = filter(&all, query);
    out.push(Line::from(format!("Keys (type to filter: \"{query}\")")).bold());
    if shown.is_empty() {
        out.push(Line::from("  nothing matches"));
    }
    out.extend(
        shown
            .iter()
            .map(|e| Line::from(format!("  {:<18} {}", e.keys, e.what))),
    );
    out.push(Line::from(""));
    out.push(Line::from("Mouse").bold());
    out.extend(MOUSE.iter().map(|l| Line::from(format!("  {l}"))));
    out.push(Line::from(""));
    out.push(Line::from("About").bold());
    out.extend(ABOUT.iter().map(|l| Line::from(format!("  {l}"))));
    out.push(Line::from(""));
    out.push(Line::from(format!("Docs: {}", env!("CARGO_PKG_HOMEPAGE"))));
    out.push(Line::from(
        "Quit: q, or Ctrl+C from anywhere. Esc closes this help.",
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::keymap::{BINDINGS, key_label};

    #[test]
    fn every_binding_appears_in_the_help() {
        let entries = help_entries();
        for b in BINDINGS {
            let label = key_label(b.code, b.mods);
            assert!(
                entries
                    .iter()
                    .any(|e| e.what == b.action.describe()
                        && e.keys.split(" or ").any(|k| k == label)),
                "{label} for {:?} is missing from help",
                b.action
            );
        }
    }

    #[test]
    fn filtering_by_a_word_keeps_only_matching_entries() {
        let entries = help_entries();
        let quit = filter(&entries, "quit");
        assert_eq!(quit.len(), 1);
        assert!(quit[0].keys.contains("Ctrl+C"));
        assert_eq!(
            filter(&entries, "").len(),
            entries.len(),
            "an empty query keeps everything"
        );
        assert_eq!(
            filter(&entries, "ctrl+k").len(),
            1,
            "a key can be searched too"
        );
    }

    #[test]
    fn the_overlay_has_all_four_parts() {
        let text: String = lines("")
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        for part in [
            "How setup works",
            "Keys",
            "Mouse",
            "Docs: https://",
            "Quit:",
        ] {
            assert!(text.contains(part), "{part}");
        }
    }

    #[test]
    fn the_about_block_states_the_privacy_promises() {
        let text: String = lines("")
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            text.contains("About") && text.contains("IGRIS itself sends nothing anywhere"),
            "{text}"
        );
        assert!(
            text.contains("stores no logins or keys") && text.contains("only fixed commands"),
            "{text}"
        );
    }
}
