//! The Test screen: the doctor's checks with their fixes, and an optional sample task.
use super::action::Action;
use super::hit::HitMap;
use super::state::{Confirm, Effect, Overlay, State};
use super::style::Role;
use super::theme::{Glyphs, Theme};
use super::widgets::{button, fit, pad, progress, status_cell, status_glyph, status_width};
use crate::check::view::Status;
use crate::doctor::Check;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TestUi {
    pub checks: Option<Vec<Check>>,
    pub running: bool,
    /// 0: run the checks again, 1: run a sample task.
    pub focus: usize,
    pub sample_running: bool,
    /// What the sample task ended with, in words.
    pub sample: Option<Result<String, String>>,
}

pub fn update(s: &mut State, a: Action) -> Effect {
    match a {
        Action::Up | Action::PrevField => s.test.focus = 0,
        Action::Down | Action::NextField => s.test.focus = 1,
        Action::Home => s.test.focus = 0,
        Action::End => s.test.focus = 1,
        Action::Select(i) => {
            s.test.focus = i.min(1);
            return activate(s);
        }
        Action::Activate | Action::Toggle => return activate(s),
        _ => {}
    }
    Effect::None
}

fn activate(s: &mut State) -> Effect {
    if s.test.focus == 0 {
        return rerun(s);
    }
    if s.test.sample_running {
        s.toast = Some("The sample task is still running.".into());
        return Effect::None;
    }
    s.overlay = Some(Overlay::Confirm(Confirm::SampleTask));
    Effect::None
}

/// Run the checks, unless they are running already.
pub fn rerun(s: &mut State) -> Effect {
    if s.test.running {
        return Effect::None;
    }
    s.test.running = true;
    Effect::RunChecks
}

/// Where the chat skill is kept in this repository, and where Hermes reads it.
const SKILL: &str = "examples/hermes/igris/SKILL.md";
const SKILL_TARGET: &str = "~/.hermes/skills/igris/SKILL.md";

/// Every check passes and a sample task ran: setup is finished.
fn finished(s: &State) -> bool {
    s.tested
        && s.test
            .checks
            .as_ref()
            .is_some_and(|c| c.iter().all(|c| c.ok))
}

fn name_of(label: Option<String>) -> String {
    label
        .and_then(|l| crate::catalog::tool(&l).map(|t| t.name.to_string()))
        .unwrap_or_else(|| "?".into())
}

/// What was set up, in words. The assistant line says plainly when there is none.
pub fn summary_lines(s: &State) -> Vec<String> {
    use super::tools_screen::AssistantChoice;
    let pick =
        |sel: Option<&super::form::Select>| sel.and_then(|x| x.current()).map(|i| i.label.clone());
    let form = s.pipeline.form.as_ref();
    let line = format!(
        "Herdr runs the agents · {} builds · {} reviews · {} decides",
        name_of(pick(form.map(|f| &f.builder))),
        name_of(pick(form.map(|f| &f.reviewer))),
        pick(form.map(|f| &f.backend)).unwrap_or_else(|| "rules".into()),
    );
    let assistant = match s.tools.assistant {
        AssistantChoice::CliOnly => {
            "Assistant: CLI only. You start tasks from the terminal; there is no chat app."
                .to_string()
        }
        AssistantChoice::Tool(id) => format!(
            "Assistant: {}. Chat with it once its chat app is linked.",
            crate::catalog::tool(id).map_or(id, |t| t.name)
        ),
        AssistantChoice::Unset => "Assistant: not chosen yet (key 2, Tools).".to_string(),
    };
    vec![line, assistant]
}

/// The commands to use now, then (without a chat assistant) how to add one later.
pub fn next_steps_lines(s: &State) -> Vec<String> {
    use super::tools_screen::AssistantChoice;
    let mut v = vec![
        "Next steps".to_string(),
        "  igris run --task \"<your task>\"   start a task".to_string(),
        "  igris doctor                     check this machine again".to_string(),
        "  igris continue <id>              carry on after you answer an agent in Herdr"
            .to_string(),
    ];
    match s.tools.assistant {
        AssistantChoice::CliOnly => {
            v.push(format!(
                "Chat later: copy {SKILL} to {SKILL_TARGET}, then set up a chat app in Hermes."
            ));
        }
        AssistantChoice::Tool("hermes") => {
            v.push(format!(
                "Chat: copy {SKILL} to {SKILL_TARGET}, then link a chat app in Hermes."
            ));
        }
        _ => {}
    }
    v
}

