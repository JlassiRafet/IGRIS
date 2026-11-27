//! The chain screen: header band, history of answered questions, the active question with its
//! hint right under the options, and a one-line footer. Pure drawing of a `ChainUi`.
use super::model::{ChainUi, Choice, Mark, Step};
use crate::tui::style::Role;
use crate::tui::theme::{Glyphs, Theme};
use crate::tui::{header, layout, widgets};
use igris_detect::Snapshot;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
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
    bar: &'static str,
    enter: &'static str,
    choose: &'static str,
    back: &'static str,
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
            bar: ">",
            enter: "Enter:",
            choose: "up/down",
            back: "Left",
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
            bar: "▌",
            enter: "⏎",
            choose: "↑/↓",
            back: "←",
        }
    }
}

/// Rows that are not options: the blank line above the question, the question, the blank rail
/// line, the action row and the closing mark.
const FIXED_ROWS: usize = 5;
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
    let mut lines: Vec<Line> = ui
        .history(snap)
        .iter()
        .map(|(label, answer, state)| {
            Line::from(vec![
                Span::styled(format!("{} ", m.done), t.style(Role::Dim)),
                Span::styled(format!("{label:<10}"), t.style(Role::Dim)),
                Span::styled(format!("{answer:<26}"), t.style(Role::Text)),
                Span::styled(state.clone(), t.style(Role::Dim)),
            ])
        })
        .collect();
    let box_rows = if blocker.is_some() { BOX_ROWS } else { 0 };
    let used = lines.len() + FIXED_ROWS + usize::from(box_rows);
    let room = (body.height as usize).saturating_sub(used).max(1);
    match ui.step {
        Step::End => lines.push(Line::styled(
            "Slice 1 ends here: the next questions come in the next release candidate.",
            t.style(Role::Dim),
        )),
        step => {
            let options = ui.choices(snap);
            lines.push(Line::raw(""));
            lines.push(question(step, &m, t));
            let rows = option_rows(&options, ui.cursor, &m, t);
            let at = rows
                .iter()
                .position(|(_, i)| *i == Some(ui.cursor))
                .unwrap_or(0);
            let start = (at + 1).saturating_sub(room);
            lines.extend(rows.into_iter().skip(start).take(room).map(|(l, _)| l));
            lines.push(Line::styled(m.rail, t.style(Role::Dim)));
            lines.push(action_row(options.get(ui.cursor), &m, t));
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

/// The active question: a bold title, then the question itself.
fn question(step: Step, m: &Marks, t: &Theme) -> Line<'static> {
    let (title, ask) = match step {
        Step::Workspace => ("Workspace", ": Herdr runs your agents side by side"),
        Step::Builder => ("Builder", ": which agent writes the code?"),
        Step::Reviewer => ("Reviewer", ": who checks the builder's work?"),
        Step::End => ("", ""),
    };
    Line::from(vec![
        Span::styled(format!("{} ", m.active), t.style(Role::Info)),
        Span::styled(title, t.style(Role::Text).add_modifier(Modifier::BOLD)),
        Span::styled(ask, t.style(Role::Text)),
    ])
}

/// The option rows, with "Installed" and "Not installed yet" above their groups when both exist.
/// Each row carries the index of its option; a label row carries none.
fn option_rows(
    options: &[Choice],
    cursor: usize,
    m: &Marks,
    t: &Theme,
) -> Vec<(Line<'static>, Option<usize>)> {
    let grouped =
        options.iter().any(|c| c.mark == Mark::Dim) && options.iter().any(|c| c.mark != Mark::Dim);
    let label = |text: &'static str| {
        Line::from(vec![
            Span::styled(format!("{}   ", m.rail), t.style(Role::Dim)),
            Span::styled(text, t.style(Role::Dim)),
        ])
    };
    let mut rows = Vec::new();
    for (i, c) in options.iter().enumerate() {
        if grouped && i == 0 {
            rows.push((label("Installed"), None));
        }
        if grouped && c.mark == Mark::Dim && (i == 0 || options[i - 1].mark != Mark::Dim) {
            rows.push((label("Not installed yet"), None));
        }
        rows.push((option_line(c, i == cursor, m, t), Some(i)));
    }
    rows
}

