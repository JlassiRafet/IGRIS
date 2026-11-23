//! The IGRIS wordmark: block-pixel letters with a gradient and a red plume above the G.
use super::theme::{ColorDepth, Theme};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

pub const PLUME: &str = "    ▀▄";
pub const ROWS: [&str; 2] = ["▀█▀ █▀▀ █▀█ ▀█▀ █▀▀", "▄█▄ █▄█ █▀▄ ▄█▄ ▄▄█"];
pub const WIDTH: u16 = 19;
const GLOW: (u8, u8, u8) = (0xbc, 0xfa, 0xf1);
const ACCENT: (u8, u8, u8) = (0x08, 0x79, 0xf3);
const RED: (u8, u8, u8) = (0xff, 0x30, 0x38);

fn rgb((r, g, b): (u8, u8, u8)) -> Color {
    Color::Rgb(r, g, b)
}

/// Colour of one cell: `t` runs 0.0 (glow) to 1.0 (accent) from the top left to the bottom right.
fn letter(theme: &Theme, t: f32) -> Style {
    match theme.depth {
        ColorDepth::True => {
            let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
            Style::new().fg(rgb((
                mix(GLOW.0, ACCENT.0),
                mix(GLOW.1, ACCENT.1),
                mix(GLOW.2, ACCENT.2),
            )))
        }
        ColorDepth::Ansi16 => Style::new().fg(Color::Cyan),
        ColorDepth::None => Style::new(),
    }
}

fn plume(theme: &Theme) -> Style {
    match theme.depth {
        ColorDepth::True => Style::new().fg(rgb(RED)),
        ColorDepth::Ansi16 => Style::new().fg(Color::Red),
        ColorDepth::None => Style::new(),
    }
}

fn row(text: &'static str, style: impl Fn(usize) -> Style) -> Line<'static> {
    Line::from(
        text.chars()
            .enumerate()
            .map(|(i, c)| {
                if c == ' ' {
                    Span::raw(" ")
                } else {
                    Span::styled(c.to_string(), style(i))
                }
            })
            .collect::<Vec<_>>(),
    )
}

pub fn wordmark(theme: &Theme) -> Vec<Line<'static>> {
    let mut lines = vec![row(PLUME, |_| plume(theme))];
    for (r, text) in ROWS.iter().enumerate() {
        let last = (WIDTH - 1) as f32;
        lines.push(row(text, |c| {
            letter(theme, (c as f32 / last + r as f32) / 2.0)
        }));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::theme::Glyphs;

    fn theme(depth: ColorDepth) -> Theme {
        Theme {
            depth,
            glyphs: Glyphs::Unicode,
        }
    }

    fn text(lines: &[Line]) -> Vec<String> {
        lines
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect()
    }

    fn fg_of(span: &Span) -> Option<Color> {
        span.style.fg
    }

    fn cells<'a>(line: &'a Line<'static>) -> Vec<&'a Span<'static>> {
        line.spans
            .iter()
            .filter(|s| s.content.trim() != "")
            .collect()
    }

    #[test]
    fn the_plain_text_is_exactly_the_approved_wordmark() {
        for depth in [ColorDepth::True, ColorDepth::Ansi16, ColorDepth::None] {
            assert_eq!(
                text(&wordmark(&theme(depth))),
                vec!["    ▀▄", "▀█▀ █▀▀ █▀█ ▀█▀ █▀▀", "▄█▄ █▄█ █▀▄ ▄█▄ ▄▄█"]
            );
        }
    }

    #[test]
    fn truecolor_has_a_red_plume_and_a_glow_to_blue_gradient() {
        let lines = wordmark(&theme(ColorDepth::True));
        for c in cells(&lines[0]) {
            assert_eq!(fg_of(c), Some(Color::Rgb(0xff, 0x30, 0x38)), "plume");
        }
        assert_eq!(
            fg_of(cells(&lines[1])[0]),
            Some(Color::Rgb(0xbc, 0xfa, 0xf1)),
            "first letter cell"
        );
        assert_eq!(
            fg_of(cells(&lines[2]).last().unwrap()),
            Some(Color::Rgb(0x08, 0x79, 0xf3)),
            "last letter cell"
        );
        let bg = (0x0b_i32, 0x11_i32, 0x14_i32);
        let dist = |c: (u8, u8, u8)| {
            let (r, g, b) = (c.0 as i32 - bg.0, c.1 as i32 - bg.1, c.2 as i32 - bg.2);
            r * r + g * g + b * b
        };
        let floor = dist((0x08, 0x79, 0xf3));
        for line in &lines[1..] {
            for c in cells(line) {
                let Some(Color::Rgb(r, g, b)) = fg_of(c) else {
                    panic!("no colour on {c:?}")
                };
                assert!(
                    dist((r, g, b)) >= floor,
                    "{r},{g},{b} is too close to the background"
                );
            }
        }
    }

    #[test]
    fn sixteen_colours_gives_solid_letters_and_a_red_plume() {
        let lines = wordmark(&theme(ColorDepth::Ansi16));
        assert!(
            cells(&lines[0])
                .iter()
                .all(|c| fg_of(c) == Some(Color::Red))
        );
        let mut letters: Vec<_> = lines[1..].iter().flat_map(cells).map(fg_of).collect();
        letters.dedup();
        assert_eq!(letters.len(), 1, "one solid colour: {letters:?}");
        assert_ne!(letters[0], Some(Color::Red));
    }

    #[test]
    fn no_color_means_no_colour_on_any_cell() {
        for line in wordmark(&theme(ColorDepth::None)) {
            assert!(
                line.spans
                    .iter()
                    .all(|s| fg_of(s).is_none() && s.style == Style::new())
            );
        }
    }
}
