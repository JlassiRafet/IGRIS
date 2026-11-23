//! The Review screen: the exact change to `igris.toml`, where it goes, and one Apply button.
use super::action::Action;
use super::apply::{Applied, Change, DiffLine, Target, plan_change};
use super::hit::HitMap;
use super::state::{Effect, State};
use super::style::Role;
use super::theme::{Glyphs, Theme};
use super::widgets::{button, fit, fit_path};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use std::path::PathBuf;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReviewUi {
    /// `None`: where the file was loaded from, else this repository.
    pub target: Option<Target>,
    /// 0 is the target line, 1 is the Apply button.
    pub focus: usize,
    pub scroll: usize,
    pub change: Option<Change>,
    /// Why there is nothing to apply yet, in words.
    pub problems: Vec<String>,
}

fn user_config(s: &State) -> Option<PathBuf> {
    s.home.as_ref().map(|h| Target::UserConfig.path(&s.repo, h))
}

fn chosen(s: &State) -> Target {
    s.review
        .target
        .unwrap_or_else(|| match (&s.pipeline.path, user_config(s)) {
            (Some(p), Some(u)) if *p == u => Target::UserConfig,
            _ => Target::Repo,
        })
}

fn target_path(s: &State, t: Target) -> Result<PathBuf, String> {
    match t {
        Target::Repo => Ok(Target::Repo.path(&s.repo, std::path::Path::new(""))),
        Target::UserConfig => user_config(s)
            .ok_or_else(|| "HOME is not set, so there is no user config folder.".into()),
    }
}

/// Recompute the change from the form and the target. Cheap: one small file is read.
pub fn refresh(s: &mut State) {
    let mut r = std::mem::take(&mut s.review);
    r.change = None;
    r.problems.clear();
    if let Some(msg) = &s.pipeline.broken {
        r.problems.push(format!("The existing igris.toml cannot be used ({msg}). Start from the defaults on the Pipeline screen first."));
    } else if let Some(form) = &s.pipeline.form {
        match form.to_config() {
            Err(errs) => r.problems.extend(errs.into_iter().map(|e| e.message)),
            Ok(cfg) => match target_path(s, r.target.unwrap_or_else(|| chosen(s))) {
                Err(e) => r.problems.push(e),
                Ok(path) => match plan_change(&path, cfg.to_toml()) {
                    Ok(c) => r.change = Some(c),
                    Err(e) => r
                        .problems
                        .push(format!("Cannot read {}: {e}", path.display())),
                },
            },
        }
    } else {
        r.problems
            .push("The Pipeline form is not ready yet. Wait for the scan to finish.".into());
    }
    let max = r
        .change
        .as_ref()
        .map_or(0, |c| c.diff.len().saturating_sub(1));
    r.scroll = r.scroll.min(max);
    s.review = r;
}

/// `(lines added, lines removed)` of a change.
pub fn counts(c: &Change) -> (usize, usize) {
    c.diff.iter().fold((0, 0), |(a, d), l| match l {
        DiffLine::Add(_) => (a + 1, d),
        DiffLine::Del(_) => (a, d + 1),
        DiffLine::Same(_) => (a, d),
    })
}

pub fn update(s: &mut State, a: Action) -> Effect {
    let last = s
        .review
        .change
        .as_ref()
        .map_or(0, |c| c.diff.len().saturating_sub(1));
    match a {
        Action::Up => s.review.scroll = s.review.scroll.saturating_sub(1),
        Action::Down => s.review.scroll = (s.review.scroll + 1).min(last),
        Action::PageUp => s.review.scroll = s.review.scroll.saturating_sub(10),
        Action::PageDown => s.review.scroll = (s.review.scroll + 10).min(last),
        Action::Home => s.review.scroll = 0,
        Action::End => s.review.scroll = last,
        Action::NextField | Action::PrevField => s.review.focus = 1 - s.review.focus,
        Action::Select(i) => {
            s.review.focus = i.min(1);
            return activate(s);
        }
        Action::Activate | Action::Toggle => return activate(s),
        _ => {}
    }
    Effect::None
}