/// The one call to action as a chip, then the other keys as plain keycaps. A missing agent cannot
/// be chosen yet, so there is no chip, only what comes next.
fn action_row(chosen: Option<&Choice>, m: &Marks, t: &Theme) -> Line<'static> {
    let key = |k: &str, verb: &str| {
        [
            Span::styled(k.to_string(), t.style(Role::Key)),
            Span::styled(format!(" {verb}   "), t.style(Role::Dim)),
        ]
    };
    let mut spans = vec![Span::styled(format!("{}   ", m.rail), t.style(Role::Dim))];
    match chosen {
        Some(c) if c.mark != Mark::Dim => {
            let chip = format!(" {} Use {} ", m.enter, c.label);
            spans.extend([Span::styled(chip, t.style(Role::Cta)), Span::raw("   ")]);
            spans.extend(key(m.choose, "choose"));
        }
        Some(_) => {
            spans.extend([
                Span::styled(m.enter.to_string(), t.style(Role::Key)),
                Span::styled(" install comes in the next step   ", t.style(Role::Dim)),
            ]);
            spans.extend(key(m.choose, "choose"));
        }
        None => {}
    }
    if chosen.is_some() {
        spans.extend(key(m.back, "back"));
    }
    spans.extend(key("r", "re-scan"));
    Line::from(spans)
}

/// Red is for a step the user must take, and only on the row the cursor is on; colour marks focus.
fn note_role(c: &Choice, selected: bool) -> Role {
    match c.mark {
        Mark::Ready => Role::Ok,
        Mark::Step if selected => Role::Bad,
        Mark::Step | Mark::Warn | Mark::Dim => Role::Dim,
    }
}

