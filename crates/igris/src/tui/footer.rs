//! The key hints on the bottom row. Each one is clickable and names a key from the keymap.
use super::action::Action;
use super::hit::HitMap;
use super::keymap::{Context, keys_for};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

pub fn footer_hints(ctx: Context) -> Vec<(String, Action)> {
    let spec: &[(Action, &str)] = match ctx {
        Context::List => &[
            (Action::Activate, "open"),
            (Action::Rescan, "rescan"),
            (Action::Palette, "filter"),
            (Action::Help, "help"),
            (Action::Quit, "quit"),
        ],
        Context::Form => &[
            (Action::NextField, "next"),
            (Action::Activate, "edit"),
            (Action::Palette, "filter"),
            (Action::Help, "help"),
            (Action::Quit, "quit"),
        ],
        Context::Dialog => &[
            (Action::Yes, "yes"),
            (Action::No, "no"),
            (Action::Cancel, "cancel"),
        ],
        Context::Input | Context::Global => &[(Action::Cancel, "close")],
    };
    spec.iter()
        .map(|(a, verb)| {
            (
                format!(
                    "{} {verb}",
                    keys_for(*a).first().cloned().unwrap_or_default()
                ),
                *a,
            )
        })
        .collect()
}

/// Draw the hints on one row and register each as a clickable area.
pub fn draw_footer(f: &mut Frame, row: Rect, ctx: Context, hits: &mut HitMap) {
    let mut x = row.x;
    let mut spans = Vec::new();
    for (text, action) in footer_hints(ctx) {
        let width = text.chars().count() as u16;
        if x + width > row.x + row.width {
            break;
        }
        hits.add(Rect::new(x, row.y, width, 1), action);
        spans.push(Span::styled(text, Style::new()));
        spans.push(Span::raw("   "));
        x += width + 3;
    }
    f.render_widget(Paragraph::new(Line::from(spans)), row);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::keymap::BINDINGS;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn every_hint_names_a_real_key_for_its_action() {
        for ctx in [
            Context::List,
            Context::Form,
            Context::Dialog,
            Context::Input,
            Context::Global,
        ] {
            for (text, action) in footer_hints(ctx) {
                let key = text.split(' ').next().unwrap();
                assert!(keys_for(action).iter().any(|k| k == key), "{ctx:?}: {text}");
                assert!(BINDINGS.iter().any(|b| b.action == action));
            }
        }
    }

    #[test]
    fn drawing_the_footer_registers_one_clickable_area_per_hint() {
        let mut term = Terminal::new(TestBackend::new(80, 3)).unwrap();
        let mut hits = HitMap::default();
        term.draw(|f| draw_footer(f, Rect::new(0, 2, 80, 1), Context::List, &mut hits))
            .unwrap();
        let mut x = 0u16;
        for (text, action) in footer_hints(Context::List) {
            assert_eq!(hits.at(x, 2), Some(action), "first cell of {text}");
            assert_eq!(
                hits.at(x + text.chars().count() as u16 - 1, 2),
                Some(action),
                "last cell of {text}"
            );
            x += text.chars().count() as u16 + 3;
        }
        assert_eq!(hits.at(x, 2), None, "nothing past the last hint");
    }
}
