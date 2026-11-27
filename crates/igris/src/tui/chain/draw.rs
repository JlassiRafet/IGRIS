//! The chain screen: header band, history of answered questions, the active question with its
//! hint right under the options, and a one-line footer. Pure drawing of a `ChainUi`.
use super::model::{ChainUi, Choice, Mark, Step};
use crate::tui::style::{GLOW, LIGHT_BLUE, Role, SHEEN, rgb};
use crate::tui::theme::{ColorDepth, Glyphs, Theme};
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
    dash: &'static str,
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
            dash: "-",
            on: "(*)",
            off: "( )",
            warn: "!",
            bar: ">",
            enter: "Enter",
            choose: "up/down",
            back: "Left",
        }
    } else {
        Marks {
            done: "◇",
            active: "◆",
            rail: "│",
            close: "└",
            dash: "─",
            on: "●",
            off: "○",
            warn: "⚠",
            bar: "▌",
            enter: "Enter",
            choose: "↑/↓",
            back: "←",
        }
    }
}

/// Rows that are not options: the blank line above the question, the question, and the action row
/// that closes the tree.
const FIXED_ROWS: usize = 3;
const BOX_ROWS: u16 = 5;
/// The column where the status words start, in the history and in the option rows alike.
const NOTE_COL: usize = 26;
/// Columns before an option's label: the tree line, the bar and the choice mark.
fn option_prefix(m: &Marks) -> usize {
    let w = |s: &str| unicode_width::UnicodeWidthStr::width(s);
    w(m.rail) + 1 + w(m.bar) + 2 + w(m.on)
}

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
                Span::styled(
                    format!("{answer:<w$} ", w = NOTE_COL - 2 - 10 - 1),
                    t.style(Role::Text),
                ),
                Span::styled(ascii_safe(state, t), t.style(Role::Dim)),
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
            lines.push(action_row(options.get(ui.cursor), &m, t, tick));
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