fn activate(s: &mut State) -> Effect {
    if s.review.focus == 0 {
        let other = match chosen(s) {
            Target::Repo => Target::UserConfig,
            Target::UserConfig => Target::Repo,
        };
        s.review.target = Some(other);
        s.review.scroll = 0;
        return Effect::None;
    }
    match &s.review.change {
        Some(c) if c.existing.as_deref() == Some(c.new.as_str()) => {
            s.toast = Some("Nothing to apply: the file already matches.".into());
            Effect::None
        }
        Some(c) => Effect::Write(c.clone()),
        None => {
            s.toast = Some("Fix the problems listed here first.".into());
            Effect::None
        }
    }
}

impl State {
    /// The result of writing the file. On success the form counts as saved.
    pub fn finish_apply(&mut self, result: Result<Applied, String>) {
        match result {
            Ok(done) => {
                self.pipeline.saved = self.pipeline.form.clone();
                self.pipeline.path = Some(done.path.clone());
                self.dirty = false;
                self.applied = true;
                // A file inside the repository makes it dirty, and runs refuse a dirty repository.
                let commit = if done.path.starts_with(&self.repo) {
                    " Commit it before you run a task."
                } else {
                    ""
                };
                self.toast = Some(match done.backup {
                    Some(b) => format!(
                        "Wrote {}.{commit} The old file is kept as {}; do not commit that copy.",
                        done.path.display(),
                        b.display()
                    ),
                    None => format!("Wrote {}.{commit}", done.path.display()),
                });
            }
            Err(e) => self.toast = Some(format!("Could not write: {e}. Nothing was changed.")),
        }
        refresh(self);
    }
}