fn option_line(c: &Choice, selected: bool, m: &Marks, t: &Theme) -> Line<'static> {
    let note_role = note_role(c, selected);
    let mut label = t.style(if c.mark == Mark::Dim {
        Role::Dim
    } else {
        Role::Text
    });
    if selected {
        label = label.add_modifier(Modifier::BOLD);
    }
    let mut spans = vec![
        Span::styled(format!("{} ", m.rail), t.style(Role::Dim)),
        Span::styled(if selected { m.bar } else { " " }, t.style(Role::Info)),
        Span::styled(
            format!(" {} ", if selected { m.on } else { m.off }),
            t.style(if selected { Role::Info } else { Role::Dim }),
        ),
        Span::styled(format!("{:<14}", c.label), label),
        Span::styled(c.note.clone(), t.style(note_role)),
    ];
    if let (true, Some(w)) = (selected, &c.warn) {
        spans.push(Span::styled(
            format!("  {} {w}", m.warn),
            t.style(note_role),
        ));
    }
    let line = Line::from(spans);
    if selected {
        line.style(t.style(Role::Hover))
    } else {
        line
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::tool;
    use crate::tui::theme::ColorDepth;
    use crate::tui::tools_screen::tests::snap;
    use igris_detect::Login::{LoggedIn, LoggedOut};
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
    fn the_action_row_follows_the_options_after_one_blank_line() {
        let s = snap(&[("claude", true, LoggedIn), ("codex", true, LoggedIn)]);
        let rows = render(&at_builder(&s), &s, Glyphs::Unicode, 120, 36);
        let last = rows.iter().rposition(|r| r.contains("Codex")).unwrap();
        let action = rows.iter().position(|r| r.contains("↑/↓ choose")).unwrap();
        assert_eq!(action, last + 2, "{rows:#?}");
        assert!(
            rows[last + 1]
                .trim_matches(|c: char| c == '│' || c == ' ')
                .is_empty()
        );
    }

    #[test]
    fn one_chip_names_the_primary_action_and_the_other_keys_stay_plain() {
        let s = snap(&[("claude", true, LoggedIn), ("codex", true, LoggedIn)]);
        let rows = render(&at_builder(&s), &s, Glyphs::Unicode, 120, 36);
        assert_eq!(
            rows.iter()
                .filter(|r| r.contains("⏎ Use Claude Code"))
                .count(),
            1,
            "{rows:#?}"
        );
        let ascii = render(&at_builder(&s), &s, Glyphs::Ascii, 120, 36).join("\n");
        assert!(ascii.contains("Enter: Use Claude Code"), "{ascii}");
    }

    #[test]
    fn a_missing_agent_gets_no_chip_and_says_what_comes_next() {
        let s = snap(&[("claude", false, LoggedIn), ("codex", false, LoggedIn)]);
        let rows = render(&at_builder(&s), &s, Glyphs::Unicode, 120, 36).join("\n");
        assert!(!rows.contains("⏎ Use"), "{rows}");
        assert!(rows.contains("install comes in the next step"), "{rows}");
    }

    #[test]
    fn the_selected_row_has_a_bar_and_the_others_do_not() {
        let s = snap(&[("claude", true, LoggedIn), ("codex", true, LoggedIn)]);
        let rows = render(&at_builder(&s), &s, Glyphs::Unicode, 120, 36);
        let bars: Vec<_> = rows.iter().filter(|r| r.contains('▌')).collect();
        assert_eq!(bars.len(), 1, "{rows:#?}");
        assert!(bars[0].contains("Claude Code"));
    }

    #[test]
    fn group_labels_appear_only_when_both_groups_exist() {
        let both = snap(&[("claude", true, LoggedIn), ("gemini", false, LoggedIn)]);
        let rows = render(&at_builder(&both), &both, Glyphs::Unicode, 120, 36).join("\n");
        assert!(
            rows.contains("Installed") && rows.contains("Not installed yet"),
            "{rows}"
        );
        let only = snap(&[("claude", true, LoggedIn)]);
        let rows = render(&at_builder(&only), &only, Glyphs::Unicode, 120, 36).join("\n");
        assert!(
            !rows.contains("Not installed yet") && !rows.contains("Installed"),
            "{rows}"
        );
    }

    fn cell_style(
        buf: &ratatui::buffer::Buffer,
        rows: &[String],
        needle: &str,
    ) -> ratatui::style::Style {
        let y = rows
            .iter()
            .position(|r| r.contains(needle))
            .unwrap_or_else(|| panic!("no row with {needle}"));
        let x = rows[y]
            .find(needle)
            .map(|b| rows[y][..b].chars().count())
            .unwrap();
        buf[(x as u16, y as u16)].style()
    }

    fn true_colour(ui: &ChainUi, s: &Snapshot) -> (ratatui::buffer::Buffer, Vec<String>) {
        let t = Theme {
            depth: ColorDepth::True,
            glyphs: Glyphs::Unicode,
            motion: false,
        };
        let mut term = Terminal::new(TestBackend::new(120, 36)).unwrap();
        term.draw(|f| draw(f, f.area(), ui, s, &t, 0)).unwrap();
        let b = term.backend().buffer().clone();
        let rows = (0..36)
            .map(|y| (0..120).map(|x| b[(x, y)].symbol().to_string()).collect())
            .collect();
        (b, rows)
    }

    #[test]
    fn history_is_quieter_than_the_question() {
        let s = snap(&[("claude", true, LoggedIn), ("codex", true, LoggedIn)]);
        let mut u = at_builder(&s);
        u.enter(&s);
        let (b, rows) = true_colour(&u, &s);
        let t = Theme {
            depth: ColorDepth::True,
            glyphs: Glyphs::Unicode,
            motion: false,
        };
        assert_eq!(
            cell_style(&b, &rows, "Workspace").fg,
            t.style(Role::Dim).fg,
            "answered label is dim"
        );
        let q = cell_style(&b, &rows, "Reviewer:");
        assert!(
            q.add_modifier.contains(ratatui::style::Modifier::BOLD),
            "the question title is bold"
        );
    }

    #[test]
    fn colour_marks_focus_the_step_is_red_only_on_the_selected_row() {
        let s = snap(&[("claude", true, LoggedOut), ("codex", true, LoggedIn)]);
        let mut u = at_builder(&s);
        let red = Theme {
            depth: ColorDepth::True,
            glyphs: Glyphs::Unicode,
            motion: false,
        }
        .style(Role::Bad)
        .fg;
        let (b, rows) = true_colour(&u, &s); // the default is Codex, so Claude is not selected
        assert_ne!(cell_style(&b, &rows, "needs a step").fg, red);
        u.cursor = u.choices(&s).iter().position(|c| c.id == "claude").unwrap();
        let (b, rows) = true_colour(&u, &s);
        assert_eq!(cell_style(&b, &rows, "needs a step").fg, red);
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
    fn red_is_only_for_a_selected_step_the_user_must_take() {
        let mut c = Choice {
            id: "claude".into(),
            label: "Claude Code".into(),
            note: "installed · login unverified".into(),
            mark: Mark::Warn,
            warn: Some("same agent, new session".into()),
        };
        assert_eq!(
            note_role(&c, true),
            Role::Dim,
            "unverified, and the neutral same-agent note"
        );
        c.mark = Mark::Step;
        assert_eq!(note_role(&c, false), Role::Dim);
        assert_eq!(note_role(&c, true), Role::Bad);
        c.mark = Mark::Ready;
        assert_eq!(note_role(&c, false), Role::Ok);
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
