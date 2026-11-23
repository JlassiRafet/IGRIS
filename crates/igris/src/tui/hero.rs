//! The first screen, while the first scan runs: the wordmark centred with its sheen, one line on
//! what IGRIS is, and a progress bar with the probe that is running. It is a real wait, so nothing
//! is delayed for effect: the work layout replaces it the moment the scan is done.
use super::state::State;
use super::style::Role;
use super::theme::{Glyphs, Theme};
use super::widgets::progress;
use super::wordmark::{WIDTH, big_height, big_width, wordmark, wordmark_big};
use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

const BAR: u16 = 32;

fn centred(area: Rect, row: u16, width: u16) -> Rect {
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + row,
        width.min(area.width),
        1,
    )
}

/// How large the wordmark is for this window: 0 is the small one (4 rows), 1 and 2 are the large ones.
fn scale(area: Rect, t: &Theme) -> u16 {
    match (t.glyphs, area.width, area.height) {
        (Glyphs::Ascii, _, _) => 0,
        (_, w, h) if w >= 150 && h >= 44 => 2,
        (_, w, h) if w >= 100 && h >= 30 => 1,
        _ => 0,
    }
}

/// `area` is the whole frame; the last row is left for the key hints.
pub fn draw(f: &mut Frame, area: Rect, s: &State, t: &Theme) {
    let rpp = scale(area, t);
    let (wh, ww) = if rpp == 0 {
        (4, WIDTH)
    } else {
        (big_height(rpp), big_width(rpp))
    };
    let avail = area.height.saturating_sub(1);
    let y0 = avail.saturating_sub(wh + 6) / 2;
    if t.glyphs == Glyphs::Unicode {
        let r = Rect::new(
            area.x + area.width.saturating_sub(ww) / 2,
            area.y + y0,
            ww,
            wh,
        );
        let lines = if rpp == 0 {
            wordmark(t, s.tick)
        } else {
            wordmark_big(t, s.tick, rpp)
        };
        f.render_widget(Paragraph::new(lines), r);
    } else {
        let line = Line::styled("IGRIS", t.style(Role::Text).add_modifier(Modifier::BOLD));
        f.render_widget(
            Paragraph::new(line).alignment(Alignment::Center),
            Rect::new(area.x, area.y + y0, area.width, 1),
        );
    }
    for (i, text) in super::header::lines(t, area.width).into_iter().enumerate() {
        let r = Rect::new(area.x, area.y + y0 + wh + 1 + i as u16, area.width, 1);
        f.render_widget(
            Paragraph::new(Line::styled(text, t.style(Role::Dim))).alignment(Alignment::Center),
            r,
        );
    }
    let (label, pct) = match s.scan {
        Some((done, total, what)) if total > 0 => (what, Some(done as f32 / total as f32)),
        _ => ("starting", None),
    };
    let what = format!("Scanning this machine · {label}");
    let what = if t.glyphs == Glyphs::Ascii {
        what.replace('·', "-")
    } else {
        what
    };
    f.render_widget(
        Paragraph::new(Line::styled(what, t.style(Role::Text))).alignment(Alignment::Center),
        Rect::new(area.x, area.y + y0 + wh + 4, area.width, 1),
    );
    let mut bar = progress(t, BAR, pct, s.tick).spans;
    let tail = pct.map_or(String::new(), |p| {
        format!(" {:>3}%", (p * 100.0).round() as u32)
    });
    let width = BAR + tail.chars().count() as u16;
    bar.push(Span::styled(tail, t.style(Role::Dim)));
    f.render_widget(
        Paragraph::new(Line::from(bar)),
        centred(area, y0 + wh + 5, width),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::theme::{ColorDepth, Env};
    use ratatui::{Terminal, backend::TestBackend};

    fn rows(s: &State, t: &Theme, w: u16, h: u16) -> Vec<String> {
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        term.draw(|f| draw(f, f.area(), s, t)).unwrap();
        let b = term.backend().buffer().clone();
        (0..h)
            .map(|y| (0..w).map(|x| b[(x, y)].symbol().to_string()).collect())
            .collect()
    }

    #[test]
    fn the_wordmark_is_centred_and_the_scan_is_named_with_its_progress() {
        let mut s = State::new((100, 30));
        s.scan = Some((3, 6, "agents"));
        let t = Theme::detect(&Env::default());
        let r = rows(&s, &t, 100, 30);
        // 100x30 gets the large wordmark: 38 columns, centred.
        let left = r
            .iter()
            .filter(|l| l.contains('█'))
            .map(|l| l.find('█').unwrap())
            .min()
            .unwrap();
        assert_eq!(left, (100 - 38) / 2, "centred:\n{r:?}");
        assert!(
            r.iter()
                .any(|l| l.contains("Scanning this machine · agents")),
            "{r:?}"
        );
        let bar = r.iter().find(|l| l.contains('░')).expect("the bar");
        assert!(
            bar.contains(" 50%") && bar.matches('█').count() == 16,
            "{bar}"
        );
    }

    #[test]
    fn before_any_progress_the_bar_sweeps_and_says_starting() {
        let s = State::new((100, 30));
        let t = Theme::detect(&Env::default());
        let a = rows(&s, &t, 100, 30);
        assert!(a.iter().any(|l| l.contains("starting")), "{a:?}");
        let mut later = s.clone();
        later.tick = 5;
        assert_ne!(a, rows(&later, &t, 100, 30), "the sweep moves");
    }

    #[test]
    fn a_plain_terminal_gets_plain_text_and_a_24_row_window_still_fits() {
        let mut s = State::new((80, 24));
        s.scan = Some((0, 6, "machine"));
        let t = Theme {
            depth: ColorDepth::None,
            glyphs: Glyphs::Ascii,
            motion: false,
        };
        let r = rows(&s, &t, 80, 24);
        assert!(r.iter().all(|l| l.is_ascii()), "{r:?}");
        assert!(
            r.iter().any(|l| l.contains("IGRIS"))
                && r.iter()
                    .any(|l| l.contains("Scanning this machine - machine"))
        );
        let u = rows(&s, &Theme::detect(&Env::default()), 80, 24);
        assert!(
            u.iter().any(|l| l.contains("▀█▀")) && u.iter().any(|l| l.contains('░')),
            "{u:?}"
        );
    }
}
