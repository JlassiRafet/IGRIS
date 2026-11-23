//! The Pipeline screen: `igris.toml` as a form. Four essentials first, the rest under Advanced. Enter or
//! a click on a field opens a framed input, picker or list box.
use super::action::Action;
use super::form::{ListEditor, NumberInput, Select};
use super::hit::HitMap;
use super::input::{Input, InputEvent};
use super::pipeline::{FieldError, Loaded, PipelineForm, load_for_form};
use super::state::{Effect, State};
use super::style::Role;
use super::theme::{Glyphs, Theme};
use super::widgets::{detail_strip, fit, pad, panel};
use igris_core::config::Config;
use igris_detect::Snapshot;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph, Wrap};
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
    /// Essentials first, then the rest: the order of the screen.
    pub const ALL: [Field; 16] = [
        Field::Builder,
        Field::Reviewer,
        Field::Command,
        Field::Base,
        Field::BuilderArgs,
        Field::ReviewerArgs,
        Field::ChecksTimeout,
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

    /// What a first-time user must look at; everything else is under "Advanced".
    pub fn is_essential(self) -> bool {
        matches!(
            self,
            Field::Builder | Field::Reviewer | Field::Command | Field::Base
        )
    }

    pub fn label(self) -> &'static str {
        match self {
            Field::Builder => "Builder agent",
            Field::Reviewer => "Reviewer agent",
            Field::Command => "Check command",
            Field::Base => "Base branch",
            Field::BuilderArgs => "Builder arguments",
            Field::ReviewerArgs => "Reviewer arguments",
            Field::ChecksTimeout => "Check timeout (s)",
            Field::RiskyPaths => "Risky paths",
            Field::RiskyWords => "Risky words",
            Field::MaxReviewRounds => "Review rounds",
            Field::MaxFixRounds => "Fix rounds",
            Field::BuildTimeout => "Build timeout (s)",
            Field::Backend => "Decision layer",
            Field::Threshold => "Confidence threshold",
            Field::HerdrBin => "Herdr program",
            Field::HerdrMachine => "Herdr machine",
        }
    }

    pub fn help(self) -> &'static str {
        match self {
            Field::Builder => "Writes the code. Agents found on this machine are listed first.",
            Field::Reviewer => "Reviews the work, read-only. A different agent catches more.",
            Field::Command => {
                "Proves the work is done. Runs in its own pane; exit code 0 means pass. Example: cargo test"
            }
            Field::Base => "IGRIS branches from this committed branch.",
            Field::BuilderArgs => "Extra arguments for the builder agent.",
            Field::ReviewerArgs => "The default keeps the reviewer read-only.",
            Field::ChecksTimeout => "Seconds before the check command is stopped.",
            Field::RiskyPaths => "A task that touches these globs waits for your yes.",
            Field::RiskyWords => "A task whose text has these words waits for your yes.",
            Field::MaxReviewRounds => "How many times the reviewer may ask for changes.",
            Field::MaxFixRounds => "How many fix attempts after a failed check.",
            Field::BuildTimeout => "Seconds before the builder is stopped.",
            Field::Backend => "Rules for now. Jev and Laya arrive later.",
            Field::Threshold => "At or above this IGRIS acts; below it asks you.",
            Field::HerdrBin => "The Herdr command to run.",
            Field::HerdrMachine => "A saved SSH machine label; empty means this computer.",
        }
    }

    /// Shown dim in an empty input.
    pub fn placeholder(self) -> &'static str {
        match self {
            Field::Command => "e.g. cargo test",
            Field::Base => "e.g. main",
            Field::HerdrBin => "herdr",
            Field::HerdrMachine => "empty means this computer",
            _ => "",
        }
    }

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

