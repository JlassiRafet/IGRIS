//! The IGRIS wordmark: block-pixel letters with a gradient and a red plume above the G.
use super::theme::{ColorDepth, Theme};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

pub const PLUME: &str = "    ▀▄";
pub const ROWS: [&str; 2] = ["▀█▀ █▀▀ █▀█ ▀█▀ █▀▀", "▄█▄ █▄█ █▀▄ ▄█▄ ▄▄█"];
pub const WIDTH: u16 = 19;
use super::style::{ACCENT, GLOW, RED};
/// The bright band of the sheen.
const SHEEN: (u8, u8, u8) = (0xf2, 0xff, 0xfd);
/// One sweep takes 35 ticks (3.5 s at 100 ms), then the wordmark rests for 10.
const CYCLE: usize = 45;
const SWEEP: f32 = 35.0;

fn rgb((r, g, b): (u8, u8, u8)) -> Color {
    Color::Rgb(r, g, b)
}

/// How bright the sheen is at a column: 1.0 at its centre, 0.0 four columns away or when resting.
fn sheen(col: usize, tick: usize) -> f32 {
    let centre = (tick % CYCLE) as f32 * (WIDTH as f32 + 8.0) / SWEEP - 4.0;
    (1.0 - (col as f32 - centre).abs() / 4.0).max(0.0)
}

/// Colour of one cell: `t` runs 0.0 (glow) to 1.0 (accent) from the top left to the bottom right.
fn letter(theme: &Theme, t: f32, col: usize, tick: usize) -> Style {
    match theme.depth {
        ColorDepth::True => {
            let mix = |a: u8, b: u8, k: f32| (a as f32 + (b as f32 - a as f32) * k).round() as u8;
            let base = (
                mix(GLOW.0, ACCENT.0, t),
                mix(GLOW.1, ACCENT.1, t),
                mix(GLOW.2, ACCENT.2, t),
            );
            let k = if theme.motion {
                0.85 * sheen(col, tick)
            } else {
                0.0
            };
            Style::new().fg(rgb((
                mix(base.0, SHEEN.0, k),
                mix(base.1, SHEEN.1, k),
                mix(base.2, SHEEN.2, k),
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

/// `tick` counts 100 ms steps; with `theme.motion` a bright band sweeps over the letters.
pub fn wordmark(theme: &Theme, tick: usize) -> Vec<Line<'static>> {
    let mut lines = vec![row(PLUME, |_| plume(theme))];
    for (r, text) in ROWS.iter().enumerate() {
        let last = (WIDTH - 1) as f32;
        lines.push(row(text, |c| {
            letter(theme, (c as f32 / last + r as f32) / 2.0, c, tick)
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
            motion: false,
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
                text(&wordmark(&theme(depth), 0)),
                vec!["    ▀▄", "▀█▀ █▀▀ █▀█ ▀█▀ █▀▀", "▄█▄ █▄█ █▀▄ ▄█▄ ▄▄█"]
            );
        }
    }

    #[test]
    fn truecolor_has_a_red_plume_and_a_glow_to_blue_gradient() {
        let lines = wordmark(&theme(ColorDepth::True), 0);
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
        let lines = wordmark(&theme(ColorDepth::Ansi16), 0);
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
        for line in wordmark(&theme(ColorDepth::None), 0) {
            assert!(
                line.spans
                    .iter()
                    .all(|s| fg_of(s).is_none() && s.style == Style::new())
            );
        }
    }

    fn moving(motion: bool) -> Theme {
        Theme {
            depth: ColorDepth::True,
            glyphs: Glyphs::Unicode,
            motion,
        }
    }

    #[test]
    fn the_sheen_moves_the_colours_but_not_the_letters_or_the_plume() {
        let (a, b) = (wordmark(&moving(true), 0), wordmark(&moving(true), 12));
        assert_eq!(text(&a), text(&b));
        assert_ne!(a, b, "a bright band is somewhere else at tick 12");
        for tick in [0, 12, 30] {
            let lines = wordmark(&moving(true), tick);
            assert!(
                cells(&lines[0])
                    .iter()
                    .all(|c| fg_of(c) == Some(Color::Rgb(0xff, 0x30, 0x38)))
            );
        }
    }

    #[test]
    fn the_sheen_is_brighter_than_the_gradient_it_passes_over() {
        let still = wordmark(&moving(false), 0);
        let lit = wordmark(&moving(true), 12);
        let sum = |c: &Span| match fg_of(c) {
            Some(Color::Rgb(r, g, b)) => u32::from(r) + u32::from(g) + u32::from(b),
            _ => 0,
        };
        let (s, l): (u32, u32) = (
            still[1..].iter().flat_map(cells).map(sum).sum(),
            lit[1..].iter().flat_map(cells).map(sum).sum(),
        );
        assert!(l > s, "{l} vs {s}");
    }

    #[test]
    fn without_motion_every_tick_looks_the_same_and_the_sweep_rests() {
        assert_eq!(wordmark(&moving(false), 0), wordmark(&moving(false), 17));
        // Ticks 36 to 44 are the rest: the band is off the wordmark, so it equals the plain gradient.
        assert_eq!(wordmark(&moving(true), 40), wordmark(&moving(false), 40));
    }
}