/// The status words use a middle dot; an ASCII terminal gets a hyphen instead.
fn ascii_safe(text: &str, t: &Theme) -> String {
    if t.glyphs == Glyphs::Ascii {
        text.replace('·', "-")
    } else {
        text.to_string()
    }
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

/// The call to action closes the tree: the corner, the Enter key, then the label with its glint.
/// The other keys follow as plain keycaps. A missing agent cannot be chosen yet, so there is no
/// call to action, only what comes next.
fn action_row(chosen: Option<&Choice>, m: &Marks, t: &Theme, tick: usize) -> Line<'static> {
    let key = |k: &str, verb: &str| {
        [
            Span::styled(k.to_string(), t.style(Role::Key)),
            Span::styled(format!(" {verb}   "), t.style(Role::Dim)),
        ]
    };
    let mut spans = vec![Span::styled(
        format!("{}{} ", m.close, m.dash.repeat(2)),
        t.style(Role::Dim),
    )];
    match chosen {
        Some(c) if c.mark != Mark::Dim => {
            spans.push(Span::styled(m.enter.to_string(), t.style(Role::Key)));
            spans.push(Span::raw("  "));
            spans.extend(shine(&format!("Use {}", c.label), tick, t));
            spans.push(Span::raw("   "));
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

/// Ticks of rest between two glints (a tick is 100 ms).
const GLINT_REST: usize = 14;

/// The label of the call to action. With motion on, a glint slides over its letters one step per
/// tick and then rests, like the effort slider in Claude Code: the letter under it is the sheen,
/// its neighbours the glow, the rest the plain blue. Without motion it is one steady span.
fn shine(text: &str, tick: usize, t: &Theme) -> Vec<Span<'static>> {
    let steady = t.style(Role::Cta);
    if !t.motion || t.depth != ColorDepth::True {
        return vec![Span::styled(text.to_string(), steady)];
    }
    let len = text.chars().count();
    let at = (tick % (len + 4 + GLINT_REST)) as isize - 2;
    text.chars()
        .enumerate()
        .map(|(i, ch)| {
            let colour = match (i as isize - at).abs() {
                0 => SHEEN,
                1 => GLOW,
                _ => LIGHT_BLUE,
            };
            Span::styled(ch.to_string(), steady.fg(rgb(colour)))
        })
        .collect()
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
    let width = NOTE_COL - option_prefix(m) - 1;
    // The band starts at the bar, so it never touches the tree line.
    let mut row = vec![
        Span::styled(if selected { m.bar } else { " " }, t.style(Role::Info)),
        Span::styled(
            format!(" {} ", if selected { m.on } else { m.off }),
            t.style(if selected { Role::Info } else { Role::Dim }),
        ),
        Span::styled(format!("{:<width$} ", c.label), label),
        Span::styled(ascii_safe(&c.note, t), t.style(note_role)),
    ];
    if let (true, Some(w)) = (selected, &c.warn) {
        row.push(Span::styled(
            format!("  {} {w}", m.warn),
            t.style(note_role),
        ));
    }
    if selected {
        row = row
            .into_iter()
            .map(|s| s.patch_style(t.style(Role::Hover)))
            .collect();
    }
    let mut spans = vec![Span::styled(format!("{} ", m.rail), t.style(Role::Dim))];
    spans.extend(row);
    Line::from(spans)
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
    fn the_tree_closes_on_the_action_row_right_after_the_options() {
        let s = snap(&[("claude", true, LoggedIn), ("codex", true, LoggedIn)]);
        for (glyphs, corner) in [(Glyphs::Unicode, '└'), (Glyphs::Ascii, '`')] {
            let rows = render(&at_builder(&s), &s, glyphs, 120, 36);
            let last = rows.iter().rposition(|r| r.contains("Codex")).unwrap();
            assert!(
                rows[last + 1]
                    .trim_start()
                    .starts_with(&format!("{corner}--"))
                    || rows[last + 1]
                        .trim_start()
                        .starts_with(&format!("{corner}──")),
                "the corner sits on the action row: {rows:#?}"
            );
            assert!(rows[last + 1].contains("choose"), "{rows:#?}");
            assert_eq!(
                rows.iter()
                    .filter(|r| r.trim_start().starts_with(corner))
                    .count(),
                1,
                "{rows:#?}"
            );
        }
    }

    #[test]
    fn the_call_to_action_names_the_enter_key_in_words_and_appears_once() {
        let s = snap(&[("claude", true, LoggedIn), ("codex", true, LoggedIn)]);
        for glyphs in [Glyphs::Unicode, Glyphs::Ascii] {
            let rows = render(&at_builder(&s), &s, glyphs, 120, 36);
            assert_eq!(
                rows.iter()
                    .filter(|r| r.contains("Enter  Use Claude Code"))
                    .count(),
                1,
                "{rows:#?}"
            );
            assert!(rows.iter().all(|r| !r.contains('⏎')), "{rows:#?}");
        }
    }

    #[test]
    fn a_missing_agent_gets_no_call_to_action_and_says_what_comes_next() {
        let s = snap(&[("claude", false, LoggedIn), ("codex", false, LoggedIn)]);
        let rows = render(&at_builder(&s), &s, Glyphs::Unicode, 120, 36).join("\n");
        assert!(!rows.contains("Enter  Use"), "{rows}");
        assert!(rows.contains("install comes in the next step"), "{rows}");
    }

    fn truecolour_with_motion() -> Theme {
        Theme {
            depth: ColorDepth::True,
            glyphs: Glyphs::Unicode,
            motion: true,
        }
    }

    fn fgs(spans: &[Span<'static>]) -> Vec<Option<ratatui::style::Color>> {
        spans.iter().map(|s| s.style.fg).collect()
    }

    #[test]
    fn the_glint_slides_over_the_letters_one_step_per_tick_then_rests() {
        let t = truecolour_with_motion();
        let sheen = Some(rgb(crate::tui::style::SHEEN));
        let at = |tick| shine("Use Codex", tick, &t);
        let lead = |tick| fgs(&at(tick)).iter().position(|c| *c == sheen);
        assert_eq!(at(0).len(), 9, "one span per letter");
        let path: Vec<_> = (2..=10).map(lead).collect();
        assert_eq!(
            path,
            (0..=8).map(Some).collect::<Vec<_>>(),
            "the brightest letter moves right one step per tick"
        );
        let blue = Some(rgb(crate::tui::style::LIGHT_BLUE));
        assert!(
            fgs(&at(14)).iter().all(|c| *c == blue),
            "at rest every letter is the plain blue: {:?}",
            fgs(&at(14))
        );
        assert_eq!(fgs(&at(0)), fgs(&at(27)), "the cycle repeats");
    }

    #[test]
    fn every_letter_of_the_glint_stays_bold_and_has_no_background() {
        let t = truecolour_with_motion();
        for tick in 0..30 {
            for span in shine("Use Codex", tick, &t) {
                assert!(
                    span.style
                        .add_modifier
                        .contains(ratatui::style::Modifier::BOLD),
                    "tick {tick}"
                );
                assert_eq!(span.style.bg, None, "tick {tick}: no rectangle");
            }
        }
    }

    #[test]
    fn without_motion_the_call_to_action_is_one_steady_span() {
        for depth in [ColorDepth::True, ColorDepth::Ansi16, ColorDepth::None] {
            let t = Theme {
                depth,
                glyphs: Glyphs::Unicode,
                motion: false,
            };
            let spans = shine("Use Codex", 5, &t);
            assert_eq!(spans.len(), 1, "{depth:?}");
            assert_eq!(spans[0].style, t.style(Role::Cta), "{depth:?}");
        }
        let odd = Theme {
            depth: ColorDepth::Ansi16,
            glyphs: Glyphs::Unicode,
            motion: true,
        };
        assert_eq!(
            shine("Use Codex", 5, &odd).len(),
            1,
            "motion needs truecolor"
        );
    }

    #[test]
    fn the_glint_never_changes_the_text_on_screen() {
        let s = snap(&[("claude", true, LoggedIn), ("codex", true, LoggedIn)]);
        let u = at_builder(&s);
        let t = truecolour_with_motion();
        let text_at = |tick| {
            let mut term = Terminal::new(TestBackend::new(120, 36)).unwrap();
            term.draw(|f| draw(f, f.area(), &u, &s, &t, tick)).unwrap();
            let b = term.backend().buffer().clone();
            let rows: Vec<String> = (0..36)
                .map(|y| (0..120).map(|x| b[(x, y)].symbol().to_string()).collect())
                .collect();
            rows[rows.iter().position(|r| r.contains("Use Claude")).unwrap()].clone()
        };
        let first = text_at(0);
        for tick in 1..30 {
            assert_eq!(text_at(tick), first, "tick {tick}");
        }
    }

    #[test]
    fn the_call_to_action_is_blue_text_with_no_rectangle_behind_it() {
        let s = snap(&[("claude", true, LoggedIn), ("codex", true, LoggedIn)]);
        let (b, rows) = true_colour(&at_builder(&s), &s);
        let st = cell_style(&b, &rows, "Use Claude");
        assert_eq!(st.fg, Some(rgb(crate::tui::style::LIGHT_BLUE)));
        assert_eq!(st.bg, Some(ratatui::style::Color::Reset), "no rectangle");
    }

    #[test]
    fn the_selected_band_starts_at_the_blue_bar_not_at_the_tree_line() {
        let s = snap(&[("claude", true, LoggedIn), ("codex", true, LoggedIn)]);
        let (b, rows) = true_colour(&at_builder(&s), &s);
        let y = rows.iter().position(|r| r.contains('▌')).unwrap();
        let col = |ch: char| rows[y].chars().position(|c| c == ch).unwrap() as u16;
        let hover = rgb((0x16, 0x22, 0x2b));
        assert_ne!(
            b[(col('│'), y as u16)].bg,
            hover,
            "the tree line stays clear"
        );
        assert_eq!(
            b[(col('▌'), y as u16)].bg,
            hover,
            "the band starts at the bar"
        );
        assert_eq!(b[(col('●'), y as u16)].bg, hover);
    }

    #[test]
    fn history_status_and_option_notes_share_one_column() {
        let s = snap(&[("claude", true, LoggedIn), ("codex", true, LoggedIn)]);
        let u = at_builder(&s);
        let note = u.choices(&s)[0].note.clone();
        for glyphs in [Glyphs::Unicode, Glyphs::Ascii] {
            let rows = render(&u, &s, glyphs, 120, 36);
            let col = |needle: &str, row: &str| {
                let b = row
                    .find(needle)
                    .unwrap_or_else(|| panic!("{needle} in {row}"));
                row[..b].chars().count()
            };
            let hist = rows.iter().find(|r| r.contains("Workspace")).unwrap();
            let opt = rows.iter().find(|r| r.contains("Claude Code")).unwrap();
            assert_eq!(col("ready", hist), col(&note, opt), "{glyphs:?}: {rows:#?}");
        }
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
    fn an_ascii_terminal_gets_no_middle_dot_even_in_the_status_words() {
        let s = snap(&[
            ("claude", true, igris_detect::Login::Unknown),
            ("codex", true, igris_detect::Login::Unknown),
        ]);
        let mut u = at_builder(&s);
        u.enter(&s);
        let text = render(&u, &s, Glyphs::Ascii, 100, 30).join("\n");
        assert!(text.contains("login unverified"), "{text}");
        assert!(text.is_ascii(), "{text}");
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
