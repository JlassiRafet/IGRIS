//! The top band: the wordmark at rest on the left, what IGRIS does and its promise on the right.
//! When the window is short or the terminal is not UTF-8 it is one plain row.
use super::style::Role;
use super::theme::{Glyphs, Theme};
use super::wordmark::{WIDTH, wordmark};
use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::Modifier;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

const FULL_META_WIDTH: u16 = 90;

fn sep(t: &Theme) -> &'static str {
    if t.glyphs == Glyphs::Ascii {
        " - "
    } else {
        " · "
    }
}

/// What IGRIS does, and where it runs, in a few words. The second line is one word in a narrow window.
pub fn lines(t: &Theme, width: u16) -> [String; 2] {
    let s = sep(t);
    let promise = if width >= FULL_META_WIDTH {
        "Runs on your machine".to_string()
    } else {
        "Local".to_string()
    };
    [format!("Build{s}Review{s}Verify"), promise]
}

pub fn draw_band(f: &mut Frame, band: Rect, t: &Theme, tick: usize, big: bool) {
    let dim = t.style(Role::Dim);
    if big && band.width > WIDTH + 24 {
        f.render_widget(
            Paragraph::new(wordmark(t, tick)),
            Rect::new(band.x, band.y, WIDTH, band.height.min(4)),
        );
        let right = Rect::new(
            band.x + WIDTH + 1,
            band.y,
            band.width - WIDTH - 1,
            band.height,
        );
        let [what, promise] = lines(t, band.width);
        let lines = vec![Line::styled(what, dim), Line::styled(promise, dim)];
        f.render_widget(Paragraph::new(lines).alignment(Alignment::Right), right);
    } else {
        let left = Line::styled("IGRIS", t.style(Role::Text).add_modifier(Modifier::BOLD));
        f.render_widget(Paragraph::new(left), band);
        let [_, promise] = lines(t, band.width);
        f.render_widget(
            Paragraph::new(Line::styled(promise, dim)).alignment(Alignment::Right),
            band,
        );
    }
    // Only the first row is used when the band is one row tall.
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::theme::ColorDepth;
    use ratatui::{Terminal, backend::TestBackend};

    fn theme(glyphs: Glyphs) -> Theme {
        Theme {
            depth: ColorDepth::None,
            glyphs,
            motion: false,
        }
    }

    fn draw(
        t: &Theme,
        w: u16,
        h: u16,
        big: bool,
        tick: usize,
    ) -> (Vec<String>, Terminal<TestBackend>) {
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        term.draw(|f| draw_band(f, f.area(), t, tick, big)).unwrap();
        let b = term.backend().buffer().clone();
        let rows = (0..h)
            .map(|y| (0..w).map(|x| b[(x, y)].symbol().to_string()).collect())
            .collect();
        (rows, term)
    }

    #[test]
    fn the_tall_band_has_the_wordmark_on_the_left_and_the_promise_on_the_right() {
        let (rows, _) = draw(&theme(Glyphs::Unicode), 98, 4, true, 0);
        assert!(rows[1].starts_with("▀█▀ █▀▀ █▀█ ▀█▀ █▀▀"), "{rows:?}");
        assert!(
            rows[0].trim_end().ends_with("Build · Review · Verify"),
            "{rows:?}"
        );
        assert!(
            rows[1].trim_end().ends_with("Runs on your machine"),
            "{rows:?}"
        );
        assert!(
            !rows.concat().contains(env!("CARGO_PKG_VERSION")),
            "the band does not show a version: {rows:?}"
        );
    }

    #[test]
    fn the_short_band_is_one_row_with_the_name_and_the_promise() {
        let (rows, _) = draw(&theme(Glyphs::Unicode), 98, 1, false, 0);
        assert!(rows[0].starts_with("IGRIS "), "{rows:?}");
        assert!(!rows[0].contains(env!("CARGO_PKG_VERSION")), "{rows:?}");
        assert!(
            rows[0].trim_end().ends_with("Runs on your machine"),
            "{rows:?}"
        );
    }

    #[test]
    fn a_narrow_window_gets_the_short_promise() {
        let (rows, _) = draw(&theme(Glyphs::Unicode), 78, 1, false, 0);
        assert!(
            rows[0].trim_end().ends_with("Local") && !rows[0].contains("machine"),
            "{rows:?}"
        );
    }

    #[test]
    fn an_ascii_terminal_gets_only_ascii() {
        let (rows, _) = draw(&theme(Glyphs::Ascii), 98, 1, false, 0);
        assert!(rows[0].is_ascii(), "{rows:?}");
    }

    #[test]
    fn the_sheen_moves_in_the_band_only_when_motion_is_on() {
        let on = Theme {
            depth: ColorDepth::True,
            glyphs: Glyphs::Unicode,
            motion: true,
        };
        let off = Theme {
            motion: false,
            ..on
        };
        let (_, a) = draw(&on, 98, 4, true, 0);
        let (_, b) = draw(&on, 98, 4, true, 12);
        assert_ne!(a.backend().buffer(), b.backend().buffer());
        let (_, c) = draw(&off, 98, 4, true, 0);
        let (_, d) = draw(&off, 98, 4, true, 12);
        assert_eq!(c.backend().buffer(), d.backend().buffer());
    }
}