pub fn draw(f: &mut Frame, area: Rect, s: &State, t: &Theme, hits: &mut HitMap, clickable: bool) {
    let r = &s.review;
    let target = chosen(s);
    let ascii = t.glyphs == Glyphs::Ascii;
    let width = area.width as usize;
    let dim = t.style(Role::Dim);
    let bold = Modifier::BOLD;
    let focus_mark = |i: usize| {
        let m = match (r.focus == i, ascii) {
            (false, _) => " ",
            (true, false) => "▌",
            (true, true) => ">",
        };
        Span::styled(m, t.style(Role::Bad))
    };
    let at = |row: u16| Rect::new(area.x, area.y + row, area.width, 1);

    // Where the file goes: a two-way choice, then the full path.
    f.render_widget(Paragraph::new(Line::styled("Write to", dim)), at(0));
    let (on, off) = if ascii {
        ("(*)", "( )")
    } else {
        ("(●)", "( )")
    };
    let option = |label: &str, chosen: bool| {
        let style = if chosen {
            t.style(Role::Text).add_modifier(bold)
        } else {
            dim
        };
        vec![
            Span::styled(if chosen { on } else { off }, style),
            Span::styled(format!(" {label}"), style),
        ]
    };
    let mut choice = vec![focus_mark(0), Span::raw(" ")];
    choice.extend(option("This repository", target == Target::Repo));
    choice.push(Span::raw("   "));
    choice.extend(option("Your user config", target == Target::UserConfig));
    f.render_widget(Paragraph::new(Line::from(choice)), at(1));
    let path = target_path(s, target)
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    f.render_widget(
        Paragraph::new(Line::styled(
            format!("  {}", fit_path(t, &path, width.saturating_sub(2))),
            dim,
        )),
        at(2),
    );
    if clickable {
        for row in 0..3 {
            hits.add(at(row), Action::Select(0));
        }
    }
    if area.height < 8 {
        return;
    }
    let button_row = at(area.height - 1);
    let body = Rect::new(area.x, area.y + 5, area.width, area.height - 7);
    let (button, note) = match &r.change {
        Some(c) => {
            let (add, del) = counts(c);
            let same = c.existing.as_deref() == Some(c.new.as_str());
            let sign = if ascii { "-" } else { "−" };
            let summary = if same {
                vec![Span::styled(
                    format!(
                        "{} No changes: the file already matches.",
                        if ascii { "[x]" } else { "✓" }
                    ),
                    t.style(Role::Ok),
                )]
            } else if c.existing.is_none() {
                vec![Span::styled(
                    format!("New file, {add} lines."),
                    t.style(Role::Info),
                )]
            } else {
                vec![
                    Span::styled(format!("+{add} added"), t.style(Role::Ok)),
                    Span::raw("   "),
                    Span::styled(format!("{sign}{del} removed"), t.style(Role::Bad)),
                    Span::styled("   The old file is kept as a .bak next to it.", dim),
                ]
            };
            f.render_widget(Paragraph::new(Line::from(summary)), at(4));
            for (row, l) in c
                .diff
                .iter()
                .skip(r.scroll)
                .take(body.height as usize)
                .enumerate()
            {
                let (mark, text, style) = match l {
                    DiffLine::Same(x) => (" ", x, dim),
                    DiffLine::Add(x) => ("+", x, t.style(Role::Ok)),
                    DiffLine::Del(x) => (if ascii { "-" } else { "−" }, x, t.style(Role::Bad)),
                };
                let line = Line::from(vec![
                    Span::styled(mark, style),
                    Span::raw(" "),
                    Span::styled(fit(t, text, width.saturating_sub(2)), style),
                ]);
                f.render_widget(
                    Paragraph::new(line),
                    Rect::new(body.x, body.y + row as u16, body.width, 1),
                );
            }
            if same {
                (button(t, "Nothing to apply", false, r.focus == 1), "")
            } else {
                (
                    button(t, "Apply", true, r.focus == 1),
                    "Enter applies; Tab moves between the two controls",
                )
            }
        }
        None => {
            let mut lines = vec![Line::styled(
                "Nothing to apply yet",
                t.style(Role::Text).add_modifier(bold),
            )];
            lines.extend(r.problems.iter().map(|p| {
                Line::styled(
                    format!(
                        "{} {}",
                        if ascii { "[!]" } else { "●" },
                        fit(t, p, width * 3)
                    ),
                    t.style(Role::Bad),
                )
            }));
            lines.push(Line::raw(""));
            lines.push(Line::styled(
                "Fix this on the Pipeline screen (key 2).",
                dim,
            ));
            f.render_widget(
                Paragraph::new(lines).wrap(Wrap { trim: false }),
                Rect::new(area.x, area.y + 4, area.width, area.height - 6),
            );
            (
                button(t, "Apply", false, r.focus == 1),
                "not available until this is fixed",
            )
        }
    };
    let mut row = vec![focus_mark(1), Span::raw(" "), button];
    if !note.is_empty() {
        row.push(Span::styled(format!("  {note}"), dim));
    }
    f.render_widget(Paragraph::new(Line::from(row)), button_row);
    if clickable {
        hits.add(button_row, Action::Select(1));
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
    use std::fs;

    fn tmp(name: &str) -> PathBuf {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let d =
            std::env::temp_dir().join(format!("igris-review-{}-{n}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    /// The app on the Review screen, for a repo holding this `igris.toml` (or none).
    fn on_review(toml: Option<&str>) -> State {
        let (repo, home) = (tmp("repo"), tmp("home"));
        if let Some(t) = toml {
            fs::write(repo.join("igris.toml"), t).unwrap();
        }
        let mut s = State::new((100, 30));
        s.repo = repo;
        s.home = Some(home);
        s.apply(Msg::Detected(Box::new(real())));
        app_update(&mut s, Action::Goto(Screen::Review));
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
    fn without_a_check_command_there_is_nothing_to_apply_and_it_says_why() {
        let mut s = on_review(None);
        assert!(s.review.change.is_none());
        assert!(
            s.review.problems[0].starts_with("checks.command is required"),
            "{:?}",
            s.review.problems
        );
        s.review.focus = 1;
        assert_eq!(app_update(&mut s, Action::Activate), Effect::None);
        assert!(s.toast.as_deref().unwrap().contains("Fix the problems"));
        let (text, _) = render(&s);
        assert!(
            text.contains("Nothing to apply yet") && text.contains("checks.command is required"),
            "{text}"
        );
    }

    #[test]
    fn a_valid_form_shows_the_diff_against_the_existing_file_and_applies_it() {
        let mut s = on_review(Some("[checks]\ncommand = \"make test\"\n"));
        let c = s.review.change.clone().expect("a change");
        assert!(
            c.existing.is_some()
                && c.diff
                    .iter()
                    .any(|l| matches!(l, DiffLine::Add(x) if x.starts_with("builder")))
        );
        s.pipeline.form.as_mut().unwrap().base = crate::tui::input::Input::new("trunk");
        crate::tui::pipeline_screen::refresh(&mut s);
        app_update(&mut s, Action::NextField); // focus the button
        let Effect::Write(change) = app_update(&mut s, Action::Activate) else {
            panic!("no write")
        };
        assert!(change.new.contains("base = \"trunk\""));
        assert_eq!(change.path, s.repo.join("igris.toml"));
        assert!(s.dirty, "unsaved until the write is done");
        let done = crate::tui::apply::apply(&change);
        s.finish_apply(done.map_err(|e| e.to_string()));
        assert!(!s.dirty && s.applied);
        assert!(
            fs::read_to_string(s.repo.join("igris.toml"))
                .unwrap()
                .contains("base = \"trunk\"")
        );
        assert!(
            s.repo.join("igris.toml.bak").is_file(),
            "the old file is kept"
        );
        assert_eq!(next_step(&s), Step::RunTest);
        let toast = s.toast.as_deref().unwrap();
        assert!(
            toast.contains("igris.toml.bak") && toast.contains("Commit it before you run a task"),
            "{toast}"
        );
        assert_eq!(
            app_update(&mut s, Action::Activate),
            Effect::None,
            "applying again is a no-op"
        );
        assert!(s.toast.as_deref().unwrap().contains("Nothing to apply"));
    }

    #[test]
    fn the_target_switches_between_the_repository_and_the_user_config() {
        let mut s = on_review(Some("[checks]\ncommand = \"t\"\n"));
        let repo_file = s.repo.join("igris.toml");
        assert_eq!(s.review.change.as_ref().unwrap().path, repo_file);
        app_update(&mut s, Action::Activate); // focus 0: switch
        let home_file = s.home.as_ref().unwrap().join(".config/igris/igris.toml");
        assert_eq!(s.review.change.as_ref().unwrap().path, home_file);
        assert_eq!(
            s.review.change.as_ref().unwrap().existing,
            None,
            "a new file there"
        );
        let (text, hits) = render(&s);
        assert!(
            text.contains("user config") && text.contains(".config/igris/igris.toml"),
            "{text}"
        );
        assert!(hits.at(0, 0) == Some(Action::Select(0)));
    }

    #[test]
    fn the_diff_is_drawn_with_plus_and_minus_and_scrolls() {
        let mut s = on_review(Some("[checks]\ncommand = \"t\"\n"));
        let (text, hits) = render(&s);
        assert!(text.contains("+ ") && text.contains("added"), "{text}");
        assert_eq!(
            hits.at(0, 23),
            Some(Action::Select(1)),
            "the button row is clickable"
        );
        app_update(&mut s, Action::End);
        assert_eq!(
            s.review.scroll,
            s.review.change.as_ref().unwrap().diff.len() - 1
        );
        app_update(&mut s, Action::Home);
        assert_eq!(s.review.scroll, 0);
    }

    #[test]
    fn a_broken_file_blocks_apply_until_the_user_starts_over() {
        let s = on_review(Some("[checks]\ncommand = \"t\"\nbogus = 1\n"));
        assert!(s.review.change.is_none());
        assert!(
            s.review.problems[0].contains("cannot be used"),
            "{:?}",
            s.review.problems
        );
        assert_eq!(
            fs::read_to_string(s.repo.join("igris.toml"))
                .unwrap()
                .matches("bogus")
                .count(),
            1
        );
    }

    #[test]
    fn a_failed_write_changes_nothing_and_says_so() {
        let mut s = on_review(None);
        s.pipeline.form.as_mut().unwrap().command = crate::tui::input::Input::new("t");
        crate::tui::pipeline_screen::refresh(&mut s);
        assert!(s.dirty && !s.applied);
        s.finish_apply(Err("permission denied".into()));
        assert!(s.dirty && !s.applied, "still unsaved");
        assert!(s.toast.as_deref().unwrap().contains("Nothing was changed"));
    }

    fn render_with(s: &State, t: &Theme, w: u16, h: u16) -> (String, ratatui::buffer::Buffer) {
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        let mut hits = HitMap::default();
        term.draw(|f| draw(f, f.area(), s, t, &mut hits, true))
            .unwrap();
        let buf = term.backend().buffer().clone();
        let text = (0..h)
            .map(|y| {
                (0..w)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        (text, buf)
    }

    fn truecolor() -> Theme {
        Theme {
            depth: crate::tui::theme::ColorDepth::True,
            glyphs: Glyphs::Unicode,
            motion: false,
        }
    }

    fn find(text: &str, needle: &str) -> (u16, u16) {
        let (y, l) = text
            .lines()
            .enumerate()
            .find(|(_, l)| l.contains(needle))
            .unwrap_or_else(|| panic!("{needle}:\n{text}"));
        (
            l[..l.find(needle).unwrap()].chars().count() as u16,
            y as u16,
        )
    }

    #[test]
    fn the_target_is_a_two_way_choice_that_swaps_when_you_pick_the_other() {
        let mut s = on_review(Some("[checks]\ncommand = \"t\"\n"));
        let (text, _) = render_with(&s, &truecolor(), 100, 24);
        assert!(
            text.contains("(●) This repository") && text.contains("( ) Your user config"),
            "{text}"
        );
        app_update(&mut s, Action::Activate);
        let (text, _) = render_with(&s, &truecolor(), 100, 24);
        assert!(
            text.contains("( ) This repository") && text.contains("(●) Your user config"),
            "{text}"
        );
    }

    #[test]
    fn the_summary_counts_added_and_removed_lines_and_the_diff_is_coloured_by_role() {
        let s = on_review(Some("[checks]\ncommand = \"t\"\n"));
        let (text, buf) = render_with(&s, &truecolor(), 100, 24);
        assert!(text.contains("added") && text.contains("removed"), "{text}");
        let (x, y) = find(&text, "+ [agents]");
        assert_eq!(
            buf[(x, y)].fg,
            ratatui::style::Color::Rgb(0xbc, 0xfa, 0xf1),
            "an added line is glow"
        );
        let (x, y) = find(&text, "+");
        assert_eq!(buf[(x, y)].fg, ratatui::style::Color::Rgb(0xbc, 0xfa, 0xf1));
        // Without colour the plus and minus signs still tell them apart.
        let plain = Theme {
            depth: crate::tui::theme::ColorDepth::None,
            glyphs: Glyphs::Ascii,
            motion: false,
        };
        let (text, buf) = render_with(&s, &plain, 100, 24);
        assert!(text.contains("+ [agents]") && text.is_ascii(), "{text}");
        let (x, y) = find(&text, "+ [agents]");
        assert_eq!(buf[(x, y)].fg, ratatui::style::Color::Reset);
    }

    #[test]
    fn apply_is_the_red_primary_button_only_when_there_is_something_to_apply() {
        let s = on_review(Some("[checks]\ncommand = \"t\"\n"));
        let (text, buf) = render_with(&s, &truecolor(), 100, 24);
        let (x, y) = find(&text, "Apply");
        assert_eq!(
            buf[(x, y)].bg,
            ratatui::style::Color::Rgb(0xff, 0x30, 0x38),
            "the primary button is red"
        );

        // After applying, the file matches: the button says so and is no longer primary.
        let mut s = s;
        app_update(&mut s, Action::NextField);
        let Effect::Write(change) = app_update(&mut s, Action::Activate) else {
            panic!()
        };
        s.finish_apply(crate::tui::apply::apply(&change).map_err(|e| e.to_string()));
        let (text, buf) = render_with(&s, &truecolor(), 100, 24);
        let (x, y) = find(&text, "Nothing to apply");
        assert_ne!(buf[(x, y)].bg, ratatui::style::Color::Rgb(0xff, 0x30, 0x38));
        assert!(
            text.contains("No changes: the file already matches."),
            "{text}"
        );

        // With a problem there is no primary button, and the problem is listed.
        let s = on_review(None);
        let (text, buf) = render_with(&s, &truecolor(), 100, 24);
        let (x, y) = find(&text, "Apply");
        assert_ne!(buf[(x, y)].bg, ratatui::style::Color::Rgb(0xff, 0x30, 0x38));
        assert!(
            text.contains("Nothing to apply yet")
                && text.contains("Fix this on the Pipeline screen (key 2)."),
            "{text}"
        );
    }
}
