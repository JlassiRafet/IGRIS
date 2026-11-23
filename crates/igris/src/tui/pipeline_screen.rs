//! The Pipeline screen: `igris.toml` as a form. Enter or a click edits the focused field in place.
use super::action::Action;
use super::form::{ListEditor, NumberInput, Select};
use super::hit::HitMap;
use super::input::{Input, InputEvent};
use super::pipeline::{FieldError, Loaded, PipelineForm, load_for_form};
use super::state::{Effect, State};
use super::theme::{Glyphs, Theme};
use igris_core::config::Config;
use igris_detect::Snapshot;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::{Paragraph, Wrap};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Builder,
    Reviewer,
    BuilderArgs,
    ReviewerArgs,
    Command,
    ChecksTimeout,
    Base,
    RiskyPaths,
    RiskyWords,
    MaxReviewRounds,
    MaxFixRounds,
    BuildTimeout,
    Backend,
    Threshold,
    HerdrBin,
    HerdrMachine,
}

impl Field {
    pub const ALL: [Field; 16] = [
        Field::Builder,
        Field::Reviewer,
        Field::BuilderArgs,
        Field::ReviewerArgs,
        Field::Command,
        Field::ChecksTimeout,
        Field::Base,
        Field::RiskyPaths,
        Field::RiskyWords,
        Field::MaxReviewRounds,
        Field::MaxFixRounds,
        Field::BuildTimeout,
        Field::Backend,
        Field::Threshold,
        Field::HerdrBin,
        Field::HerdrMachine,
    ];

