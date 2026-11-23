//! Small reusable pieces: shortening text by display width, framed panels, status chips and the
//! progress bar. Every string from outside the binary is shortened here before it is drawn.

use super::style::{ACCENT, GLOW, Role, rgb};
use super::theme::{ColorDepth, Glyphs, Theme};
use crate::check::view::Status;
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::border;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Padding, Paragraph, Wrap};
use unicode_width::UnicodeWidthChar;

/// Shorten `s` to at most `width` display columns, ending in `ellipsis` when it was cut. Control
/// characters are dropped; a wide character is never split.
fn shorten(s: &str, width: usize, ellipsis: &str) -> String {
    let clean: Vec<char> = s.chars().filter(|c| !c.is_control()).collect();
    let w = |c: &char| c.width().unwrap_or(0);
    if clean.iter().map(w).sum::<usize>() <= width {
        return clean.into_iter().collect();
    }
    let room = width.saturating_sub(ellipsis.chars().count());
    let mut out = String::new();
    let mut used = 0;
    for c in &clean {
        if used + w(c) > room {
            break;
        }
        used += w(c);
        out.push(*c);
    }
    if width >= ellipsis.chars().count() {
        out.push_str(ellipsis);
    }
    out
}

/// `"hello world"` to 8 columns is `"hello w…"`.
pub fn ellipsize(s: &str, width: usize) -> String {
    shorten(s, width, "…")
}

/// The same for terminals that cannot show `…`: three dots, and anything that is not ASCII is
/// replaced (`·` by `-`, the rest by `?`) so the text never garbles on a non-UTF-8 terminal.
pub fn ellipsize_ascii(s: &str, width: usize) -> String {
    let plain: String = s
        .chars()
        .map(|c| match c {
            c if c.is_ascii() => c,
            '·' | '–' | '—' => '-',
            '’' | '‘' => '\'',
            _ => '?',
        })
        .collect();
    shorten(&plain, width, "...")
}

/// Like `ellipsize` but keeps the END: `…/igris/igris.toml`. For paths, where the file name matters
/// most. Control characters are dropped; a wide character is never split.
pub fn ellipsize_start(s: &str, width: usize, ellipsis: &str) -> String {
    let clean: Vec<char> = s.chars().filter(|c| !c.is_control()).collect();
    let w = |c: &char| c.width().unwrap_or(0);
    if clean.iter().map(w).sum::<usize>() <= width {
        return clean.into_iter().collect();
    }
    let room = width.saturating_sub(ellipsis.chars().count());
    let mut tail: Vec<char> = vec![];
    let mut used = 0;
    for c in clean.iter().rev() {
        if used + w(c) > room {
            break;
        }
        used += w(c);
        tail.push(*c);
    }
    tail.reverse();
    let mut out = if width >= ellipsis.chars().count() {
        ellipsis.to_string()
    } else {
        String::new()
    };
    out.extend(tail);
    out
}

/// The path version of `fit`: the start is cut, the file name stays.
pub fn fit_path(t: &Theme, s: &str, width: usize) -> String {
    match t.glyphs {
        Glyphs::Unicode => ellipsize_start(s, width, "…"),
        Glyphs::Ascii => ellipsize_start(s, width, "..."),
    }
}

/// The right one for this terminal's glyph set.
pub fn fit(t: &Theme, s: &str, width: usize) -> String {
    match t.glyphs {
        Glyphs::Unicode => ellipsize(s, width),
        Glyphs::Ascii => ellipsize_ascii(s, width),
    }
}

/// `s` shortened to `width` columns and padded with spaces to exactly that width.
pub fn pad(t: &Theme, s: &str, width: usize) -> String {
    let f = fit(t, s, width);
    let fill = width.saturating_sub(unicode_width::UnicodeWidthStr::width(f.as_str()));
    f + &" ".repeat(fill)
}