pub fn draw(f: &mut Frame, area: Rect, s: &State, t: &Theme, hits: &mut HitMap, clickable: bool) {
    let width = area.width as usize;
    let ascii = t.glyphs == Glyphs::Ascii;
    let dim = t.style(Role::Dim);
    let bold = Modifier::BOLD;
    let at = |row: u16| Rect::new(area.x, area.y + row, area.width, 1);
    // The heading: a sweep while the checks run, then how many need a step.
    let heading = match &s.test.checks {
        None => {
            let mut v = vec![Span::styled("Running the checks ", t.style(Role::Text))];
            v.extend(progress(t, 14, None, s.tick).spans);
            v
        }
        Some(checks) => {
            let bad = checks.iter().filter(|c| !c.ok).count();
            if bad == 0 {
                vec![Span::styled(
                    format!(
                        "{} All checks pass. This machine is ready to run a task.",
                        status_glyph(t, Status::Ready)
                    ),
                    t.style(Role::Ok),
                )]
            } else {
                vec![
                    Span::styled(
                        format!("{} {bad} need a step", status_glyph(t, Status::NeedsStep)),
                        t.style(Role::Bad),
                    ),
                    Span::styled(format!(" of {} checks", checks.len()), dim),
                ]
            }
        }
    };
    f.render_widget(Paragraph::new(Line::from(heading)), at(0));

    // The table: id, detail, status in one column at the right edge; a failing check says how to fix it.
    let table_bottom = area.height.saturating_sub(5);
    let mut row = 2u16;
    // Once setup is finished the table has said its piece: the summary and the next steps replace it.
    let table = if finished(s) {
        None
    } else {
        s.test.checks.as_ref()
    };
    if finished(s) {
        let mut lines = summary_lines(s);
        lines.push(String::new());
        lines.extend(next_steps_lines(s));
        for (i, text) in lines.iter().enumerate() {
            let y = 2 + i as u16;
            if y >= table_bottom {
                break;
            }
            let style = if text == "Next steps" {
                t.style(Role::Text).add_modifier(bold)
            } else {
                dim
            };
            f.render_widget(
                Paragraph::new(Line::styled(fit(t, text, width.saturating_sub(2)), style)),
                at(y),
            );
        }
    }
    if let Some(checks) = table {
        let sw = status_width(t);
        let detail_w = width.saturating_sub(2 + 10 + 1 + 1 + sw);
        for c in checks {
            if row >= table_bottom {
                break;
            }
            let st = if c.ok {
                Status::Ready
            } else {
                Status::NeedsStep
            };
            let line = Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    pad(t, c.id, 10),
                    t.style(Role::Text)
                        .add_modifier(if c.ok { Modifier::empty() } else { bold }),
                ),
                Span::raw(" "),
                Span::styled(pad(t, &c.detail, detail_w), dim),
                Span::raw(" "),
                status_cell(t, st),
            ]);
            f.render_widget(Paragraph::new(line), at(row));
            row += 1;
            if !c.ok && row < table_bottom {
                let fix = fit(t, &c.fix, width.saturating_sub(10 + 5));
                let line = Line::from(vec![
                    Span::raw("     "),
                    Span::styled("fix: ", t.style(Role::Bad)),
                    Span::styled(fix, dim),
                ]);
                f.render_widget(Paragraph::new(line), at(row));
                row += 1;
            }
        }
    }

    // The sample task's state or result, then the two buttons at the bottom.
    let result_row = area.height.saturating_sub(4);
    if s.test.sample_running {
        let mut v = vec![Span::styled(
            "Running the sample task ",
            t.style(Role::Text),
        )];
        v.extend(progress(t, 14, None, s.tick).spans);
        f.render_widget(Paragraph::new(Line::from(v)), at(result_row));
    } else {
        match &s.test.sample {
            Some(Ok(said)) => {
                let text = fit(t, &format!("Sample task: {said}"), width.saturating_sub(2));
                let glyph = if ascii { "[x]" } else { "✓" };
                f.render_widget(
                    Paragraph::new(Line::styled(format!("{glyph} {text}"), t.style(Role::Ok))),
                    at(result_row),
                );
            }
            Some(Err(e)) => {
                let text = fit(
                    t,
                    &format!("Sample task failed: {e}"),
                    width.saturating_sub(2),
                );
                let glyph = if ascii { "[!]" } else { "●" };
                f.render_widget(
                    Paragraph::new(Line::styled(format!("{glyph} {text}"), t.style(Role::Bad))),
                    at(result_row),
                );
            }
            None => {}
        }
    }
    let labels = ["Run the checks again", "Run a sample task"];
    for (i, label) in labels.iter().enumerate() {
        let r = at(area.height.saturating_sub(2) + i as u16);
        let mark = match (s.test.focus == i, ascii) {
            (false, _) => " ",
            (true, false) => "▌",
            (true, true) => ">",
        };
        let mut spans = vec![
            Span::styled(mark, t.style(Role::Bad)),
            Span::raw(" "),
            button(t, label, s.test.focus == i),
        ];
        if i == 1 {
            spans.push(Span::styled("  uses your own agent subscriptions", dim));
        }
        if i == 0 && s.test.running {
            spans.push(Span::styled("  running", dim));
        }
        f.render_widget(Paragraph::new(Line::from(spans)), r);
        if clickable {
            hits.add(r, Action::Select(i));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::view::tests::real;
    use crate::tui::detect_worker::Msg;
    use crate::tui::next_step::{Step, next_step};
    use crate::tui::state::{Screen, update as app_update};
    use crate::tui::theme::Env;
    use ratatui::{Terminal, backend::TestBackend};

    fn check(id: &'static str, ok: bool) -> Check {
        Check {
            id,
            ok,
            detail: format!("{id} detail"),
            fix: if ok {
                String::new()
            } else {
                format!("fix for {id}")
            },
        }
    }

    fn on_test() -> State {
        let mut s = State::new((100, 30));
        s.repo = std::env::temp_dir();
        s.apply(Msg::Detected(Box::new(real())));
        s
    }

    fn render(s: &State) -> (String, HitMap) {
        render_h(s, 24)
    }

    fn render_h(s: &State, h: u16) -> (String, HitMap) {
        let mut term = Terminal::new(TestBackend::new(100, h)).unwrap();
        let mut hits = HitMap::default();
        let t = Theme::detect(&Env::default());
        term.draw(|f| draw(f, f.area(), s, &t, &mut hits, true))
            .unwrap();
        let buf = term.backend().buffer().clone();
        let text = (0..h)
            .map(|y| {
                (0..100u16)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        (text, hits)
    }

    /// Setup is finished: the checks pass and the sample task ran.
    fn green() -> State {
        let mut s = on_test();
        s.blocker = None;
        s.test.checks = Some(vec![check("config", true)]);
        s.tested = true;
        s.tools.assistant = crate::tui::tools_screen::AssistantChoice::CliOnly;
        s
    }

    #[test]
    fn the_next_steps_box_lists_run_doctor_and_continue_after_a_green_sample() {
        let (text, _) = render(&green());
        for want in [
            "Next steps",
            "igris run --task \"<your task>\"",
            "igris doctor",
            "igris continue <id>",
        ] {
            assert!(text.contains(want), "{want}:\n{text}");
        }
        // Not before the sample ran, and not while a check fails.
        let mut s = green();
        s.tested = false;
        assert!(!render(&s).0.contains("Next steps"));
        let mut s = green();
        s.test.checks = Some(vec![check("config", true), check("clean", false)]);
        let (text, _) = render(&s);
        assert!(
            !text.contains("Next steps") && text.contains("fix for clean"),
            "{text}"
        );
    }

    #[test]
    fn cli_only_adds_the_chat_later_line() {
        use crate::tui::tools_screen::AssistantChoice;
        let (text, _) = render(&green());
        assert!(
            text.contains("Chat later:") && text.contains("examples/hermes/igris/SKILL.md"),
            "{text}"
        );
        let mut s = green();
        s.tools.assistant = AssistantChoice::Tool("openclaw");
        assert!(
            !render(&s).0.contains("SKILL.md"),
            "no Hermes skill for another assistant"
        );
        s.tools.assistant = AssistantChoice::Tool("hermes");
        assert!(
            render(&s)
                .0
                .contains("Chat: copy examples/hermes/igris/SKILL.md")
        );
    }

    #[test]
    fn the_summary_says_cli_only_plainly() {
        let (text, _) = render(&green());
        assert!(
            text.contains(
                "Assistant: CLI only. You start tasks from the terminal; there is no chat app."
            ),
            "{text}"
        );
        assert!(
            text.contains("Herdr runs the agents ·") && text.contains("builds ·"),
            "{text}"
        );
    }

    #[test]
    fn the_summary_and_the_steps_fit_the_smallest_panel() {
        // The smallest window (80x24) leaves a panel about 15 rows high.
        let (text, _) = render_h(&green(), 15);
        for want in ["Assistant: CLI only", "igris continue <id>", "Chat later:"] {
            assert!(text.contains(want), "{want}:\n{text}");
        }
        assert!(
            text.contains("Run a sample task"),
            "the buttons stay:\n{text}"
        );
    }

    #[test]
    fn opening_the_screen_runs_the_checks_once() {
        let mut s = on_test();
        assert_eq!(
            app_update(&mut s, Action::Goto(Screen::Test)),
            Effect::RunChecks
        );
        assert!(s.test.running);
        assert_eq!(
            app_update(&mut s, Action::Goto(Screen::Check)),
            Effect::None
        );
        assert_eq!(
            app_update(&mut s, Action::Goto(Screen::Test)),
            Effect::None,
            "already running"
        );
        s.apply(Msg::Checked(vec![check("config", true)]));
        assert!(!s.test.running);
        assert_eq!(
            {
                crate::tui::verify_screen::clear_all(&mut s);
                app_update(&mut s, Action::Goto(Screen::Review))
            },
            Effect::None
        );
        assert_eq!(
            app_update(&mut s, Action::Goto(Screen::Test)),
            Effect::RunChecks,
            "fresh each visit"
        );
    }

    #[test]
    fn what_the_screen_shows_is_what_the_doctor_returned_with_the_fix_text() {
        let mut s = on_test();
        let list = vec![check("config", true), check("herdr", false)];
        s.apply(Msg::Checked(list.clone()));
        let (text, _) = render(&s);
        for c in &list {
            assert!(text.contains(c.id) && text.contains(&c.detail), "{text}");
        }
        assert!(
            text.contains("fix: fix for herdr") && text.contains("1 need a step"),
            "{text}"
        );
        assert!(!text.contains("fix: fix for config"));
    }

    #[test]
    fn the_checks_the_screen_runs_are_the_doctors_own() {
        let repo = std::env::temp_dir().join(format!("igris-doctor-eq-{}", std::process::id()));
        std::fs::create_dir_all(&repo).unwrap();
        let direct = crate::doctor::checks(&repo, None);
        let again = crate::doctor::checks(&repo, None);
        assert_eq!(direct, again);
        let ids: Vec<_> = direct.iter().map(|c| c.id).collect();
        assert_eq!(
            ids,
            [
                "config", "herdr", "builder", "reviewer", "repo", "clean", "base"
            ]
        );
        let _ = std::fs::remove_dir_all(&repo);
    }

    #[test]
    fn the_sample_task_asks_first_and_names_the_cost() {
        let mut s = on_test();
        app_update(&mut s, Action::Goto(Screen::Test));
        app_update(&mut s, Action::Down);
        assert_eq!(
            app_update(&mut s, Action::Activate),
            Effect::None,
            "nothing runs on the first press"
        );
        assert_eq!(s.overlay, Some(Overlay::Confirm(Confirm::SampleTask)));
        assert_eq!(app_update(&mut s, Action::No), Effect::None);
        assert_eq!(s.overlay, None);
        app_update(&mut s, Action::Activate);
        assert_eq!(app_update(&mut s, Action::Yes), Effect::RunSample);
        assert!(s.test.sample_running);
        s.apply(Msg::Sampled(Ok("done: all green".into())));
        assert!(!s.test.sample_running);
        let (text, _) = render(&s);
        assert!(text.contains("Sample task: done: all green"), "{text}");
        assert!(s.tested, "a finished sample counts as tested");
    }

    #[test]
    fn a_failed_sample_says_why_and_does_not_count_as_tested() {
        let mut s = on_test();
        s.apply(Msg::Sampled(Err("herdr is not running".into())));
        assert!(!s.tested);
        let (text, _) = render(&s);
        assert!(
            text.contains("Sample task failed: herdr is not running"),
            "{text}"
        );
    }

    #[test]
    fn passing_checks_and_a_sample_finish_setup() {
        let mut s = on_test();
        s.blocker = None;
        s.command_set = true;
        s.applied = true;
        s.tools.assistant = crate::tui::tools_screen::AssistantChoice::CliOnly;
        crate::tui::verify_screen::clear_all(&mut s); // the tools were checked this session
        assert_eq!(next_step(&s), Step::RunTest);
        s.apply(Msg::Checked(vec![check("config", true)]));
        s.apply(Msg::Sampled(Ok("done".into())));
        assert_eq!(next_step(&s), Step::Done);
    }

    #[test]
    fn everything_clickable_on_every_screen_also_has_a_key() {
        use crate::tui::keymap::BINDINGS;
        let mut s = on_test();
        s.apply(Msg::Checked(vec![
            check("config", true),
            check("herdr", false),
        ]));
        for screen in Screen::ALL {
            s.screen = screen;
            s.overlay = None;
            let mut term = Terminal::new(TestBackend::new(100, 30)).unwrap();
            let mut hits = HitMap::default();
            let t = Theme::detect(&Env::default());
            term.draw(|f| crate::tui::draw(f, &s, &t, &mut hits))
                .unwrap();
            for y in 0..30u16 {
                for x in 0..100u16 {
                    let Some(a) = hits.at(x, y) else { continue };
                    let keyed = BINDINGS.iter().any(|b| b.action == a);
                    let click_to_activate = matches!(a, Action::Select(_));
                    assert!(
                        keyed || click_to_activate,
                        "{screen:?}: {a:?} at {x},{y} has no key"
                    );
                }
            }
        }
    }

    fn buf_of(s: &State, t: &Theme) -> (String, ratatui::buffer::Buffer) {
        let mut term = Terminal::new(TestBackend::new(100, 24)).unwrap();
        let mut hits = HitMap::default();
        term.draw(|f| draw(f, f.area(), s, t, &mut hits, true))
            .unwrap();
        let buf = term.backend().buffer().clone();
        let text = (0..24u16)
            .map(|y| {
                (0..100u16)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        (text, buf)
    }

    #[test]
    fn while_the_checks_run_a_sweep_moves_and_it_is_gone_when_they_finish() {
        let t = Theme::detect(&Env::default());
        let mut s = on_test();
        s.test.running = true;
        let (a, _) = buf_of(&s, &t);
        s.tick = 7;
        let (b, _) = buf_of(&s, &t);
        assert!(a.contains("Running the checks") && a.contains('█'), "{a}");
        assert_ne!(a, b, "the sweep moves");
        s.apply(Msg::Checked(vec![check("config", true)]));
        let (c, _) = buf_of(&s, &t);
        assert!(
            !c.contains("Running the checks") && c.contains("All checks pass."),
            "{c}"
        );
    }

    #[test]
    fn the_sample_shows_a_sweep_while_it_runs_and_its_result_in_words() {
        let t = Theme::detect(&Env::default());
        let mut s = on_test();
        s.test.sample_running = true;
        let (text, _) = buf_of(&s, &t);
        assert!(
            text.contains("Running the sample task") && text.contains('█'),
            "{text}"
        );
        s.apply(Msg::Sampled(Ok("done: all green".into())));
        let (text, _) = buf_of(&s, &t);
        assert!(
            text.contains("✓ Sample task: done: all green")
                && !text.contains("Running the sample task"),
            "{text}"
        );
    }

    #[test]
    fn the_table_keeps_one_status_column_and_both_buttons_are_secondary() {
        let t = Theme {
            depth: crate::tui::theme::ColorDepth::True,
            glyphs: Glyphs::Unicode,
            motion: false,
        };
        let mut s = on_test();
        s.apply(Msg::Checked(vec![
            check("config", true),
            check("herdr", false),
            check("repo", true),
        ]));
        let (text, buf) = buf_of(&s, &t);
        let ends: Vec<&str> = text
            .lines()
            .filter(|l| l.trim_end().ends_with("ready") || l.trim_end().ends_with("needs a step"))
            .collect();
        assert_eq!(ends.len(), 3, "{text}");
        assert!(
            ends.iter().all(|l| l.chars().count() == 100),
            "one right-aligned column: {ends:?}"
        );
        for label in ["Run the checks again", "Run a sample task"] {
            let (y, l) = text
                .lines()
                .enumerate()
                .find(|(_, l)| l.contains(label))
                .unwrap();
            let x = l[..l.find(label).unwrap()].chars().count() as u16;
            assert_ne!(
                buf[(x, y as u16)].bg,
                ratatui::style::Color::Rgb(0xff, 0x30, 0x38),
                "{label} is not the primary button"
            );
        }
    }
}
