//! The steps: Check, Pipeline, Review, Test. A rail on the left of a wide window, a one-row strip in
//! a narrow one. Each step shows whether it is done, to do or blocked, and where you are.
use super::action::Action;
use super::hit::HitMap;
use super::state::{Screen, State};
use super::style::Role;
use super::theme::{Glyphs, Theme};
use super::widgets::progress;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepState {
    Done,
    Todo,
    Blocked,
}

/// How far a step is, from the app state alone. Where you are is separate (`State::screen`).
pub fn step_state(s: &State, screen: Screen) -> StepState {
    match screen {
        Screen::Check => match (&s.blocker, &s.check.view) {
            (Some(_), _) => StepState::Blocked,
            (None, Some(_)) => StepState::Done,
            (None, None) => StepState::Todo,
        },
        Screen::Pipeline => {
            if s.pipeline.broken.is_some() || (s.command_set && !s.pipeline.errors.is_empty()) {
                StepState::Blocked
            } else if s.command_set {
                StepState::Done
            } else {
                StepState::Todo
            }
        }
        Screen::Review => {
            if s.applied && !s.dirty {
                StepState::Done
            } else {
                StepState::Todo
            }
        }
        Screen::Test => match &s.test.checks {
            Some(c) if c.iter().any(|c| !c.ok) => StepState::Blocked,
            Some(_) => StepState::Done,
            None if s.tested => StepState::Done,
            None => StepState::Todo,
        },
    }
}

/// `(steps done, 4)`.
pub fn setup_progress(s: &State) -> (usize, usize) {
    let done = Screen::ALL
        .iter()
        .filter(|sc| step_state(s, **sc) == StepState::Done)
        .count();
    (done, Screen::ALL.len())
}

fn glyph(t: &Theme, st: StepState, current: bool) -> (&'static str, Role) {
    let ascii = t.glyphs == Glyphs::Ascii;
    match (st, current, ascii) {
        (StepState::Done, _, false) => ("✓", Role::Ok),
        (StepState::Done, _, true) => ("x", Role::Ok),
        (StepState::Blocked, _, _) => ("!", Role::Bad),
        (StepState::Todo, true, false) => ("●", Role::Bad),
        (StepState::Todo, true, true) => (">", Role::Bad),
        (StepState::Todo, false, false) => ("○", Role::Dim),
        (StepState::Todo, false, true) => (" ", Role::Dim),
    }
}

fn step_spans(t: &Theme, s: &State, screen: Screen, n: usize) -> Vec<Span<'static>> {
    let current = s.screen == screen;
    let (g, role) = glyph(t, step_state(s, screen), current);
    let bar = if current {
        if t.glyphs == Glyphs::Ascii {
            ">"
        } else {
            "▌"
        }
    } else {
        " "
    };
    let mut label = t.style(Role::Text);
    if current {
        label = label.add_modifier(Modifier::BOLD);
    }
    vec![
        Span::styled(bar, t.style(Role::Bad)),
        Span::styled(g, t.style(role)),
        Span::raw(" "),
        Span::styled(format!("{n}"), t.style(Role::Dim)),
        Span::raw("  "),
        Span::styled(screen.title(), label),
    ]
}

pub fn draw_rail(
    f: &mut Frame,
    area: Rect,
    s: &State,
    t: &Theme,
    hits: &mut HitMap,
    clickable: bool,
) {
    for (i, screen) in Screen::ALL.iter().enumerate() {
        let r = Rect::new(area.x, area.y + i as u16, area.width.saturating_sub(2), 1);
        if r.bottom() > area.bottom() {
            break;
        }
        f.render_widget(
            Paragraph::new(Line::from(step_spans(t, s, *screen, i + 1))),
            r,
        );
        if clickable {
            hits.add(r, Action::Goto(*screen));
        }
    }
    let (done, total) = setup_progress(s);
    let y = area.y + Screen::ALL.len() as u16 + 1;
    if y + 2 <= area.bottom() {
        f.render_widget(
            Paragraph::new(Line::styled("Setup", t.style(Role::Dim))),
            Rect::new(area.x + 1, y, area.width.saturating_sub(2), 1),
        );
        let mut bar = progress(t, 12, Some(done as f32 / total as f32), 0).spans;
        bar.push(Span::styled(format!(" {done}/{total}"), t.style(Role::Dim)));
        f.render_widget(
            Paragraph::new(Line::from(bar)),
            Rect::new(area.x + 1, y + 1, area.width.saturating_sub(2), 1),
        );
    }
}