/// What can be focused: a field, or the fold that shows the rest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PItem {
    Field(Field),
    Advanced,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PipelineUi {
    /// The rest of the fields are shown (also opened by a problem in one of them).
    pub show_advanced: bool,
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
    /// The focusable items, top to bottom: the four essentials, the Advanced fold, and when it is
    /// open the other twelve fields.
    pub fn items(&self) -> Vec<PItem> {
        let mut v: Vec<PItem> = Field::ALL
            .iter()
            .filter(|f| f.is_essential())
            .map(|f| PItem::Field(*f))
            .collect();
        v.push(PItem::Advanced);
        if self.show_advanced {
            v.extend(
                Field::ALL
                    .iter()
                    .filter(|f| !f.is_essential())
                    .map(|f| PItem::Field(*f)),
            );
        }
        v
    }

    /// A problem in a hidden field must not stay hidden.
    fn open_for_errors(&mut self) {
        let hidden = |e: &FieldError| {
            Field::ALL
                .iter()
                .any(|f| !f.is_essential() && f.key() == e.field)
        };
        if self.errors.iter().any(hidden) {
            self.show_advanced = true;
        }
        self.focus = self.focus.min(self.items().len() - 1);
    }

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
        self.open_for_errors();
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
    let Some(f) = &s.pipeline.form else {
        return;
    };
    let errors = f.to_config().err().unwrap_or_default();
    let dirty = s.pipeline.saved.as_ref() != Some(f);
    let command_set = !f.command.value().trim().is_empty();
    s.pipeline.errors = errors;
    s.pipeline.open_for_errors();
    s.dirty = dirty;
    s.command_set = command_set;
}

