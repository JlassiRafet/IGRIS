//! What this terminal can show: colour depth and glyph set, decided once from the environment.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorDepth {
    True,
    Ansi16,
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Glyphs {
    Unicode,
    Ascii,
}

/// The environment variables that matter, passed in so tests need no process state.
#[derive(Clone, Debug, Default)]
pub struct Env {
    pub colorterm: Option<String>,
    pub no_color: bool,
    /// `IGRIS_NO_MOTION` is set: nothing animates.
    pub no_motion: bool,
    pub term: Option<String>,
    pub lang: Option<String>,
}

impl Env {
    pub fn from_process() -> Env {
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        Env {
            colorterm: var("COLORTERM"),
            // no-color.org: any non-empty value turns colour off.
            no_color: var("NO_COLOR").is_some(),
            no_motion: var("IGRIS_NO_MOTION").is_some(),
            term: var("TERM"),
            lang: var("LC_ALL")
                .or_else(|| var("LC_CTYPE"))
                .or_else(|| var("LANG")),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Theme {
    pub depth: ColorDepth,
    pub glyphs: Glyphs,
    /// Decorative animation (the wordmark sheen) is on: truecolor and not switched off.
    pub motion: bool,
}

impl Theme {
    pub fn detect(env: &Env) -> Theme {
        let depth = if env.no_color {
            ColorDepth::None
        } else if matches!(env.colorterm.as_deref(), Some("truecolor" | "24bit")) {
            ColorDepth::True
        } else {
            ColorDepth::Ansi16
        };
        // An unset locale is assumed to be UTF-8; "C", "POSIX" and Latin-1 are not.
        let utf8 = env.lang.as_deref().is_none_or(|l| {
            let l = l.to_ascii_lowercase();
            l.contains("utf-8") || l.contains("utf8")
        });
        let glyphs = if env.term.as_deref() == Some("linux") || !utf8 {
            Glyphs::Ascii
        } else {
            Glyphs::Unicode
        };
        let motion = depth == ColorDepth::True && !env.no_motion;
        Theme {
            depth,
            glyphs,
            motion,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(colorterm: Option<&str>, no_color: bool, term: Option<&str>, lang: Option<&str>) -> Env {
        Env {
            colorterm: colorterm.map(Into::into),
            no_color,
            no_motion: false,
            term: term.map(Into::into),
            lang: lang.map(Into::into),
        }
    }

    #[test]
    fn truecolor_needs_the_colorterm_hint() {
        let t = Theme::detect(&env(
            Some("truecolor"),
            false,
            Some("xterm-256color"),
            Some("en_US.UTF-8"),
        ));
        assert_eq!(t.depth, ColorDepth::True);
        let t = Theme::detect(&env(Some("24bit"), false, None, None));
        assert_eq!(t.depth, ColorDepth::True);
        let t = Theme::detect(&env(None, false, Some("xterm-256color"), None));
        assert_eq!(
            t.depth,
            ColorDepth::Ansi16,
            "256 colours alone is not truecolor"
        );
    }

    #[test]
    fn no_color_wins_over_truecolor() {
        let t = Theme::detect(&env(
            Some("truecolor"),
            true,
            Some("xterm"),
            Some("en_US.UTF-8"),
        ));
        assert_eq!(t.depth, ColorDepth::None);
    }

    #[test]
    fn ascii_for_the_linux_console_and_non_utf8_locales() {
        assert_eq!(
            Theme::detect(&env(None, false, Some("linux"), Some("en_US.UTF-8"))).glyphs,
            Glyphs::Ascii
        );
        assert_eq!(
            Theme::detect(&env(None, false, Some("xterm"), Some("C"))).glyphs,
            Glyphs::Ascii
        );
        assert_eq!(
            Theme::detect(&env(None, false, Some("xterm"), Some("en_US.ISO-8859-1"))).glyphs,
            Glyphs::Ascii
        );
        assert_eq!(
            Theme::detect(&env(None, false, Some("xterm"), Some("de_DE.utf8"))).glyphs,
            Glyphs::Unicode
        );
        assert_eq!(
            Theme::detect(&env(None, false, Some("xterm"), None)).glyphs,
            Glyphs::Unicode
        );
    }
}