pub fn draw_strip(
    f: &mut Frame,
    area: Rect,
    s: &State,
    t: &Theme,
    hits: &mut HitMap,
    clickable: bool,
) {
    let mut x = area.x;
    for (i, screen) in Screen::ALL.iter().enumerate() {
        let spans = step_spans(t, s, *screen, i + 1);
        let width: u16 = spans.iter().map(|sp| sp.width() as u16).sum();
        if x + width > area.right() {
            break;
        }
        let r = Rect::new(x, area.y, width, 1);
        f.render_widget(Paragraph::new(Line::from(spans)), r);
        if clickable {
            hits.add(r, Action::Goto(*screen));
        }
        x += width + 2;
    }
    let (done, total) = setup_progress(s);
    let tail = format!("{done}/{total} done");
    let w = tail.chars().count() as u16;
    if x + w <= area.right() {
        f.render_widget(
            Paragraph::new(Line::styled(tail, t.style(Role::Dim))),
            Rect::new(area.right() - w, area.y, w, 1),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::view::tests::real;
    use crate::doctor::Check;
    use crate::tui::detect_worker::Msg;
    use crate::tui::theme::{ColorDepth, Env};
    use ratatui::{Terminal, backend::TestBackend};

    fn loaded() -> State {
        let mut s = State::new((100, 30));
        s.repo = std::env::temp_dir().join("igris-rail-none");
        s.apply(Msg::Detected(Box::new(real())));
        s
    }

    fn check(ok: bool) -> Check {
        Check {
            id: "clean",
            ok,
            detail: String::new(),
            fix: "commit it".into(),
        }
    }

    fn text(term: &Terminal<TestBackend>, w: u16, h: u16) -> Vec<String> {
        let b = term.backend().buffer().clone();
        (0..h)
            .map(|y| (0..w).map(|x| b[(x, y)].symbol().to_string()).collect())
            .collect()
    }

    #[test]
    fn each_step_is_done_todo_or_blocked_from_the_app_state() {
        let mut s = State::new((100, 30));
        assert_eq!(
            step_state(&s, Screen::Check),
            StepState::Todo,
            "still scanning"
        );
        s.apply(Msg::Detected(Box::new(real())));
        assert_eq!(step_state(&s, Screen::Check), StepState::Done);
        s.blocker = Some("log in".into());
        assert_eq!(step_state(&s, Screen::Check), StepState::Blocked);

        assert_eq!(
            step_state(&s, Screen::Pipeline),
            StepState::Todo,
            "no check command yet"
        );
        s.command_set = true;
        s.pipeline.errors.clear();
        assert_eq!(step_state(&s, Screen::Pipeline), StepState::Done);
        s.pipeline.errors = vec![crate::tui::pipeline::FieldError {
            field: "checks.timeout_s",
            message: "x".into(),
        }];
        assert_eq!(
            step_state(&s, Screen::Pipeline),
            StepState::Blocked,
            "a set command with a problem"
        );
        s.pipeline.errors.clear();

        assert_eq!(step_state(&s, Screen::Review), StepState::Todo);
        s.applied = true;
        assert_eq!(step_state(&s, Screen::Review), StepState::Done);
        s.dirty = true;
        assert_eq!(
            step_state(&s, Screen::Review),
            StepState::Todo,
            "unsaved changes"
        );

        assert_eq!(step_state(&s, Screen::Test), StepState::Todo);
        s.test.checks = Some(vec![check(true)]);
        assert_eq!(step_state(&s, Screen::Test), StepState::Done);
        s.test.checks = Some(vec![check(true), check(false)]);
        assert_eq!(step_state(&s, Screen::Test), StepState::Blocked);
    }

    #[test]
    fn setup_progress_counts_the_done_steps() {
        let mut s = State::new((100, 30));
        assert_eq!(setup_progress(&s), (0, 4));
        s.apply(Msg::Detected(Box::new(real())));
        s.command_set = true;
        s.pipeline.errors.clear();
        assert_eq!(setup_progress(&s), (2, 4));
    }

    #[test]
    fn the_rail_lists_four_clickable_steps_and_marks_where_you_are() {
        let mut s = loaded();
        s.screen = Screen::Pipeline;
        let t = Theme::detect(&Env::default());
        let mut term = Terminal::new(TestBackend::new(22, 12)).unwrap();
        let mut hits = HitMap::default();
        term.draw(|f| draw_rail(f, f.area(), &s, &t, &mut hits, true))
            .unwrap();
        let rows = text(&term, 22, 12);
        assert!(
            rows[0].contains("Check")
                && rows[1].contains("Pipeline")
                && rows[2].contains("Review")
                && rows[3].contains("Test"),
            "{rows:?}"
        );
        assert!(
            rows[1].contains('▌') && !rows[0].contains('▌'),
            "the bar marks the current step: {rows:?}"
        );
        assert!(rows[0].contains('✓'), "Check is done: {rows:?}");
        for (y, sc) in Screen::ALL.iter().enumerate() {
            assert_eq!(hits.at(3, y as u16), Some(Action::Goto(*sc)));
        }
        assert!(
            rows.iter().any(|r| r.contains("Setup")) && rows.iter().any(|r| r.contains("1/4")),
            "{rows:?}"
        );
    }

    #[test]
    fn the_strip_shows_the_same_steps_in_one_row() {
        let s = loaded();
        let t = Theme::detect(&Env::default());
        let mut term = Terminal::new(TestBackend::new(78, 1)).unwrap();
        let mut hits = HitMap::default();
        term.draw(|f| draw_strip(f, f.area(), &s, &t, &mut hits, true))
            .unwrap();
        let row = &text(&term, 78, 1)[0];
        for label in ["Check", "Pipeline", "Review", "Test", "1/4 done"] {
            assert!(row.contains(label), "{label}: {row}");
        }
        let col = row[..row.find("Review").unwrap()].chars().count() as u16;
        assert_eq!(hits.at(col, 0), Some(Action::Goto(Screen::Review)));
    }

    #[test]
    fn an_ascii_terminal_gets_ascii_marks() {
        let s = loaded();
        let t = Theme {
            depth: ColorDepth::None,
            glyphs: Glyphs::Ascii,
            motion: false,
        };
        let mut term = Terminal::new(TestBackend::new(22, 12)).unwrap();
        let mut hits = HitMap::default();
        term.draw(|f| draw_rail(f, f.area(), &s, &t, &mut hits, true))
            .unwrap();
        let rows = text(&term, 22, 12);
        assert!(rows.iter().all(|r| r.is_ascii()), "{rows:?}");
        assert!(rows[0].contains('>') && rows[0].contains('x'), "{rows:?}");
    }
}