/// A rounded frame with the title in the border (`+-|` on ASCII terminals) and one column of padding.
pub fn panel<'a>(t: &Theme, title: &str, focused: bool) -> Block<'a> {
    let border_role = if focused {
        Role::BorderFocus
    } else {
        Role::Border
    };
    let title = Line::from(vec![
        Span::raw(" "),
        Span::styled(
            title.to_string(),
            t.style(Role::Text).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
    ]);
    let block = Block::bordered()
        .border_style(t.style(border_role))
        .padding(Padding::horizontal(1))
        .title(title);
    match t.glyphs {
        Glyphs::Unicode => block.border_type(BorderType::Rounded),
        Glyphs::Ascii => block.border_set(ASCII_BORDER),
    }
}

/// `+-|`, for terminals that cannot draw box characters.
const ASCII_BORDER: border::Set = border::Set {
    top_left: "+",
    top_right: "+",
    bottom_left: "+",
    bottom_right: "+",
    vertical_left: "|",
    vertical_right: "|",
    horizontal_top: "-",
    horizontal_bottom: "-",
};

pub fn status_glyph(t: &Theme, st: Status) -> &'static str {
    match (st, t.glyphs) {
        (Status::Ready, Glyphs::Unicode) => "✓",
        (Status::NeedsStep, Glyphs::Unicode) => "●",
        (Status::Optional, Glyphs::Unicode) => "○",
        (Status::Info, Glyphs::Unicode) => "·",
        (Status::Ready, Glyphs::Ascii) => "[x]",
        (Status::NeedsStep, Glyphs::Ascii) => "[!]",
        (Status::Optional, Glyphs::Ascii) => "[-]",
        (Status::Info, Glyphs::Ascii) => "[i]",
    }
}

pub fn status_word(st: Status) -> &'static str {
    match st {
        Status::Ready => "ready",
        Status::NeedsStep => "needs a step",
        Status::Optional => "optional",
        Status::Info => "info",
    }
}

pub fn status_role(st: Status) -> Role {
    match st {
        Status::Ready => Role::Ok,
        Status::NeedsStep => Role::Bad,
        Status::Optional => Role::Optional,
        Status::Info => Role::Info,
    }
}

/// `✓ 4 ready`: glyph, count and word, in the status colour. Never colour alone.
pub fn chip(t: &Theme, st: Status, n: usize) -> Vec<Span<'static>> {
    let style = t.style(status_role(st));
    vec![
        Span::styled(status_glyph(t, st), style),
        Span::styled(format!(" {n} {}", status_word(st)), style),
    ]
}

/// A strip under a list: a rule, then the full text of the focused item. Lines are wrapped.
pub fn detail_strip<'a>(t: &Theme, lines: Vec<Line<'a>>) -> Paragraph<'a> {
    let block = Block::new()
        .borders(Borders::TOP)
        .border_style(t.style(Role::Border));
    let block = match t.glyphs {
        Glyphs::Unicode => block,
        Glyphs::Ascii => block.border_set(ASCII_BORDER),
    };
    Paragraph::new(lines).wrap(Wrap { trim: true }).block(block)
}

/// Width of the status column at the right edge: glyph, space, word ("● needs a step").
pub fn status_width(t: &Theme) -> usize {
    if t.glyphs == Glyphs::Ascii { 16 } else { 14 }
}

/// `● needs a step`, right-aligned in the status column and coloured by its role.
pub fn status_cell(t: &Theme, st: Status) -> Span<'static> {
    let text = format!("{} {}", status_glyph(t, st), status_word(st));
    let fill = status_width(t).saturating_sub(unicode_width::UnicodeWidthStr::width(text.as_str()));
    Span::styled(" ".repeat(fill) + &text, t.style(status_role(st)))
}

/// A button. The primary one (the thing to do next) is red; the others are light-blue text in
/// brackets. A focused one is also bold and underlined, so focus never relies on colour.
pub fn button(t: &Theme, label: &str, primary: bool, focused: bool) -> Span<'static> {
    let mut style = if primary {
        t.style(Role::Primary)
    } else {
        t.style(Role::Link)
    };
    if focused {
        style = style.add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
    }
    if primary {
        Span::styled(format!(" {label} "), style)
    } else {
        Span::styled(format!("[ {label} ]"), style)
    }
}

