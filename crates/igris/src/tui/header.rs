//! The top of every screen: the wordmark when there is room, one line when there is not.
use super::theme::{Glyphs, Theme};
use super::wordmark::{WIDTH, wordmark};
use ratatui::style::Style;
use ratatui::text::{Line, Span};

pub const TAGLINE: &str = "Agents build and review. The shell decides when it is done.";
const TALL: u16 = 28;

pub fn header(theme: &Theme, width: u16, height: u16, tick: usize) -> Vec<Line<'static>> {
    // Block characters need a UTF-8 terminal; otherwise (or when short) one plain line.
    if height >= TALL && width >= WIDTH && theme.glyphs == Glyphs::Unicode {
        let mut lines = wordmark(theme, tick);
        lines.push(Line::from(TAGLINE));
        return lines;
    }
    vec![Line::from(vec![
        Span::styled("IGRIS", Style::new().bold()),
        Span::raw(format!(" {}", env!("CARGO_PKG_VERSION"))),
    ])]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::theme::ColorDepth;

    fn theme(glyphs: Glyphs) -> Theme {
        Theme {
            motion: false,
            depth: ColorDepth::None,
            glyphs,
        }
    }

    fn flat(l: &Line) -> String {
        l.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn a_tall_window_gets_the_wordmark_and_the_tagline() {
        let h = header(&theme(Glyphs::Unicode), 100, 28, 0);
        assert_eq!(h.len(), 4);
        assert_eq!(flat(&h[1]), "▀█▀ █▀▀ █▀█ ▀█▀ █▀▀");
        assert_eq!(flat(&h[3]), TAGLINE);
    }

    #[test]
    fn a_short_window_gets_one_line_with_the_version() {
        let h = header(&theme(Glyphs::Unicode), 100, 24, 0);
        assert_eq!(h.len(), 1);
        let line = flat(&h[0]);
        assert!(
            line.contains("IGRIS") && line.contains(env!("CARGO_PKG_VERSION")),
            "{line}"
        );
    }

    #[test]
    fn a_narrow_window_never_gets_a_wordmark_that_does_not_fit() {
        assert_eq!(header(&theme(Glyphs::Unicode), WIDTH - 1, 40, 0).len(), 1);
    }

    #[test]
    fn ascii_terminals_get_plain_ascii() {
        let h = header(&theme(Glyphs::Ascii), 100, 40, 0);
        assert_eq!(h.len(), 1);
        assert!(flat(&h[0]).is_ascii(), "{}", flat(&h[0]));
    }
}