    /// The `igris.toml` key: shown as the label and matched against error fields.
    pub fn key(self) -> &'static str {
        match self {
            Field::Builder => "agents.builder",
            Field::Reviewer => "agents.reviewer",
            Field::BuilderArgs => "agents.builder_args",
            Field::ReviewerArgs => "agents.reviewer_args",
            Field::Command => "checks.command",
            Field::ChecksTimeout => "checks.timeout_s",
            Field::Base => "policy.base",
            Field::RiskyPaths => "policy.risky_paths",
            Field::RiskyWords => "policy.risky_words",
            Field::MaxReviewRounds => "policy.max_review_rounds",
            Field::MaxFixRounds => "policy.max_fix_rounds",
            Field::BuildTimeout => "policy.build_timeout_s",
            Field::Backend => "decision.backend",
            Field::Threshold => "decision.threshold",
            Field::HerdrBin => "herdr.bin",
            Field::HerdrMachine => "herdr.machine",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Edit {
    Text(Field, Input),
    Num(Field, NumberInput),
    Pick(Field, Select),
    List {
        field: Field,
        list: ListEditor,
        sel: usize,
        /// An item being typed: the index it replaces, or None for a new one.
        input: Option<(Option<usize>, Input)>,
    },
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PipelineUi {
    pub form: Option<PipelineForm>,
    /// The form as it was loaded or last applied: the form differs from it when there are changes.
    pub saved: Option<PipelineForm>,
    /// Why the existing file cannot be used. Nothing is written until the user chooses to start over.
    pub broken: Option<String>,
    pub focus: usize,
    pub edit: Option<Edit>,
    pub errors: Vec<FieldError>,
    /// The file the form was loaded from.
    pub path: Option<PathBuf>,
}

impl PipelineUi {
    pub fn load(&mut self, repo: &std::path::Path, snap: &Snapshot) -> bool {
        match load_for_form(repo, snap) {
            Loaded::Existing(c, path) => {
                self.path = Some(path);
                self.set(&c, snap);
                true
            }
            Loaded::Fresh(c) => {
                self.set(&c, snap);
                false
            }
            Loaded::Broken(msg) => {
                self.broken = Some(msg);
                false
            }
        }
    }

    fn set(&mut self, c: &Config, snap: &Snapshot) {
        let f = PipelineForm::from_config(c, snap);
        self.saved = Some(f.clone());
        self.form = Some(f);
        self.errors = self
            .form
            .as_ref()
            .and_then(|f| f.to_config().err())
            .unwrap_or_default();
    }

    /// Drop the unusable file's contents and start from the defaults and this machine's suggestions.
    pub fn start_fresh(&mut self, snap: &Snapshot) {
        self.broken = None;
        let c = Config {
            agents: super::pipeline::suggest(snap),
            ..Config::default()
        };
        self.set(&c, snap);
    }
}

pub fn value_text(f: &PipelineForm, field: Field) -> String {
    let sel = |s: &Select| {
        s.current().map_or(String::new(), |i| match &i.note {
            Some(n) => format!("{} ({n})", i.label),
            None => i.label.clone(),
        })
    };
    let list = |l: &ListEditor| {
        if l.items.is_empty() {
            "(none)".to_string()
        } else {
            l.items.join(", ")
        }
    };
    match field {
        Field::Builder => sel(&f.builder),
        Field::Reviewer => sel(&f.reviewer),
        Field::BuilderArgs => list(&f.builder_args),
        Field::ReviewerArgs => f.reviewer_args.as_ref().map_or_else(
            || "(default: read-only for the chosen reviewer)".to_string(),
            list,
        ),
        Field::Command => f.command.value().to_string(),
        Field::ChecksTimeout => f.checks_timeout.text().to_string(),
        Field::Base => f.base.value().to_string(),
        Field::RiskyPaths => list(&f.risky_paths),
        Field::RiskyWords => list(&f.risky_words),
        Field::MaxReviewRounds => f.max_review_rounds.text().to_string(),
        Field::MaxFixRounds => f.max_fix_rounds.text().to_string(),
        Field::BuildTimeout => f.build_timeout.text().to_string(),
        Field::Backend => sel(&f.backend),
        Field::Threshold => f.threshold.text().to_string(),
        Field::HerdrBin => f.herdr_bin.value().to_string(),
        Field::HerdrMachine => f.herdr_machine.value().to_string(),
    }
}

fn begin(f: &mut PipelineForm, field: Field) -> Edit {
    let list = |field, l: &ListEditor| Edit::List {
        field,
        list: l.clone(),
        sel: 0,
        input: None,
    };
    match field {
        Field::Builder => Edit::Pick(field, f.builder.clone()),
        Field::Reviewer => Edit::Pick(field, f.reviewer.clone()),
        Field::Backend => Edit::Pick(field, f.backend.clone()),
        Field::BuilderArgs => list(field, &f.builder_args),
        Field::ReviewerArgs => {
            let current = f.reviewer_args.clone().unwrap_or_else(|| {
                // Start from what the default means for the chosen reviewer.
                let kind = f
                    .reviewer
                    .current()
                    .map(|i| i.label.clone())
                    .unwrap_or_default();
                let a = igris_core::config::Agents {
                    reviewer: kind,
                    ..Default::default()
                };
                ListEditor {
                    items: a.reviewer_args(),
                }
            });
            list(field, &current)
        }
        Field::RiskyPaths => list(field, &f.risky_paths),
        Field::RiskyWords => list(field, &f.risky_words),
        Field::Command => Edit::Text(field, f.command.clone()),
        Field::Base => Edit::Text(field, f.base.clone()),
        Field::HerdrBin => Edit::Text(field, f.herdr_bin.clone()),
        Field::HerdrMachine => Edit::Text(field, f.herdr_machine.clone()),
        Field::ChecksTimeout => Edit::Num(field, f.checks_timeout.clone()),
        Field::MaxReviewRounds => Edit::Num(field, f.max_review_rounds.clone()),
        Field::MaxFixRounds => Edit::Num(field, f.max_fix_rounds.clone()),
        Field::BuildTimeout => Edit::Num(field, f.build_timeout.clone()),
        Field::Threshold => Edit::Num(field, f.threshold.clone()),
    }
}

fn commit(f: &mut PipelineForm, e: Edit) {
    match e {
        Edit::Text(Field::Command, i) => f.command = i,
        Edit::Text(Field::Base, i) => f.base = i,
        Edit::Text(Field::HerdrBin, i) => f.herdr_bin = i,
        Edit::Text(Field::HerdrMachine, i) => f.herdr_machine = i,
        Edit::Num(Field::ChecksTimeout, n) => f.checks_timeout = n,
        Edit::Num(Field::MaxReviewRounds, n) => f.max_review_rounds = n,
        Edit::Num(Field::MaxFixRounds, n) => f.max_fix_rounds = n,
        Edit::Num(Field::BuildTimeout, n) => f.build_timeout = n,
        Edit::Num(Field::Threshold, n) => f.threshold = n,
        Edit::Pick(Field::Builder, s) => f.builder = s,
        Edit::Pick(Field::Reviewer, s) => f.reviewer = s,
        Edit::Pick(Field::Backend, s) => f.backend = s,
        Edit::List {
            field: Field::BuilderArgs,
            list,
            ..
        } => f.builder_args = list,
        Edit::List {
            field: Field::ReviewerArgs,
            list,
            ..
        } => f.reviewer_args = Some(list),
        Edit::List {
            field: Field::RiskyPaths,
            list,
            ..
        } => f.risky_paths = list,
        Edit::List {
            field: Field::RiskyWords,
            list,
            ..
        } => f.risky_words = list,
        _ => {}
    }
}

/// Recompute what depends on the form: its problems, unsaved changes, and the next-step flags.
pub fn refresh(s: &mut State) {
    let ui = &mut s.pipeline;
    let Some(f) = &ui.form else { return };
    ui.errors = f.to_config().err().unwrap_or_default();
    s.dirty = ui.saved.as_ref() != Some(f);
    s.command_set = !f.command.value().trim().is_empty();
}

pub fn update(s: &mut State, a: Action) -> Effect {
    let ui = &mut s.pipeline;
    if ui.edit.is_some() {
        if a == Action::Cancel {
            ui.edit = None;
        }
        return Effect::None;
    }
    let last = Field::ALL.len() - 1;
    match a {
        Action::Up | Action::PrevField => ui.focus = ui.focus.saturating_sub(1),
        Action::Down | Action::NextField => ui.focus = (ui.focus + 1).min(last),
        Action::PageUp => ui.focus = ui.focus.saturating_sub(8),
        Action::PageDown => ui.focus = (ui.focus + 8).min(last),
        Action::Home => ui.focus = 0,
        Action::End => ui.focus = last,
        Action::Select(i) => {
            ui.focus = i.min(last);
            return activate(s);
        }
        Action::Activate | Action::Toggle => return activate(s),
        _ => {}
    }
    Effect::None
}

fn activate(s: &mut State) -> Effect {
    let ui = &mut s.pipeline;
    if ui.broken.is_some() {
        // The file stays as it is until Review writes (and backs it up).
        let snap = s.snapshot.clone();
        if let Some(snap) = snap {
            ui.start_fresh(&snap);
            refresh(s);
        }
        return Effect::None;
    }
    if let Some(form) = &mut ui.form {
        ui.edit = Some(begin(form, Field::ALL[ui.focus]));
    }
    Effect::None
}

/// Keys while a field is being edited. Everything but Ctrl+C (the caller's) comes here.
pub fn edit_key(s: &mut State, key: KeyEvent) {
    let Some(edit) = s.pipeline.edit.take() else {
        return;
    };
    let (next, done) = step(edit, key);
    s.pipeline.edit = next;
    if let (Some(done), Some(f)) = (done, &mut s.pipeline.form) {
        commit(f, done);
    }
    refresh(s);
}

pub fn paste(s: &mut State, text: &str) {
    match &mut s.pipeline.edit {
        Some(Edit::Text(_, i))
        | Some(Edit::List {
            input: Some((_, i)),
            ..
        }) => i.paste(text),
        Some(Edit::Num(_, n)) => n.paste(text),
        _ => {}
    }
}

/// One key for one edit. Returns the edit to keep (None when closed) and the edit to commit.
fn step(edit: Edit, key: KeyEvent) -> (Option<Edit>, Option<Edit>) {
    match edit {
        Edit::Text(f, mut i) => match i.handle(key) {
            InputEvent::Commit => (None, Some(Edit::Text(f, i))),
            InputEvent::Cancel => (None, None),
            _ => (Some(Edit::Text(f, i)), None),
        },
        Edit::Num(f, mut n) => match n.handle(key) {
            InputEvent::Commit => (None, Some(Edit::Num(f, n))),
            InputEvent::Cancel => (None, None),
            _ => (Some(Edit::Num(f, n)), None),
        },
        Edit::Pick(f, mut p) => match key.code {
            KeyCode::Enter => {
                // A "Soon" item cannot be chosen: the picker stays open.
                let at = p.index;
                if p.choose(at) {
                    (None, Some(Edit::Pick(f, p)))
                } else {
                    (Some(Edit::Pick(f, p)), None)
                }
            }
            KeyCode::Esc => (None, None),
            _ => {
                p.handle(key);
                (Some(Edit::Pick(f, p)), None)
            }
        },
        Edit::List {
            field,
            mut list,
            mut sel,
            input,
        } => {
            if let Some((at, mut i)) = input {
                match i.handle(key) {
                    InputEvent::Commit => {
                        let text = i.value().trim().to_string();
                        match at {
                            _ if text.is_empty() => {}
                            Some(n) => {
                                list.edit(n, &text);
                            }
                            None => {
                                list.add(&text);
                                sel = list.items.len() - 1;
                            }
                        }
                        (
                            Some(Edit::List {
                                field,
                                list,
                                sel,
                                input: None,
                            }),
                            None,
                        )
                    }
                    InputEvent::Cancel => (
                        Some(Edit::List {
                            field,
                            list,
                            sel,
                            input: None,
                        }),
                        None,
                    ),
                    _ => (
                        Some(Edit::List {
                            field,
                            list,
                            sel,
                            input: Some((at, i)),
                        }),
                        None,
                    ),
                }
            } else {
                let last = list.items.len().saturating_sub(1);
                let plain = key.modifiers.is_empty();
                let mut input = None;
                match key.code {
                    KeyCode::Esc => {
                        return (
                            None,
                            Some(Edit::List {
                                field,
                                list,
                                sel,
                                input: None,
                            }),
                        );
                    }
                    KeyCode::Up | KeyCode::Char('k') if plain => sel = sel.saturating_sub(1),
                    KeyCode::Down | KeyCode::Char('j') if plain => sel = (sel + 1).min(last),
                    KeyCode::Char('a') if plain => input = Some((None, Input::new(""))),
                    KeyCode::Enter | KeyCode::Char('e') if plain => {
                        if let Some(item) = list.items.get(sel) {
                            input = Some((Some(sel), Input::new(item)));
                        }
                    }
                    KeyCode::Char('d') | KeyCode::Delete if plain => {
                        list.remove(sel);
                        sel = sel.min(list.items.len().saturating_sub(1));
                    }
                    _ => {}
                }
                (
                    Some(Edit::List {
                        field,
                        list,
                        sel,
                        input,
                    }),
                    None,
                )
            }
        }
    }
}

/// Draw the form. Each field line is clickable when `clickable`.
pub fn draw(f: &mut Frame, area: Rect, s: &State, t: &Theme, hits: &mut HitMap, clickable: bool) {
    let ui = &s.pipeline;
    if let Some(msg) = &ui.broken {
        let text = format!(
            "The existing igris.toml cannot be used:\n  {msg}\n\nNothing was changed. Press Enter to start from the defaults; \
             the old file is kept as igris.toml.bak when you apply."
        );
        f.render_widget(Paragraph::new(text).wrap(Wrap { trim: false }), area);
        if clickable {
            hits.add(area, Action::Select(0));
        }
        return;
    }
    let Some(form) = &ui.form else {
        f.render_widget(
            Paragraph::new("Waiting for the scan of this machine…"),
            area,
        );
        return;
    };
    let ascii = t.glyphs == Glyphs::Ascii;
    let cursor = if ascii { "|" } else { "▏" };
    let mut lines: Vec<(String, Option<usize>)> = vec![];
    if form.same_agent() {
        lines.push(("Note: the builder and the reviewer are the same agent; a different reviewer catches more.".into(), None));
    }
    for (i, field) in Field::ALL.iter().enumerate() {
        let marker = if i == ui.focus { ">" } else { " " };
        let editing = ui.edit.as_ref().filter(|_| i == ui.focus);
        let value = match editing {
            Some(Edit::Text(_, inp)) => with_cursor(inp, cursor),
            Some(Edit::Num(_, n)) => format!("{}{cursor}", n.text()),
            _ => value_text(form, *field),
        };
        lines.push((format!("{marker} {:<26} {value}", field.key()), Some(i)));
        match editing {
            Some(Edit::Pick(_, p)) => {
                for (n, it) in p.items.iter().enumerate() {
                    let note = it
                        .note
                        .as_deref()
                        .map(|x| format!(" ({x})"))
                        .unwrap_or_default();
                    let mark = if n == p.index { ">" } else { " " };
                    lines.push((format!("      {mark} {}{note}", it.label), None));
                }
                lines.push((
                    "      Up/Down choose, Enter commits, Esc cancels".into(),
                    None,
                ));
            }
            Some(Edit::List {
                list, sel, input, ..
            }) => {
                for (n, it) in list.items.iter().enumerate() {
                    let mark = if n == *sel { ">" } else { " " };
                    lines.push((format!("      {mark} {it}"), None));
                }
                if let Some((_, inp)) = input {
                    lines.push((
                        format!("      new text: {}", with_cursor(inp, cursor)),
                        None,
                    ));
                }
                lines.push(("      a add, Enter edit, d remove, Esc done".into(), None));
            }
            _ => {}
        }
        for e in ui.errors.iter().filter(|e| e.field == field.key()) {
            lines.push((format!("      ! {}", e.message), None));
        }
    }
    for e in ui
        .errors
        .iter()
        .filter(|e| !Field::ALL.iter().any(|f| f.key() == e.field))
    {
        lines.push((format!("  ! {}", e.message), None));
    }
    let focus = lines
        .iter()
        .position(|(_, i)| *i == Some(ui.focus))
        .unwrap_or(0);
    let first = (focus + 1).saturating_sub(area.height as usize);
    for (row, (text, item)) in lines
        .iter()
        .skip(first)
        .take(area.height as usize)
        .enumerate()
    {
        let r = Rect::new(area.x, area.y + row as u16, area.width, 1);
        let style = if *item == Some(ui.focus) {
            Style::new().bold()
        } else {
            Style::new()
        };
        f.render_widget(Paragraph::new(text.clone()).style(style), r);
        if let (true, Some(i), None) = (clickable, item, &ui.edit) {
            hits.add(r, Action::Select(*i));
        }
    }
}

fn with_cursor(i: &Input, mark: &str) -> String {
    let at = i
        .value()
        .char_indices()
        .nth(i.cursor())
        .map_or(i.value().len(), |(b, _)| b);
    format!("{}{mark}{}", &i.value()[..at], &i.value()[at..])
}

/// Is a field being edited? The app then sends every key here.
pub fn editing(s: &State) -> bool {
    s.pipeline.edit.is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::view::tests::real;
    use crate::tui::detect_worker::Msg;
    use crate::tui::state::{Overlay, update as app_update};
    use crate::tui::theme::Env;
    use ratatui::crossterm::event::KeyModifiers;
    use ratatui::{Terminal, backend::TestBackend};

    fn tmp(name: &str) -> PathBuf {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let d =
            std::env::temp_dir().join(format!("igris-pscreen-{}-{n}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A state on the Pipeline screen, loaded from a repo with this file text (or none).
    fn on_pipeline(toml: Option<&str>) -> State {
        let repo = tmp("repo");
        if let Some(t) = toml {
            std::fs::write(repo.join("igris.toml"), t).unwrap();
        }
        let mut s = State::new((100, 30));
        s.repo = repo;
        s.apply(Msg::Detected(Box::new(real())));
        s.screen = crate::tui::state::Screen::Pipeline;
        s
    }

    fn press(s: &mut State, code: KeyCode) {
        edit_key(s, KeyEvent::new(code, KeyModifiers::NONE));
    }

    fn type_text(s: &mut State, text: &str) {
        for c in text.chars() {
            press(s, KeyCode::Char(c));
        }
    }

    fn render(s: &State, w: u16, h: u16) -> (String, HitMap) {
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        let mut hits = HitMap::default();
        let t = Theme::detect(&Env::default());
        term.draw(|f| draw(f, f.area(), s, &t, &mut hits, true))
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
        (text, hits)
    }

    fn focus(s: &mut State, f: Field) {
        s.pipeline.focus = Field::ALL.iter().position(|x| *x == f).unwrap();
    }

    #[test]
    fn the_form_starts_from_the_first_scan_with_suggestions_and_the_missing_command() {
        let s = on_pipeline(None);
        let f = s.pipeline.form.as_ref().unwrap();
        assert_eq!(value_text(f, Field::Builder), "claude (logged in)");
        assert_eq!(value_text(f, Field::Reviewer), "codex (logged in)");
        assert!(!s.dirty && !s.command_set && !s.applied);
        assert_eq!(s.pipeline.errors.len(), 1);
        let (text, _) = render(&s, 100, 30);
        assert!(
            text.contains("checks.command") && text.contains("! checks.command is required"),
            "{text}"
        );
    }

    #[test]
    fn editing_the_check_command_updates_the_form_and_clears_the_error() {
        let mut s = on_pipeline(None);
        focus(&mut s, Field::Command);
        app_update(&mut s, Action::Activate);
        assert!(matches!(
            s.pipeline.edit,
            Some(Edit::Text(Field::Command, _))
        ));
        type_text(&mut s, "cargo test");
        press(&mut s, KeyCode::Enter);
        assert_eq!(s.pipeline.edit, None);
        assert_eq!(
            s.pipeline.form.as_ref().unwrap().command.value(),
            "cargo test"
        );
        assert!(s.dirty && s.command_set);
        assert!(s.pipeline.errors.is_empty(), "{:?}", s.pipeline.errors);
    }

    #[test]
    fn escape_leaves_the_value_as_it_was() {
        let mut s = on_pipeline(None);
        focus(&mut s, Field::Command);
        app_update(&mut s, Action::Activate);
        type_text(&mut s, "x");
        press(&mut s, KeyCode::Esc);
        assert_eq!(s.pipeline.form.as_ref().unwrap().command.value(), "");
        assert!(!s.dirty);
    }

    #[test]
    fn a_picker_changes_the_agent_and_cannot_land_on_soon() {
        let mut s = on_pipeline(None);
        focus(&mut s, Field::Reviewer);
        app_update(&mut s, Action::Activate);
        press(&mut s, KeyCode::Down);
        press(&mut s, KeyCode::Enter);
        let f = s.pipeline.form.as_ref().unwrap();
        assert_ne!(value_text(f, Field::Reviewer), "codex (logged in)");
        assert!(s.dirty);

        focus(&mut s, Field::Backend);
        app_update(&mut s, Action::Activate);
        press(&mut s, KeyCode::Down);
        press(&mut s, KeyCode::Down);
        press(&mut s, KeyCode::Enter);
        assert_eq!(
            value_text(s.pipeline.form.as_ref().unwrap(), Field::Backend),
            "rules"
        );
    }

    #[test]
    fn a_number_field_refuses_letters_and_checks_the_range() {
        let mut s = on_pipeline(Some("[checks]\ncommand = \"t\"\n"));
        focus(&mut s, Field::Threshold);
        app_update(&mut s, Action::Activate);
        for _ in 0..4 {
            press(&mut s, KeyCode::Backspace);
        }
        type_text(&mut s, "x7");
        press(&mut s, KeyCode::Enter);
        assert_eq!(s.pipeline.form.as_ref().unwrap().threshold.text(), "7");
        assert!(
            s.pipeline
                .errors
                .iter()
                .any(|e| e.field == "decision.threshold"),
            "{:?}",
            s.pipeline.errors
        );
    }

    #[test]
    fn a_list_can_gain_an_item_and_lose_one() {
        let mut s = on_pipeline(None);
        focus(&mut s, Field::RiskyWords);
        let before = s.pipeline.form.as_ref().unwrap().risky_words.items.len();
        app_update(&mut s, Action::Activate);
        press(&mut s, KeyCode::Char('a'));
        type_text(&mut s, "payment");
        press(&mut s, KeyCode::Enter);
        press(&mut s, KeyCode::Char('d'));
        press(&mut s, KeyCode::Esc);
        let items = &s.pipeline.form.as_ref().unwrap().risky_words.items;
        assert_eq!(
            items.len(),
            before,
            "one added, the selected (the new one) removed"
        );
        press_none(&mut s);
        assert!(s.pipeline.edit.is_none());
    }

    fn press_none(_: &mut State) {}

    #[test]
    fn reviewer_args_start_as_the_default_and_become_explicit_once_edited() {
        let mut s = on_pipeline(None);
        focus(&mut s, Field::ReviewerArgs);
        assert!(
            value_text(s.pipeline.form.as_ref().unwrap(), Field::ReviewerArgs)
                .starts_with("(default")
        );
        app_update(&mut s, Action::Activate);
        let Some(Edit::List { list, .. }) = &s.pipeline.edit else {
            panic!()
        };
        assert_eq!(
            list.items,
            ["--sandbox", "read-only"],
            "what the default means for codex"
        );
        press(&mut s, KeyCode::Esc);
        assert!(s.pipeline.form.as_ref().unwrap().reviewer_args.is_some());
    }

    #[test]
    fn a_click_on_a_field_starts_editing_it() {
        let mut s = on_pipeline(None);
        let (_, hits) = render(&s, 100, 30);
        let row = (0..30u16)
            .find(|y| hits.at(2, *y) == Some(Action::Select(4)))
            .expect("the command line is clickable");
        app_update(&mut s, hits.at(2, row).unwrap());
        assert!(matches!(
            s.pipeline.edit,
            Some(Edit::Text(Field::Command, _))
        ));
    }

    #[test]
    fn an_unusable_file_is_shown_untouched_and_starting_over_is_a_choice() {
        let mut s = on_pipeline(Some("[checks]\ncommand = \"t\"\nbogus = 1\n"));
        assert!(s.pipeline.form.is_none());
        let (text, _) = render(&s, 100, 30);
        let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(
            flat.contains("cannot be used") && flat.contains("unknown field `bogus`"),
            "a long path may wrap the message:\n{text}"
        );
        assert_eq!(
            std::fs::read_to_string(s.repo.join("igris.toml"))
                .unwrap()
                .matches("bogus")
                .count(),
            1,
            "file untouched"
        );
        app_update(&mut s, Action::Activate);
        assert!(s.pipeline.form.is_some() && s.pipeline.broken.is_none());
    }

    #[test]
    fn an_existing_valid_file_counts_as_applied() {
        let s = on_pipeline(Some("[checks]\ncommand = \"make test\"\n"));
        assert!(s.applied && s.command_set && !s.dirty);
        assert!(s.pipeline.errors.is_empty());
    }

    #[test]
    fn quitting_with_unsaved_edits_asks_first() {
        let mut s = on_pipeline(None);
        focus(&mut s, Field::Command);
        app_update(&mut s, Action::Activate);
        type_text(&mut s, "x");
        press(&mut s, KeyCode::Enter);
        app_update(&mut s, Action::Quit);
        assert_eq!(
            s.overlay,
            Some(Overlay::Confirm(crate::tui::state::Confirm::Discard))
        );
    }
}