pub fn update(s: &mut State, a: Action) -> Effect {
    let ui = &mut s.pipeline;
    if ui.edit.is_some() {
        match (a, &mut ui.edit) {
            (Action::Cancel, _) => ui.edit = None,
            // A click on an item of an open picker chooses it.
            (Action::Select(i), Some(Edit::Pick(_, p))) => {
                if p.choose(i) {
                    if let (Some(done), Some(f)) = (ui.edit.take(), &mut ui.form) {
                        commit(f, done);
                    }
                    refresh(s);
                }
            }
            _ => {}
        }
        return Effect::None;
    }
    let last = ui.items().len() - 1;
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
    match ui.items().get(ui.focus).copied() {
        Some(PItem::Advanced) => ui.show_advanced = !ui.show_advanced,
        Some(PItem::Field(field)) => {
            if let Some(form) = &mut ui.form {
                ui.edit = Some(begin(form, field));
            }
        }
        None => {}
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

/// The problems the field being edited would have if the edit were saved now.
fn live_errors(form: &PipelineForm, edit: &Edit) -> Vec<String> {
    let key = match edit {
        Edit::Text(f, _) | Edit::Num(f, _) | Edit::Pick(f, _) => f.key(),
        Edit::List { field, .. } => field.key(),
    };
    let mut f = form.clone();
    commit(&mut f, edit.clone());
    f.to_config()
        .err()
        .unwrap_or_default()
        .into_iter()
        .filter(|e| e.field == key)
        .map(|e| e.message)
        .collect()
}

/// Draw the form in `area` (the inside of the panel). Each field line is clickable when `clickable`.
pub fn draw(f: &mut Frame, area: Rect, s: &State, t: &Theme, hits: &mut HitMap, clickable: bool) {
    let ui = &s.pipeline;
    if let Some(msg) = &ui.broken {
        let text = format!(
            "The existing igris.toml cannot be used:\n  {}\n\nNothing was changed. Press Enter to start from the defaults; \
             the old file is kept as igris.toml.bak when you apply.",
            fit(t, msg, 600)
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
    let width = area.width as usize;
    let ascii = t.glyphs == Glyphs::Ascii;
    let dim = t.style(Role::Dim);
    let items = ui.items();
    let mut lines: Vec<(Line<'static>, Option<usize>)> = vec![(
        Line::styled("Essentials", dim.add_modifier(Modifier::BOLD)),
        None,
    )];
    let label_w = 22;
    let value_w = width.saturating_sub(1 + 1 + label_w + 1 + 2);
    for (i, item) in items.iter().enumerate() {
        let selected = i == ui.focus && ui.edit.is_none();
        let marker = Span::styled(
            match (selected, ascii) {
                (false, _) => " ",
                (true, false) => "▌",
                (true, true) => ">",
            },
            t.style(Role::Bad),
        );
        match item {
            PItem::Advanced => {
                if i > 0 {
                    lines.push((Line::raw(""), None));
                }
                let fold = match (ui.show_advanced, ascii) {
                    (true, false) => "▾",
                    (false, false) => "▸",
                    (true, true) => "v",
                    (false, true) => ">",
                };
                let more = if ui.show_advanced {
                    String::new()
                } else {
                    format!("  ({} more)", Field::ALL.len() - 4)
                };
                lines.push((
                    Line::from(vec![
                        marker,
                        Span::styled(fold, t.style(Role::Link)),
                        Span::raw(" "),
                        Span::styled("Advanced", t.style(Role::Link).add_modifier(Modifier::BOLD)),
                        Span::styled(more, dim),
                    ]),
                    Some(i),
                ));
            }
            PItem::Field(fl) => {
                let value = value_text(form, *fl);
                let (shown, vstyle) = if value.is_empty() {
                    ("(not set)".to_string(), dim)
                } else {
                    (fit(t, &value, value_w), t.style(Role::Text))
                };
                let has_err = ui.errors.iter().any(|e| e.field == fl.key());
                let label_style = t.style(Role::Text).add_modifier(if selected {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                });
                lines.push((
                    Line::from(vec![
                        marker,
                        Span::raw(" "),
                        Span::styled(pad(t, fl.label(), label_w), label_style),
                        Span::raw(" "),
                        Span::styled(pad(t, &shown, value_w), vstyle),
                        Span::styled(if has_err { " !" } else { "  " }, t.style(Role::Bad)),
                    ]),
                    Some(i),
                ));
                if *fl == Field::Reviewer && form.same_agent() {
                    lines.push((
                        Line::styled(
                            format!("    {}", fit(t, "The builder and the reviewer are the same agent; a different reviewer catches more.", width.saturating_sub(4))),
                            t.style(Role::Info),
                        ),
                        None,
                    ));
                }
                for e in ui.errors.iter().filter(|e| e.field == fl.key()) {
                    lines.push((
                        Line::styled(
                            format!("    ! {}", fit(t, &e.message, width.saturating_sub(6))),
                            t.style(Role::Bad),
                        ),
                        None,
                    ));
                }
            }
        }
    }
    for e in ui
        .errors
        .iter()
        .filter(|e| !Field::ALL.iter().any(|f| f.key() == e.field))
    {
        lines.push((
            Line::styled(
                format!("! {}", fit(t, &e.message, width.saturating_sub(2))),
                t.style(Role::Bad),
            ),
            None,
        ));
    }
    let strip_h = if area.height >= 14 { 4 } else { 0 };
    let list = Rect::new(
        area.x,
        area.y,
        area.width,
        area.height.saturating_sub(strip_h),
    );
    let focus = lines
        .iter()
        .position(|(_, i)| *i == Some(ui.focus))
        .unwrap_or(0);
    let first = (focus + 1).saturating_sub(list.height as usize);
    for (row, (line, item)) in lines
        .iter()
        .skip(first)
        .take(list.height as usize)
        .enumerate()
    {
        let r = Rect::new(list.x, list.y + row as u16, list.width, 1);
        f.render_widget(Paragraph::new(line.clone()), r);
        if let (true, Some(i), None) = (clickable, item, &ui.edit) {
            hits.add(r, Action::Select(*i));
        }
    }
    if strip_h > 0 {
        let strip = Rect::new(area.x, area.bottom() - strip_h, area.width, strip_h);
        f.render_widget(detail_strip(t, strip_lines(t, ui, width)), strip);
    }
    if ui.edit.is_some() {
        draw_edit(f, s, t, hits);
    }
}

/// The focused field's name, key and help, and its problems in full.
fn strip_lines(t: &Theme, ui: &PipelineUi, width: usize) -> Vec<Line<'static>> {
    match ui.items().get(ui.focus) {
        Some(PItem::Field(fl)) => {
            let mut v = vec![
                Line::from(vec![
                    Span::styled(fl.label(), t.style(Role::Text).add_modifier(Modifier::BOLD)),
                    Span::styled(format!("  {}", fl.key()), t.style(Role::Dim)),
                ]),
                Line::styled(fl.help(), t.style(Role::Dim)),
            ];
            v.extend(
                ui.errors
                    .iter()
                    .filter(|e| e.field == fl.key())
                    .map(|e| Line::styled(fit(t, &e.message, width * 2), t.style(Role::Bad))),
            );
            v
        }
        Some(PItem::Advanced) => vec![
            Line::styled("Advanced", t.style(Role::Text).add_modifier(Modifier::BOLD)),
            Line::styled(
                "Arguments, limits, risky paths and words, the decision layer and Herdr. The defaults are fine to start.",
                t.style(Role::Dim),
            ),
        ],
        None => vec![],
    }
}

/// The edit box: a framed input, picker or list in the middle of the window. A click outside it
/// closes it (cancel); a click inside keeps it open.
fn draw_edit(f: &mut Frame, s: &State, t: &Theme, hits: &mut HitMap) {
    let ui = &s.pipeline;
    let (Some(edit), Some(form)) = (&ui.edit, &ui.form) else {
        return;
    };
    let frame = f.area();
    let cursor = if t.glyphs == Glyphs::Ascii {
        "|"
    } else {
        "▏"
    };
    let prompt = if t.glyphs == Glyphs::Ascii {
        "> "
    } else {
        "❯ "
    };
    let dim = t.style(Role::Dim);
    let mut lines: Vec<Line<'static>> = vec![];
    let mut item_rows: Vec<(usize, usize)> = vec![]; // (line index, item number) for a picker
    let title;
    let input_line = |text: String, placeholder: &str| {
        let shown = if text == cursor {
            vec![
                Span::styled(prompt, t.style(Role::Link)),
                Span::styled(cursor, t.style(Role::Link)),
                Span::styled(placeholder.to_string(), dim),
            ]
        } else {
            vec![
                Span::styled(prompt, t.style(Role::Link)),
                Span::styled(text, t.style(Role::Text)),
            ]
        };
        Line::from(shown)
    };
    match edit {
        Edit::Text(field, inp) => {
            title = field.label();
            lines.push(input_line(with_cursor(inp, cursor), field.placeholder()));
            lines.push(Line::raw(""));
            lines.push(Line::styled(field.help(), dim));
        }
        Edit::Num(field, n) => {
            title = field.label();
            lines.push(input_line(
                format!("{}{cursor}", n.text()),
                field.placeholder(),
            ));
            lines.push(Line::raw(""));
            lines.push(Line::styled(field.help(), dim));
        }
        Edit::Pick(field, p) => {
            title = field.label();
            let window = 10usize;
            let start = (p.index + 1).saturating_sub(window);
            for (n, it) in p.items.iter().enumerate().skip(start).take(window) {
                let mark = if n == p.index {
                    if t.glyphs == Glyphs::Ascii {
                        ">"
                    } else {
                        "▌"
                    }
                } else {
                    " "
                };
                let style = if it.enabled { t.style(Role::Text) } else { dim };
                let note = it
                    .note
                    .as_deref()
                    .map(|x| format!("  {x}"))
                    .unwrap_or_default();
                item_rows.push((lines.len(), n));
                lines.push(Line::from(vec![
                    Span::styled(mark, t.style(Role::Bad)),
                    Span::raw(" "),
                    Span::styled(
                        it.label.clone(),
                        style.add_modifier(if n == p.index {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                    ),
                    Span::styled(note, dim),
                ]));
            }
            lines.push(Line::raw(""));
            lines.push(Line::styled(field.help(), dim));
        }
        Edit::List {
            field,
            list,
            sel,
            input,
        } => {
            title = field.label();
            if list.items.is_empty() {
                lines.push(Line::styled("(none yet)", dim));
            }
            for (n, it) in list.items.iter().enumerate().take(8) {
                let mark = if n == *sel {
                    if t.glyphs == Glyphs::Ascii {
                        ">"
                    } else {
                        "▌"
                    }
                } else {
                    " "
                };
                lines.push(Line::from(vec![
                    Span::styled(mark, t.style(Role::Bad)),
                    Span::raw(" "),
                    Span::styled(fit(t, it, 60), t.style(Role::Text)),
                ]));
            }
            if let Some((_, inp)) = input {
                lines.push(Line::raw(""));
                lines.push(input_line(with_cursor(inp, cursor), "type an item"));
            }
            lines.push(Line::raw(""));
            lines.push(Line::styled(field.help(), dim));
        }
    }
    for e in live_errors(form, edit) {
        lines.push(Line::styled(
            format!("! {}", fit(t, &e, 120)),
            t.style(Role::Bad),
        ));
    }
    lines.push(Line::raw(""));
    let hint = match edit {
        Edit::Pick(..) => "Up/Down choose · Enter select · Esc cancel",
        Edit::List { input: Some(_), .. } => "Enter keep · Esc back",
        Edit::List { .. } => "a add · Enter edit · d remove · Esc done",
        _ => "Enter save · Esc cancel",
    };
    lines.push(Line::styled(
        if t.glyphs == Glyphs::Ascii {
            hint.replace('·', "-")
        } else {
            hint.to_string()
        },
        dim,
    ));
    let w = 70.min(frame.width.saturating_sub(4));
    // Long lines wrap inside the border and its padding, so count the rows they really take.
    let inner = usize::from(w.saturating_sub(4)).max(1);
    let rows: usize = lines.iter().map(|l| l.width().div_ceil(inner).max(1)).sum();
    let h = (rows as u16 + 2).min(frame.height.saturating_sub(2));
    let r = Rect::new(
        frame.x + (frame.width - w) / 2,
        frame.y + (frame.height - h) / 2,
        w,
        h,
    );
    hits.add(frame, Action::Cancel);
    hits.add(r, Action::Stay);
    for (line, n) in item_rows {
        let y = r.y + 1 + line as u16;
        if y < r.bottom() - 1 {
            hits.add(Rect::new(r.x + 1, y, r.width - 2, 1), Action::Select(n));
        }
    }
    f.render_widget(Clear, r);
    f.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(panel(t, title, true)),
        r,
    );
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
        if !f.is_essential() {
            s.pipeline.show_advanced = true;
        }
        s.pipeline.focus = s
            .pipeline
            .items()
            .iter()
            .position(|i| *i == PItem::Field(f))
            .unwrap();
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
            text.contains("Check command") && text.contains("! checks.command is required"),
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
            .find(|y| hits.at(2, *y) == Some(Action::Select(2)))
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

    fn flat(text: &str) -> String {
        text.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    #[test]
    fn the_first_view_is_four_essentials_and_a_fold_for_the_rest() {
        let s = on_pipeline(None);
        let (text, _) = render(&s, 100, 30);
        assert!(text.contains("Essentials"), "{text}");
        for label in [
            "Builder agent",
            "Reviewer agent",
            "Check command",
            "Base branch",
        ] {
            assert!(text.contains(label), "{label}:\n{text}");
        }
        assert!(
            text.contains("Advanced") && text.contains("(12 more)"),
            "{text}"
        );
        for hidden in ["Risky paths", "Herdr program", "Decision layer"] {
            assert!(!text.contains(hidden), "{hidden} should be folded:\n{text}");
        }
        assert_eq!(s.pipeline.items().len(), 5);
    }

    #[test]
    fn enter_on_advanced_unfolds_the_other_twelve_and_again_folds_them() {
        let mut s = on_pipeline(None);
        s.pipeline.focus = 4;
        app_update(&mut s, Action::Activate);
        assert!(s.pipeline.show_advanced && s.pipeline.items().len() == 17);
        let (text, _) = render(&s, 100, 40);
        assert!(
            text.contains("Risky paths") && text.contains("Herdr program"),
            "{text}"
        );
        app_update(&mut s, Action::Toggle);
        assert!(!s.pipeline.show_advanced);
    }

    #[test]
    fn a_problem_inside_advanced_opens_it_and_marks_the_row() {
        let s = on_pipeline(Some("[checks]\ncommand = \"t\"\ntimeout_s = 0\n"));
        assert!(s.pipeline.show_advanced, "the hidden problem is shown");
        let (text, _) = render(&s, 100, 40);
        let row = text
            .lines()
            .find(|l| l.contains("Check timeout"))
            .expect("the row");
        assert!(row.trim_end().ends_with('!'), "{row:?}");
        assert!(flat(&text).contains("must be above 0"), "{text}");
    }

    #[test]
    fn editing_a_text_field_opens_a_framed_input_with_a_placeholder_and_the_help() {
        let mut s = on_pipeline(None);
        focus(&mut s, Field::Command);
        app_update(&mut s, Action::Activate);
        let (text, _) = render(&s, 100, 30);
        assert!(
            text.contains("Check command")
                && text.contains("❯")
                && text.contains("e.g. cargo test"),
            "{text}"
        );
        assert!(flat(&text).contains("Proves the work is done."), "{text}");
        assert!(text.contains("Enter save · Esc cancel"), "{text}");
        assert_eq!(
            flat(&text).matches("checks.command is required").count(),
            3,
            "the list row, the strip and the box all say so while the value is empty"
        );
        type_text(&mut s, "make test");
        let (text, _) = render(&s, 100, 30);
        assert!(
            text.contains("❯ make test") && !text.contains("e.g. cargo test"),
            "{text}"
        );
        assert_eq!(
            flat(&text).matches("checks.command is required").count(),
            2,
            "the box stops complaining once the value is valid; the saved form still does"
        );
        press(&mut s, KeyCode::Esc);
        assert_eq!(
            s.pipeline.form.as_ref().unwrap().command.value(),
            "",
            "Esc changes nothing"
        );
    }

    #[test]
    fn a_click_outside_the_edit_box_cancels_and_a_click_inside_keeps_it() {
        let mut s = on_pipeline(None);
        focus(&mut s, Field::Command);
        app_update(&mut s, Action::Activate);
        let (_, hits) = render(&s, 100, 30);
        assert_eq!(hits.at(0, 0), Some(Action::Cancel));
        assert_eq!(hits.at(50, 15), Some(Action::Stay));
    }

    #[test]
    fn the_picker_lists_detected_agents_first_and_soon_items_stay_dim_and_unchoosable() {
        let mut s = on_pipeline(None);
        focus(&mut s, Field::Reviewer);
        app_update(&mut s, Action::Activate);
        let (text, _) = render(&s, 100, 30);
        assert!(
            text.contains("Reviewer agent") && text.contains("logged in"),
            "{text}"
        );
        press(&mut s, KeyCode::Esc);
        focus(&mut s, Field::Backend);
        app_update(&mut s, Action::Activate);
        let (text, _) = render(&s, 100, 40);
        assert!(
            text.contains("jev") && text.contains("Soon") && text.contains("laya"),
            "{text}"
        );
        press(&mut s, KeyCode::Down);
        press(&mut s, KeyCode::Enter);
        assert_eq!(
            value_text(s.pipeline.form.as_ref().unwrap(), Field::Backend),
            "rules"
        );
    }

    #[test]
    fn a_click_on_a_picker_item_chooses_it() {
        let mut s = on_pipeline(None);
        focus(&mut s, Field::Reviewer);
        app_update(&mut s, Action::Activate);
        let (text, hits) = render(&s, 100, 30);
        let (y, line) = text
            .lines()
            .enumerate()
            .filter(|(_, l)| l.contains("claude"))
            .last()
            .expect("claude is listed");
        let x = line[..line.find("claude").unwrap()].chars().count() as u16;
        let a = hits.at(x, y as u16).expect("the item is clickable");
        assert!(matches!(a, Action::Select(_)));
        app_update(&mut s, a);
        assert_eq!(s.pipeline.edit, None, "choosing closes the box");
        assert!(
            value_text(s.pipeline.form.as_ref().unwrap(), Field::Reviewer).starts_with("claude")
        );
    }

    #[test]
    fn the_list_box_names_its_keys() {
        let mut s = on_pipeline(None);
        focus(&mut s, Field::RiskyWords);
        app_update(&mut s, Action::Activate);
        let (text, _) = render(&s, 100, 40);
        assert!(
            text.contains("a add · Enter edit · d remove · Esc done"),
            "{text}"
        );
        press(&mut s, KeyCode::Char('a'));
        let (text, _) = render(&s, 100, 40);
        assert!(
            text.contains("Enter keep · Esc back") && text.contains("type an item"),
            "{text}"
        );
    }

    #[test]
    fn the_strip_shows_the_focused_fields_key_and_help() {
        let mut s = on_pipeline(None);
        focus(&mut s, Field::Command);
        let (text, _) = render(&s, 100, 30);
        assert!(
            text.contains("checks.command") && flat(&text).contains("exit code 0 means pass"),
            "{text}"
        );
        focus(&mut s, Field::Builder);
        let (text, _) = render(&s, 100, 30);
        assert!(flat(&text).contains("Writes the code."), "{text}");
    }

    #[test]
    fn at_80x24_the_essentials_the_advanced_row_and_the_strip_all_fit() {
        let mut s = on_pipeline(None);
        s.screen = crate::tui::state::Screen::Pipeline;
        let mut term = Terminal::new(TestBackend::new(80, 24)).unwrap();
        let mut hits = HitMap::default();
        let t = Theme::detect(&Env::default());
        term.draw(|f| crate::tui::draw(f, &s, &t, &mut hits))
            .unwrap();
        let buf = term.backend().buffer().clone();
        let text = (0..24u16)
            .map(|y| {
                (0..80u16)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        for want in [
            "Essentials",
            "Builder agent",
            "Check command",
            "Base branch",
            "Advanced",
        ] {
            assert!(text.contains(want), "{want}:\n{text}");
        }
        assert!(
            flat(&text).contains("Writes the code."),
            "the strip fits at 80x24:\n{text}"
        );
    }
}