/// A progress bar exactly `width` cells wide. `Some(fraction)` fills from the left; `None` is a
/// highlight sweeping back and forth, for work whose length is not known. The percentage is the
/// caller's to add.
pub fn progress(t: &Theme, width: u16, pct: Option<f32>, tick: usize) -> Line<'static> {
    let width = usize::from(width);
    let (full, empty) = match t.glyphs {
        Glyphs::Unicode => ("█", "░"),
        Glyphs::Ascii => ("#", "-"),
    };
    let window = match pct {
        Some(p) => (0, (p.clamp(0.0, 1.0) * width as f32).round() as usize),
        None => {
            let len = (width / 3).clamp(1, 6).min(width);
            let span = width - len;
            let cycle = (2 * span).max(1);
            let p = tick % cycle;
            let start = if p <= span { p } else { cycle - p };
            (start, start + len)
        }
    };
    let cells = (0..width).map(|i| {
        if (window.0..window.1).contains(&i) {
            let k = if window.1 - window.0 <= 1 {
                1.0
            } else {
                (i - window.0) as f32 / (window.1 - window.0 - 1) as f32
            };
            Span::styled(full, filled_style(t, k))
        } else {
            Span::styled(empty, t.style(Role::Dim))
        }
    });
    Line::from(cells.collect::<Vec<_>>())
}

