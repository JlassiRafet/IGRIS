//! The chain screen: header band, history of answered questions, the active question with its
//! hint right under the options, and a one-line footer. Pure drawing of a `ChainUi`.
use super::model::{ChainUi, Choice, Mark, Step};
use crate::tui::style::Role;
use crate::tui::theme::{Glyphs, Theme};
use crate::tui::{header, layout, widgets};
use igris_detect::Snapshot;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

/// The marks of one terminal kind: Unicode, or plain ASCII when the terminal cannot show it.
struct Marks {
    done: &'static str,
    active: &'static str,
    rail: &'static str,
    close: &'static str,
    on: &'static str,
    off: &'static str,
    warn: &'static str,
    hint: &'static str,
}

fn marks(t: &Theme) -> Marks {
    if t.glyphs == Glyphs::Ascii {
        Marks {
            done: "o",
            active: "*",
            rail: "|",
            close: "`",
            on: "(*)",
            off: "( )",
            warn: "!",
            hint: "up/down choose - Enter confirm - Left back - r re-scan",
        }
    } else {
        Marks {
            done: "◇",
            active: "◆",
            rail: "│",
            close: "└",
            on: "●",
            off: "○",
            warn: "⚠",
            hint: "↑/↓ choose · Enter confirm · ← back · r re-scan",
        }
    }
}

const QUESTION_ROWS: usize = 4; // the blank line above, the question, the hint, the closing mark
const BOX_ROWS: u16 = 5;

pub fn draw(f: &mut Frame, area: Rect, ui: &ChainUi, snap: &Snapshot, t: &Theme, tick: usize) {
    let big = area.height >= layout::TALL_HEIGHT && t.glyphs == Glyphs::Unicode;
    let a = layout::compute(area, big);
    header::draw_band(f, a.band, t, tick, big);
    let rule = if t.glyphs == Glyphs::Ascii {
        "-"
    } else {
        "─"
    };
    f.render_widget(
        Paragraph::new(Line::styled(
            rule.repeat(a.rule.width as usize),
            t.style(Role::Border),
        )),
        a.rule,
    );
    let top = a.rule.y + 1;
    let body = Rect::new(a.band.x, top, a.band.width, a.footer.y.saturating_sub(top));
    let m = marks(t);
    let blocker = ui.blocker(snap);
    let history = ui.history(snap);
    let mut lines: Vec<Line> = history
        .iter()
        .map(|(label, answer, state)| {
            Line::from(vec![
                Span::styled(format!("{} ", m.done), t.style(Role::Dim)),
                Span::styled(format!("{label:<10}"), t.style(Role::Text)),
                Span::styled(format!("{answer:<26}"), t.style(Role::Text)),
                Span::styled(state.clone(), t.style(Role::Dim)),
            ])
        })
        .collect();
    let options = ui.choices(snap);
    let box_rows = if blocker.is_some() { BOX_ROWS } else { 0 };
    let used = lines.len() + QUESTION_ROWS + usize::from(box_rows);
    let room = (body.height as usize).saturating_sub(used).max(1);
    match ui.step {
        Step::End => lines.push(Line::styled(
            "Slice 1 ends here: the next questions come in the next release candidate.",
            t.style(Role::Dim),
        )),
        step => {
            lines.push(Line::raw(""));
            lines.push(Line::from(vec![
                Span::styled(format!("{} ", m.active), t.style(Role::Info)),
                Span::styled(question(step), t.style(Role::Text)),
            ]));
            let start = (ui.cursor + 1).saturating_sub(room);
            for (i, c) in options.iter().enumerate().skip(start).take(room) {
                lines.push(option_line(c, i == ui.cursor, &m, t));
            }
            let keys = if options.is_empty() {
                "r re-scan"
            } else {
                m.hint
            };
            lines.push(Line::from(vec![
                Span::styled(format!("{}  ", m.rail), t.style(Role::Dim)),
                Span::styled(keys, t.style(Role::Dim)),
            ]));
            lines.push(Line::styled(m.close, t.style(Role::Dim)));
        }
    }
    let n = lines.len() as u16;
    f.render_widget(Paragraph::new(lines), body);
    if let Some(text) = blocker {
        let y = body.y + n.min(body.height.saturating_sub(box_rows));
        let r = Rect::new(body.x, y, body.width, box_rows.min(body.height));
        let block = widgets::panel(t, "Needs a step", true);
        f.render_widget(
            Paragraph::new(text).wrap(Wrap { trim: true }).block(block),
            r,
        );
    }
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("q", t.style(Role::Key)),
            Span::styled(" quit", t.style(Role::Dim)),
        ])),
        a.footer,
    );
}

fn question(step: Step) -> &'static str {
    match step {
        Step::Workspace => "Workspace: Herdr runs your agents side by side",
        Step::Builder => "Builder: which agent writes the code?",
        Step::Reviewer => "Reviewer: who checks the builder's work?",
        Step::End => "",
    }
}

