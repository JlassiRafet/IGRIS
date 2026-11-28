//! The colour roles of the IGRIS palette. Screens ask for a role, never for a colour, so truecolor,
//! 16 colours and no colour all stay in step. Colour supports the hierarchy and never carries it.

use super::theme::{ColorDepth, Theme};
use ratatui::style::{Color, Modifier, Style};

pub const RED: (u8, u8, u8) = (0xff, 0x30, 0x38);
pub const LIGHT_BLUE: (u8, u8, u8) = (0x53, 0x9f, 0xdf);
pub const ACCENT: (u8, u8, u8) = (0x08, 0x79, 0xf3);
pub const GUIDE: (u8, u8, u8) = (0x7f, 0x8c, 0xf0);
pub const GUIDE_GLOW: (u8, u8, u8) = (0xc4, 0xcb, 0xfa);
pub const GLOW: (u8, u8, u8) = (0xbc, 0xfa, 0xf1);
const DIM: (u8, u8, u8) = (0x7d, 0x87, 0x90);
const BORDER: (u8, u8, u8) = (0x3a, 0x46, 0x50);
const HOVER: (u8, u8, u8) = (0x16, 0x22, 0x2b);
const INK: (u8, u8, u8) = (0x0b, 0x11, 0x14);
/// The bright band of a sheen (wordmark and progress bar).
pub const SHEEN: (u8, u8, u8) = (0xf2, 0xff, 0xfd);

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
    /// The primary button: dark text on the light blue.
    Primary,
    /// The one call to action of a chain question: bold light-blue text, no rectangle. The chain
    /// draws a glint over it when motion is on.
    Cta,
    /// The guidance box (what to do next): periwinkle from the indigo shadows of the reference art,
    /// apart from the blue of keys and the call to action.
    Guide,
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
                .bg(rgb(LIGHT_BLUE))
                .add_modifier(Modifier::BOLD),
            (ColorDepth::Ansi16, Role::Primary) => Style::new()
                .fg(Color::Black)
                .bg(Color::LightBlue)
                .add_modifier(Modifier::BOLD),
            (ColorDepth::None, Role::Primary) => {
                Style::new().add_modifier(Modifier::REVERSED | Modifier::BOLD)
            }
            (ColorDepth::True, Role::Cta) => fg(LIGHT_BLUE).add_modifier(Modifier::BOLD),
            (ColorDepth::Ansi16, Role::Cta) => Style::new()
                .fg(Color::LightBlue)
                .add_modifier(Modifier::BOLD),
            (ColorDepth::None, Role::Cta) => {
                Style::new().add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
            }
            (ColorDepth::True, Role::Guide) => fg(GUIDE),
            (ColorDepth::Ansi16, Role::Guide) => Style::new().fg(Color::LightMagenta),
            (ColorDepth::None, Role::Guide) => Style::new().add_modifier(Modifier::BOLD),
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
    fn the_guidance_box_is_periwinkle_light_magenta_in_16_colours_and_bold_without_colour() {
        assert_eq!(
            t(ColorDepth::True).style(Role::Guide).fg,
            Some(Color::Rgb(0x7f, 0x8c, 0xf0))
        );
        assert_eq!(
            t(ColorDepth::Ansi16).style(Role::Guide).fg,
            Some(Color::LightMagenta)
        );
        let none = t(ColorDepth::None).style(Role::Guide);
        assert_eq!(none.fg, None);
        assert!(none.add_modifier.contains(Modifier::BOLD));
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
            Role::Cta,
        ] {
            let s = t(ColorDepth::None).style(role);
            assert!(s.fg.is_none() && s.bg.is_none(), "{role:?}: {s:?}");
        }
    }

    #[test]
    fn the_primary_button_is_dark_text_on_the_light_blue_and_hover_only_tints_in_truecolor() {
        let s = t(ColorDepth::True).style(Role::Primary);
        assert_eq!(s.bg, Some(Color::Rgb(0x53, 0x9f, 0xdf)));
        assert_eq!(s.fg, Some(Color::Rgb(0x0b, 0x11, 0x14)));
        assert!(s.add_modifier.contains(Modifier::BOLD));
        let s = t(ColorDepth::Ansi16).style(Role::Primary);
        assert_eq!((s.fg, s.bg), (Some(Color::Black), Some(Color::LightBlue)));
        assert!(
            t(ColorDepth::None)
                .style(Role::Primary)
                .add_modifier
                .contains(Modifier::REVERSED | Modifier::BOLD)
        );
        assert_eq!(
            t(ColorDepth::True).style(Role::Hover).bg,
            Some(Color::Rgb(0x16, 0x22, 0x2b))
        );
        assert_eq!(t(ColorDepth::Ansi16).style(Role::Hover).bg, None);
    }

    #[test]
    fn the_call_to_action_is_bold_blue_text_with_no_rectangle_and_underlined_without_colour() {
        let s = t(ColorDepth::True).style(Role::Cta);
        assert_eq!(s.fg, Some(Color::Rgb(0x53, 0x9f, 0xdf)));
        assert_eq!(s.bg, None);
        assert!(s.add_modifier.contains(Modifier::BOLD));
        let s = t(ColorDepth::Ansi16).style(Role::Cta);
        assert_eq!((s.fg, s.bg), (Some(Color::LightBlue), None));
        assert!(s.add_modifier.contains(Modifier::BOLD));
        let s = t(ColorDepth::None).style(Role::Cta);
        assert!(
            s.add_modifier
                .contains(Modifier::BOLD | Modifier::UNDERLINED)
        );
    }

    #[test]
    fn blue_text_and_the_dark_text_on_a_blue_button_both_have_enough_contrast() {
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
        let ratio = (lum(LIGHT_BLUE) + 0.05) / (lum(INK) + 0.05);
        assert!(ratio >= 4.5, "{ratio}");
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