/// Filled cells run from the accent blue to the glow, left to right.
fn filled_style(t: &Theme, k: f32) -> Style {
    match t.depth {
        ColorDepth::True => {
            let mix =
                |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * k).round() as u8;
            Style::new().fg(rgb((
                mix(ACCENT.0, GLOW.0),
                mix(ACCENT.1, GLOW.1),
                mix(ACCENT.2, GLOW.2),
            )))
        }
        ColorDepth::Ansi16 => Style::new().fg(Color::LightBlue),
        ColorDepth::None => Style::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::theme::Glyphs;
    use ratatui::{Terminal, backend::TestBackend};
    use unicode_width::UnicodeWidthStr;

    fn theme(depth: ColorDepth, glyphs: Glyphs) -> Theme {
        Theme {
            depth,
            glyphs,
            motion: false,
        }
    }

    fn plain(l: &Line) -> String {
        l.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn long_text_ends_in_an_ellipsis_at_the_column() {
        assert_eq!(ellipsize("hello world", 8), "hello w…");
        assert_eq!(ellipsize("short", 8), "short");
        assert_eq!(
            ellipsize("hello", 5),
            "hello",
            "exactly fitting text is kept"
        );
        assert_eq!(ellipsize("hello", 0), "");
        assert_eq!(ellipsize_ascii("hello world", 8), "hello...");
        assert_eq!(
            ellipsize_ascii("12 cores · 7 GiB", 40),
            "12 cores - 7 GiB",
            "no non-ASCII on a plain terminal"
        );
        assert_eq!(ellipsize_ascii("日本", 10), "??");
    }

    #[test]
    fn a_long_path_keeps_its_end_and_never_exceeds_the_width() {
        assert_eq!(
            ellipsize_start("/very/long/path/igris.toml", 14, "…"),
            "…th/igris.toml"
        );
        assert_eq!(ellipsize_start("/short", 14, "…"), "/short");
        assert_eq!(ellipsize_start("/a/b/c.toml", 0, "…"), "");
        for w in 0..16 {
            assert!(
                ellipsize_start("/tmp/日本語/igris.toml", w, "…").width() <= w,
                "{w}"
            );
        }
        assert!(
            !ellipsize_start("/a\x1b[2J/b", 20, "…")
                .chars()
                .any(char::is_control)
        );
    }

    #[test]
    fn a_wide_character_is_never_split_and_the_width_is_never_exceeded() {
        for w in 0..=14 {
            let s = ellipsize("日本語テキストです", w);
            assert!(s.width() <= w, "{w}: {s:?} is {} wide", s.width());
        }
        assert_eq!(
            ellipsize("日本語テキスト", 7),
            "日本語…",
            "three wide characters and the dots"
        );
    }

    #[test]
    fn control_characters_never_survive() {
        let s = ellipsize("a\x1b[2Jb\x07c\r\nd", 20);
        assert_eq!(s, "a[2Jbcd");
        assert!(
            !ellipsize(&"x\x1b]52;c;aGk=\x07".repeat(40), 30)
                .chars()
                .any(char::is_control)
        );
    }

    #[test]
    fn the_progress_bar_fills_from_the_left_and_is_exactly_as_wide_as_asked() {
        let t = theme(ColorDepth::True, Glyphs::Unicode);
        assert_eq!(
            plain(&progress(&t, 20, Some(0.5), 0)),
            "██████████░░░░░░░░░░"
        );
        assert_eq!(plain(&progress(&t, 8, Some(0.0), 0)), "░░░░░░░░");
        assert_eq!(plain(&progress(&t, 8, Some(1.0), 0)), "████████");
        assert_eq!(
            plain(&progress(&t, 8, Some(7.0), 0)),
            "████████",
            "over 100% is clamped"
        );
        let a = theme(ColorDepth::None, Glyphs::Ascii);
        assert_eq!(plain(&progress(&a, 6, Some(0.5), 0)), "###---");
    }

    #[test]
    fn the_unknown_length_bar_sweeps_and_stays_inside() {
        let t = theme(ColorDepth::True, Glyphs::Unicode);
        let at = |tick| plain(&progress(&t, 20, None, tick));
        assert_ne!(at(0), at(5));
        for tick in 0..100 {
            let s = at(tick);
            assert_eq!(s.chars().count(), 20, "tick {tick}");
            assert_eq!(
                s.chars().filter(|c| *c == '█').count(),
                6,
                "tick {tick}: the window keeps its size"
            );
        }
        assert_eq!(at(0), at(28), "it bounces: the cycle is 2 x (20 - 6)");
    }

    #[test]
    fn the_filled_part_runs_from_the_accent_blue_to_the_glow() {
        let t = theme(ColorDepth::True, Glyphs::Unicode);
        let l = progress(&t, 10, Some(1.0), 0);
        assert_eq!(l.spans[0].style.fg, Some(Color::Rgb(0x08, 0x79, 0xf3)));
        assert_eq!(l.spans[9].style.fg, Some(Color::Rgb(0xbc, 0xfa, 0xf1)));
        let none = progress(&theme(ColorDepth::None, Glyphs::Unicode), 4, Some(1.0), 0);
        assert!(none.spans.iter().all(|s| s.style.fg.is_none()));
    }

    #[test]
    fn the_panel_is_rounded_with_the_title_in_the_border_and_plain_ascii_when_needed() {
        let draw = |t: &Theme| {
            let mut term = Terminal::new(TestBackend::new(20, 4)).unwrap();
            term.draw(|f| f.render_widget(panel(t, "Check", true), f.area()))
                .unwrap();
            let b = term.backend().buffer().clone();
            (0..20)
                .map(|x| b[(x, 0)].symbol().to_string())
                .collect::<String>()
        };
        let top = draw(&theme(ColorDepth::True, Glyphs::Unicode));
        assert!(
            top.starts_with('╭') && top.contains(" Check ") && top.ends_with('╮'),
            "{top}"
        );
        let ascii = draw(&theme(ColorDepth::None, Glyphs::Ascii));
        assert!(
            ascii.starts_with('+') && ascii.contains(" Check ") && ascii.ends_with('+'),
            "{ascii}"
        );
        assert!(ascii.is_ascii());
    }

    #[test]
    fn a_focused_panel_has_the_blue_border_and_an_unfocused_one_the_quiet_border() {
        let t = theme(ColorDepth::True, Glyphs::Unicode);
        let fg = |focused| {
            let mut term = Terminal::new(TestBackend::new(10, 3)).unwrap();
            term.draw(|f| f.render_widget(panel(&t, "x", focused), f.area()))
                .unwrap();
            term.backend().buffer()[(0, 0)].fg
        };
        assert_eq!(fg(true), Color::Rgb(0x53, 0x9f, 0xdf));
        assert_eq!(fg(false), Color::Rgb(0x3a, 0x46, 0x50));
    }

    #[test]
    fn a_chip_has_a_glyph_a_count_and_a_word() {
        let t = theme(ColorDepth::True, Glyphs::Unicode);
        let c: String = chip(&t, Status::NeedsStep, 2)
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert_eq!(c, "● 2 needs a step");
        let a = theme(ColorDepth::None, Glyphs::Ascii);
        let c: String = chip(&a, Status::Ready, 4)
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert_eq!(c, "[x] 4 ready");
        assert_eq!(
            chip(&t, Status::NeedsStep, 1)[0].style.fg,
            Some(Color::Rgb(0xff, 0x30, 0x38))
        );
    }
}