fn option_line(c: &Choice, selected: bool, m: &Marks, t: &Theme) -> Line<'static> {
    let (note_role, label_role) = match c.mark {
        Mark::Ready => (Role::Ok, Role::Text),
        Mark::Warn => (Role::Bad, Role::Text),
        Mark::Dim => (Role::Dim, Role::Dim),
    };
    let mut spans = vec![
        Span::styled(format!("{}  ", m.rail), t.style(Role::Dim)),
        Span::styled(
            format!("{} ", if selected { m.on } else { m.off }),
            t.style(if selected { Role::Info } else { Role::Dim }),
        ),
        Span::styled(format!("{:<14}", c.label), t.style(label_role)),
        Span::styled(c.note.clone(), t.style(note_role)),
    ];
    if let (true, Some(w)) = (selected, &c.warn) {
        spans.push(Span::styled(
            format!("  {} {w}", m.warn),
            t.style(Role::Bad),
        ));
    }
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::tool;
    use crate::tui::theme::ColorDepth;
    use crate::tui::tools_screen::tests::snap;
    use igris_detect::Login::LoggedIn;
    use ratatui::{Terminal, backend::TestBackend};

    fn theme(glyphs: Glyphs) -> Theme {
        Theme {
            depth: ColorDepth::None,
            glyphs,
            motion: false,
        }
    }

    fn render(ui: &ChainUi, s: &Snapshot, glyphs: Glyphs, w: u16, h: u16) -> Vec<String> {
        let t = theme(glyphs);
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        term.draw(|f| draw(f, f.area(), ui, s, &t, 0)).unwrap();
        let b = term.backend().buffer().clone();
        (0..h)
            .map(|y| (0..w).map(|x| b[(x, y)].symbol().to_string()).collect())
            .collect()
    }

    fn at_builder(s: &Snapshot) -> ChainUi {
        let mut u = ChainUi::new();
        u.settle(s);
        u
    }

    #[test]
    fn the_hint_is_the_line_right_under_the_last_option() {
        let s = snap(&[("claude", true, LoggedIn), ("codex", true, LoggedIn)]);
        let rows = render(&at_builder(&s), &s, Glyphs::Unicode, 120, 36);
        let last = rows.iter().rposition(|r| r.contains("Codex")).unwrap();
        let hint = rows.iter().position(|r| r.contains("↑/↓ choose")).unwrap();
        assert_eq!(hint, last + 1, "{rows:#?}");
    }

    #[test]
    fn history_stays_above_the_question_and_the_footer_is_one_line() {
        let s = snap(&[("claude", true, LoggedIn), ("codex", true, LoggedIn)]);
        let mut u = at_builder(&s);
        u.enter(&s);
        let rows = render(&u, &s, Glyphs::Unicode, 120, 36);
        let h = rows
            .iter()
            .position(|r| r.contains("Builder") && r.contains("Claude Code"))
            .unwrap();
        let q = rows.iter().position(|r| r.contains("Reviewer")).unwrap();
        assert!(h < q, "{rows:#?}");
        let keys: Vec<_> = rows
            .iter()
            .enumerate()
            .filter(|(_, r)| r.contains("q quit"))
            .collect();
        assert_eq!(keys.len(), 1, "{rows:#?}");
        assert_eq!(keys[0].0, rows.len() - 1);
    }

    #[test]
    fn at_80x24_with_three_agents_the_whole_active_zone_is_visible() {
        let s = snap(&[
            ("claude", true, LoggedIn),
            ("codex", true, LoggedIn),
            ("gemini", true, LoggedIn),
        ]);
        let mut u = at_builder(&s);
        u.enter(&s);
        let text = render(&u, &s, Glyphs::Unicode, 80, 24).join("\n");
        for want in [
            "Reviewer",
            "Claude Code",
            "Codex",
            "Gemini",
            "↑/↓ choose",
            "q quit",
        ] {
            assert!(text.contains(want), "missing {want}:\n{text}");
        }
    }

    #[test]
    fn ascii_glyphs_replace_the_marks() {
        let s = snap(&[("claude", true, LoggedIn), ("codex", true, LoggedIn)]);
        let mut u = at_builder(&s);
        u.enter(&s);
        let text = render(&u, &s, Glyphs::Ascii, 100, 30).join("\n");
        for g in ['◇', '◆', '●', '○', '│', '└', '↑', '↓', '←', '·', '⚠'] {
            assert!(!text.contains(g), "{g} in an ASCII terminal:\n{text}");
        }
        assert!(text.contains("Reviewer"), "{text}");
    }

    #[test]
    fn no_next_banner_and_no_rail() {
        let s = snap(&[("claude", true, LoggedIn)]);
        let text = render(&at_builder(&s), &s, Glyphs::Unicode, 120, 36).join("\n");
        assert!(text.contains("Builder"), "the question is drawn:\n{text}");
        assert!(!text.contains("Next:"), "{text}");
        assert!(!text.contains("Scan"), "{text}");
    }

    #[test]
    fn a_blocker_shows_its_exact_command_in_a_box() {
        let mut s = snap(&[("claude", true, LoggedIn)]);
        s.herdr.found = false;
        let mut u = ChainUi::new();
        u.settle(&s);
        let rows = render(&u, &s, Glyphs::Unicode, 160, 36);
        let cmd = crate::tui::install::quote_cmd(tool("herdr").unwrap().install.unwrap());
        assert!(
            rows.iter().any(|r| r.contains("Herdr is not installed")),
            "{rows:#?}"
        );
        assert!(rows.iter().any(|r| r.contains(&cmd)), "{rows:#?}");
        assert!(rows.iter().any(|r| r.contains('╭')), "a box: {rows:#?}");
    }
}
