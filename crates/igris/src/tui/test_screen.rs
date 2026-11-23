//! The Test screen: the doctor's checks with their fixes, and an optional sample task.
use super::action::Action;
use super::hit::HitMap;
use super::state::{Confirm, Effect, Overlay, State};
use super::theme::{Glyphs, Theme};
use crate::doctor::Check;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::{Paragraph, Wrap};

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

pub fn draw(f: &mut Frame, area: Rect, s: &State, t: &Theme, hits: &mut HitMap, clickable: bool) {
    let ascii = t.glyphs == Glyphs::Ascii;
    let mut lines: Vec<String> = vec![];
    match &s.test.checks {
        None => lines.push("Running the checks…".into()),
        Some(checks) => {
            let bad = checks.iter().filter(|c| !c.ok).count();
            lines.push(if bad == 0 {
                "All checks pass. This machine is ready to run a task.".into()
            } else {
                format!("{bad} check(s) need a step:")
            });
            for c in checks {
                let (glyph, word) = match (c.ok, ascii) {
                    (true, false) => ("⬢", "ok"),
                    (false, false) => ("⬡", "needs a step"),
                    (true, true) => ("[x]", "ok"),
                    (false, true) => ("[ ]", "needs a step"),
                };
                lines.push(format!("  {glyph} {:<9} {}  [{word}]", c.id, c.detail));
                if !c.ok {
                    lines.push(format!("      fix: {}", c.fix));
                }
            }
        }
    }
    lines.push(String::new());
    let rows = [
        "Run the checks again",
        "Run a sample task (uses your own agent subscriptions)",
    ];
    let first_button = lines.len() as u16;
    for (i, r) in rows.iter().enumerate() {
        let mark = if s.test.focus == i { ">" } else { " " };
        let suffix = match i {
            0 if s.test.running => "  (running)",
            1 if s.test.sample_running => "  (running…)",
            _ => "",
        };
        lines.push(format!("{mark} [ {r} ]{suffix}"));
    }
    match &s.test.sample {
        Some(Ok(said)) => lines.extend([String::new(), format!("Sample task: {said}")]),
        Some(Err(e)) => lines.extend([String::new(), format!("Sample task failed: {e}")]),
        None => {}
    }
    f.render_widget(
        Paragraph::new(lines.join("\n"))
            .wrap(Wrap { trim: false })
            .style(Style::new()),
        area,
    );
    if clickable {
        for i in 0..2u16 {
            if first_button + i < area.height {
                hits.add(
                    Rect::new(area.x, area.y + first_button + i, area.width, 1),
                    Action::Select(i as usize),
                );
            }
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
        let mut term = Terminal::new(TestBackend::new(100, 24)).unwrap();
        let mut hits = HitMap::default();
        let t = Theme::detect(&Env::default());
        term.draw(|f| draw(f, f.area(), s, &t, &mut hits, true))
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
        (text, hits)
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
            app_update(&mut s, Action::Goto(Screen::Review)),
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
            text.contains("fix: fix for herdr") && text.contains("1 check(s) need a step"),
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
}
