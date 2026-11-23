//! The colour roles of the IGRIS palette. Screens ask for a role, never for a colour, so truecolor,
//! 16 colours and no colour all stay in step. Colour supports the hierarchy and never carries it.

use super::theme::{ColorDepth, Theme};
use ratatui::style::{Color, Modifier, Style};

pub const RED: (u8, u8, u8) = (0xff, 0x30, 0x38);
pub const LIGHT_BLUE: (u8, u8, u8) = (0x53, 0x9f, 0xdf);
pub const ACCENT: (u8, u8, u8) = (0x08, 0x79, 0xf3);
pub const GLOW: (u8, u8, u8) = (0xbc, 0xfa, 0xf1);
const DIM: (u8, u8, u8) = (0x7d, 0x87, 0x90);
const BORDER: (u8, u8, u8) = (0x3a, 0x46, 0x50);
const HOVER: (u8, u8, u8) = (0x16, 0x22, 0x2b);
const INK: (u8, u8, u8) = (0x0b, 0x11, 0x14);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Text,
    /// Secondary text: details, labels in hints.
    Dim,
    Link,
    /// Ready, done, added.
    Ok,
    /// Your action: needs a step, the current step, removed.
    Bad,
    Optional,
    Info,
    Border,
    BorderFocus,
    /// A key in a hint.
    Key,
    Hover,
    /// The primary button.
    Primary,
}

pub fn rgb((r, g, b): (u8, u8, u8)) -> Color {
    Color::Rgb(r, g, b)
}

impl Theme {
    pub fn style(&self, role: Role) -> Style {
        let fg = |c| Style::new().fg(rgb(c));
        let dim = Style::new().add_modifier(Modifier::DIM);
        match (self.depth, role) {
            (_, Role::Text) => Style::new(),
            (ColorDepth::True, Role::Dim | Role::Optional) => fg(DIM),
            (_, Role::Dim | Role::Optional) => dim,
            (ColorDepth::True, Role::Link | Role::Key | Role::Info | Role::BorderFocus) => {
                fg(LIGHT_BLUE)
            }
            (ColorDepth::Ansi16, Role::Link | Role::Key | Role::Info | Role::BorderFocus) => {
                Style::new().fg(Color::LightBlue)
            }
            (ColorDepth::None, Role::Link | Role::Key | Role::Info | Role::BorderFocus) => {
                Style::new()
            }
            (ColorDepth::True, Role::Ok) => fg(GLOW),
            (ColorDepth::Ansi16, Role::Ok) => Style::new().fg(Color::Cyan),
            (ColorDepth::None, Role::Ok) => Style::new(),
            (ColorDepth::True, Role::Bad) => fg(RED),
            (ColorDepth::Ansi16, Role::Bad) => Style::new().fg(Color::Red),
            (ColorDepth::None, Role::Bad) => Style::new().add_modifier(Modifier::BOLD),
            (ColorDepth::True, Role::Border) => fg(BORDER),
            (_, Role::Border) => dim,
            (ColorDepth::True, Role::Hover) => Style::new().bg(rgb(HOVER)),
            (_, Role::Hover) => Style::new().add_modifier(Modifier::BOLD),
            (ColorDepth::True, Role::Primary) => Style::new()
                .fg(rgb(INK))
                .bg(rgb(RED))
                .add_modifier(Modifier::BOLD),
            (ColorDepth::Ansi16, Role::Primary) => Style::new()
                .fg(Color::Black)
                .bg(Color::Red)
                .add_modifier(Modifier::BOLD),
            (ColorDepth::None, Role::Primary) => {
                Style::new().add_modifier(Modifier::REVERSED | Modifier::BOLD)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::theme::Glyphs;

    fn t(depth: ColorDepth) -> Theme {
        Theme {
            depth,
            glyphs: Glyphs::Unicode,
            motion: false,
        }
    }

    #[test]
    fn needs_a_step_is_the_plume_red_in_truecolor_red_in_16_colours_and_bold_without_colour() {
        assert_eq!(
            t(ColorDepth::True).style(Role::Bad).fg,
            Some(Color::Rgb(0xff, 0x30, 0x38))
        );
        assert_eq!(t(ColorDepth::Ansi16).style(Role::Bad).fg, Some(Color::Red));
        let none = t(ColorDepth::None).style(Role::Bad);
        assert_eq!(none.fg, None);
        assert!(
            none.add_modifier.contains(Modifier::BOLD),
            "still stands out"
        );
    }

    #[test]
    fn secondary_text_is_the_dim_tone_or_the_dim_attribute() {
        assert_eq!(
            t(ColorDepth::True).style(Role::Dim).fg,
            Some(Color::Rgb(0x7d, 0x87, 0x90))
        );
        for d in [ColorDepth::Ansi16, ColorDepth::None] {
            let s = t(d).style(Role::Dim);
            assert_eq!(s.fg, None);
            assert!(s.add_modifier.contains(Modifier::DIM));
        }
    }

    #[test]
    fn no_colour_means_no_colour_on_any_role() {
        for role in [
            Role::Text,
            Role::Dim,
            Role::Link,
            Role::Ok,
            Role::Bad,
            Role::Optional,
            Role::Info,
            Role::Border,
            Role::BorderFocus,
            Role::Key,
            Role::Hover,
            Role::Primary,
        ] {
            let s = t(ColorDepth::None).style(role);
            assert!(s.fg.is_none() && s.bg.is_none(), "{role:?}: {s:?}");
        }
    }

    #[test]
    fn the_primary_button_is_red_and_hover_only_tints_in_truecolor() {
        assert_eq!(
            t(ColorDepth::True).style(Role::Primary).bg,
            Some(Color::Rgb(0xff, 0x30, 0x38))
        );
        assert_eq!(
            t(ColorDepth::True).style(Role::Hover).bg,
            Some(Color::Rgb(0x16, 0x22, 0x2b))
        );
        assert_eq!(t(ColorDepth::Ansi16).style(Role::Hover).bg, None);
    }

    #[test]
    fn dim_text_keeps_enough_contrast_on_the_palette_background() {
        // WCAG relative luminance of #7d8790 against #0b1114: must be at least 4.5:1.
        let lum = |(r, g, b): (u8, u8, u8)| {
            let c = |v: u8| {
                let v = f64::from(v) / 255.0;
                if v <= 0.03928 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * c(r) + 0.7152 * c(g) + 0.0722 * c(b)
        };
        let ratio = (lum(DIM) + 0.05) / (lum(INK) + 0.05);
        assert!(ratio >= 4.5, "{ratio}");
    }
}
