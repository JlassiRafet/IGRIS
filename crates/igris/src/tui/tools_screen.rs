//! The Tools step: the five groups of tools IGRIS can set up, their pick lists and honest status
//! words. This file is the state and the keys; `tools_draw` draws it.
use super::action::Action;
use super::form::{Select, SelectItem};
use super::input::{Input, InputEvent};
use super::pipeline::PipelineForm;
use super::state::{Effect, Screen, State};
use crate::catalog::{Group, TOOLS, Tool, tool, tools_in};
use igris_core::config::KINDS;
use igris_detect::fit::Report;
use igris_detect::{Login, Snapshot};
use ratatui::crossterm::event::KeyEvent;
use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

/// The groups in the order the step walks them. Agents run inside Herdr, so it comes first.
pub const GROUPS: [Group; 5] = [
    Group::Workspace,
    Group::Builder,
    Group::Reviewer,
    Group::Decision,
    Group::Assistant,
];

/// The row number of a click on a group's name (any other number is a row of that group).
pub const HEADER: usize = 999;
/// The id of the Assistant group's "no assistant" answer.
pub const CLI_ONLY: &str = "cli-only";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolStatus {
    Ready,
    InstalledLoginUnverified,
    NeedsStep,
    Checking,
    Skipped,
    NotInstalled,
    NotSupportedYet,
    NotSelectableYet,
}

impl ToolStatus {
    /// The words the spec fixes, used on every screen and in `igris doctor`.
    pub fn word(self) -> &'static str {
        match self {
            ToolStatus::Ready => "ready",
            ToolStatus::InstalledLoginUnverified => "installed · login unverified",
            ToolStatus::NeedsStep => "needs a step",
            ToolStatus::Checking => "checking…",
            ToolStatus::Skipped => "skipped",
            ToolStatus::NotInstalled => "not installed",
            ToolStatus::NotSupportedYet => "not supported yet",
            ToolStatus::NotSelectableYet => "not selectable yet",
        }
    }

    /// Shown dimmed: nothing can be done with the row yet.
    pub fn dim(self) -> bool {
        matches!(
            self,
            ToolStatus::NotInstalled | ToolStatus::NotSupportedYet | ToolStatus::NotSelectableYet
        )
    }
}

/// What the user answered for the assistant. Session state only: no `igris.toml` key holds it,
/// and "connected" is read from the scan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssistantChoice {
    Unset,
    Tool(&'static str),
    CliOnly,
}

/// A live check of one tool, filled in by the connect and verify steps.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CheckState {
    Pending,
    Running,
    Passed { ms: u64 },
    Failed(Failure),
    Skipped,
}

/// The failure box: why, what was left as it was, and the exact command to fix it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    pub reason: String,
    pub unchanged: String,
    pub fix: Option<String>,
}

/// An install started from the Tools step.
#[derive(Clone, Debug, PartialEq)]
pub enum Install {
    Running(Instant),
    /// The installer exited 0 with this output; the next scan says whether the tool is there.
    Done(Vec<String>),
    Failed(Failure),
}

/// A model download started from the Assistant group.
#[derive(Clone, Debug, PartialEq)]
pub enum Pull {
    Running(Instant),
    Failed(String),
}

/// What is typed into: the filter of the focused list, or Herdr's machine label.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edit {
    Filter,
    Machine,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ToolsUi {
    pub group: Group,
    /// The focused row of each group, indexed like `GROUPS`.
    pub cursor: [usize; 5],
    pub filter: Input,
    pub edit: Option<Edit>,
    pub machine: Input,
    pub assistant: AssistantChoice,
    pub checks: BTreeMap<String, CheckState>,
    /// Tools found on `PATH` that the scan does not know (OpenClaw).
    pub found: BTreeSet<&'static str>,
    /// The key being typed in the connect step: masked, and emptied the moment it is taken.
    pub key: Input,
    /// When each running check started, for its latency.
    pub started: BTreeMap<&'static str, Instant>,
    /// The tool whose key is being handed to its store command.
    pub storing: Option<&'static str>,
    /// Which local models fit this machine, from the fit adapter (arrives after the scan).
    pub fit: Option<Report>,
    /// Installs, by tool id.
    pub installs: BTreeMap<&'static str, Install>,
    /// Model downloads, by Ollama tag.
    pub pulls: BTreeMap<&'static str, Pull>,
}

impl Default for ToolsUi {
    fn default() -> ToolsUi {
        ToolsUi {
            group: Group::Workspace,
            cursor: [0; 5],
            filter: Input::default(),
            edit: None,
            machine: Input::default(),
            assistant: AssistantChoice::Unset,
            checks: BTreeMap::new(),
            found: BTreeSet::new(),
            key: Input::masked(),
            started: BTreeMap::new(),
            storing: None,
            fit: None,
            installs: BTreeMap::new(),
            pulls: BTreeMap::new(),
        }
    }
}

/// One line of a pick list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub id: &'static str,
    pub name: &'static str,
    pub status: ToolStatus,
    /// How it connects, in IGRIS's own words: the dim bracket of the focused row.
    pub connects: &'static str,
    /// What IGRIS suggests: the first row of a group that has a default.
    pub default: bool,
    /// What is chosen now.
    pub chosen: bool,
    /// False for a dimmed row: it cannot be picked.
    pub pickable: bool,
    /// More lines for the detail strip (a local model's fit, size and pull command).
    pub detail: String,
    /// A local model: the place of its candidate in the fit report.
    pub model: Option<usize>,
}

pub fn index(g: Group) -> usize {
    GROUPS.iter().position(|x| *x == g).unwrap_or(0)
}

/// Whether the tool is on this machine, from the scan or the extra `PATH` look.
pub(super) fn installed(t: &Tool, snap: &Snapshot, ui: &ToolsUi) -> bool {
    match t.id {
        "rules" => true,
        "herdr" => snap.herdr.found,
        "hermes" => snap.hermes.found,
        id => snap.agents.iter().any(|a| a.kind == id && a.found) || ui.found.contains(id),
    }
}

/// Whether the scan read a login (or, for Hermes, a provider) for this tool.
pub fn login_read(t: &Tool, snap: &Snapshot) -> bool {
    match t.id {
        "hermes" => snap.hermes.provider.is_some(),
        id => snap
            .agents
            .iter()
            .any(|a| a.kind == id && a.login == Login::LoggedIn),
    }
}

/// The honest word for a tool. Nothing is `ready` unless a login was read or a check passed.
pub fn status_of(t: &Tool, snap: &Snapshot, ui: &ToolsUi) -> ToolStatus {
    use ToolStatus::*;
    if !t.selectable {
        return if t.groups.contains(&Group::Decision) {
            NotSelectableYet
        } else {
            NotSupportedYet
        };
    }
    match ui.installs.get(t.id) {
        Some(Install::Running(_) | Install::Done(_)) => return Checking,
        Some(Install::Failed(_)) => return NeedsStep,
        None => {}
    }
    match ui.checks.get(t.id) {
        Some(CheckState::Running) => return Checking,
        Some(CheckState::Passed { .. }) => return Ready,
        Some(CheckState::Failed(_)) => return NeedsStep,
        Some(CheckState::Skipped) => return Skipped,
        Some(CheckState::Pending) | None => {}
    }
    if !installed(t, snap, ui) {
        return NotInstalled;
    }
    match t.id {
        "rules" => Ready,
        "herdr" if snap.herdr.ok => Ready,
        "herdr" => NeedsStep,
        "hermes" if snap.hermes.provider.is_some() => Ready,
        "hermes" => NeedsStep,
        id => match snap.agents.iter().find(|a| a.kind == id).map(|a| a.login) {
            Some(Login::LoggedIn) => Ready,
            Some(Login::LoggedOut) => NeedsStep,
            _ => InstalledLoginUnverified,
        },
    }
}

/// The recommendation order: a static list, because nothing on the machine says which agent is
/// better. The builder prefers Claude Code, the reviewer prefers a different agent.
fn rank(g: Group, id: &str) -> usize {
    let order: &[&str] = match g {
        Group::Builder => &["claude", "codex"],
        Group::Reviewer => &["codex", "claude"],
        _ => &[],
    };
    order.iter().position(|x| *x == id).unwrap_or(order.len())
}

/// The rows of one group: the ones that can be picked and are on this machine first, then the
/// pickable ones that are not installed, then the dimmed ones.
pub fn all_rows(g: Group, snap: &Snapshot, ui: &ToolsUi, form: Option<&PipelineForm>) -> Vec<Row> {
    let picked = |sel: Option<&Select>| sel.and_then(|s| s.current()).map(|i| i.label.clone());
    let current = match g {
        Group::Builder => picked(form.map(|f| &f.builder)),
        Group::Reviewer => picked(form.map(|f| &f.reviewer)),
        Group::Decision => picked(form.map(|f| &f.backend)),
        _ => None,
    };
    let mut v: Vec<(usize, usize, usize, Row)> = tools_in(g)
        .into_iter()
        .enumerate()
        .map(|(i, t)| {
            let status = status_of(t, snap, ui);
            let class = match status {
                ToolStatus::NotSupportedYet | ToolStatus::NotSelectableYet => 2,
                ToolStatus::NotInstalled => 1,
                _ => 0,
            };
            let chosen = match g {
                Group::Workspace => t.id == "herdr",
                Group::Assistant => ui.assistant == AssistantChoice::Tool(t.id),
                _ => current.as_deref() == Some(t.id),
            };
            let row = Row {
                id: t.id,
                name: t.name,
                status,
                connects: t.connects,
                default: false,
                chosen,
                pickable: t.selectable,
                detail: String::new(),
                model: None,
            };
            (class, rank(g, t.id), i, row)
        })
        .collect();
    v.sort_by_key(|(class, rank, i, _)| (*class, *rank, *i));
    let mut out: Vec<Row> = v.into_iter().map(|(.., r)| r).collect();
    if g == Group::Assistant {
        // "No assistant" is an answer, listed after the assistants that can be picked.
        let at = out.iter().position(|r| !r.pickable).unwrap_or(out.len());
        out.insert(
            at,
            Row {
                id: CLI_ONLY,
                name: "CLI only",
                status: ToolStatus::Ready,
                connects: "no chat assistant: you start tasks from the terminal",
                default: false,
                chosen: ui.assistant == AssistantChoice::CliOnly,
                pickable: true,
                detail: String::new(),
                model: None,
            },
        );
        // Local models that fit this machine, after the answers a user can pick.
        out.splice(at + 1..at + 1, super::install::model_rows(snap, ui));
    } else if let Some(first) = out.first_mut().filter(|r| r.pickable) {
        first.default = true;
    }
    out
}

/// `all_rows`, narrowed by the filter when `g` is the focused group (the other list is not).
pub fn rows(g: Group, snap: &Snapshot, ui: &ToolsUi, form: Option<&PipelineForm>) -> Vec<Row> {
    let mut out = all_rows(g, snap, ui, form);
    let q = ui.filter.value().to_lowercase();
    if g == ui.group && !q.is_empty() {
        out.retain(|r| r.name.to_lowercase().contains(&q) || r.id.contains(&q));
    }
    out
}

/// The same agent in both roles: a second agent catches more, so the screen warns.
pub fn same_agent(form: &PipelineForm) -> bool {
    form.builder.current().map(|i| &i.label) == form.reviewer.current().map(|i| &i.label)
}

/// Every group is answered with something on this machine: Herdr, the builder and the reviewer
/// installed, and the assistant answered (CLI only counts).
pub fn tools_done(ui: &ToolsUi, form: &PipelineForm, snap: &Snapshot) -> bool {
    first_unsettled(ui, form, snap).is_none()
}

/// The first group, in the order the step walks them, that is not answered yet. The decision layer
/// is never unsettled: `rules` is built in.
pub fn first_unsettled(ui: &ToolsUi, form: &PipelineForm, snap: &Snapshot) -> Option<Group> {
    let has = |item: Option<&SelectItem>| {
        item.and_then(|i| tool(&i.label))
            .is_some_and(|t| installed(t, snap, ui))
    };
    if !snap.herdr.found {
        Some(Group::Workspace)
    } else if !has(form.builder.current()) {
        Some(Group::Builder)
    } else if !has(form.reviewer.current()) {
        Some(Group::Reviewer)
    } else if ui.assistant == AssistantChoice::Unset {
        Some(Group::Assistant)
    } else {
        None
    }
}

/// The catalog tools the scan has no field for: they need their own look at `PATH`.
pub fn extra_ids() -> Vec<&'static str> {
    TOOLS
        .iter()
        .filter(|t| t.selectable && !t.groups.is_empty())
        .filter(|t| !KINDS.contains(&t.id) && !matches!(t.id, "herdr" | "rules"))
        .map(|t| t.id)
        .collect()
}

/// Of `ids`, the ones `which` finds.
pub fn found_on_path(ids: &[&'static str], which: impl Fn(&str) -> bool) -> Vec<&'static str> {
    ids.iter().copied().filter(|id| which(id)).collect()
}

pub fn editing(s: &State) -> bool {
    s.screen == Screen::Tools && s.tools.edit.is_some()
}

const READ_ONLY: &str = "igris.toml cannot be read, so nothing is changed here. Fix it or start fresh on the Pipeline screen.";

pub fn update(s: &mut State, a: Action) -> Effect {
    if s.tools.edit.is_some() {
        // Keys go to `edit_key`; a click or Esc that reaches here only ends the edit.
        if a == Action::Cancel {
            end_edit(s, false);
        }
        return Effect::None;
    }
    let Some(snap) = &s.snapshot else {
        return Effect::None;
    };
    let g = index(s.tools.group);
    let len = rows(s.tools.group, snap, &s.tools, s.pipeline.form.as_ref())
        .len()
        .saturating_sub(1);
    let at = s.tools.cursor[g].min(len);
    match a {
        Action::Up => s.tools.cursor[g] = at.saturating_sub(1),
        Action::Down => s.tools.cursor[g] = (at + 1).min(len),
        Action::PageUp => s.tools.cursor[g] = at.saturating_sub(8),
        Action::PageDown => s.tools.cursor[g] = (at + 8).min(len),
        Action::Home => s.tools.cursor[g] = 0,
        Action::End => s.tools.cursor[g] = len,
        Action::NextField => focus(s, (g + 1) % GROUPS.len()),
        Action::PrevField => focus(s, (g + GROUPS.len() - 1) % GROUPS.len()),
        Action::Filter => s.tools.edit = Some(Edit::Filter),
        Action::Connect => return open_connect(s),
        Action::Cancel => clear_filter(s),
        Action::Select(n) => {
            let (group, row) = (n / 1000, n % 1000);
            if group >= GROUPS.len() {
                return Effect::None;
            }
            focus(s, group);
            if row == HEADER {
                return Effect::None;
            }
            s.tools.cursor[group] = row;
            return activate(s);
        }
        Action::Activate | Action::Toggle => return activate(s),
        _ => {}
    }
    Effect::None
}

fn clear_filter(s: &mut State) {
    s.tools.filter = Input::default();
    s.tools.cursor[index(s.tools.group)] = 0;
}

/// Move the focus to a group. The filter belongs to one list, so it is dropped.
fn focus(s: &mut State, group: usize) {
    if GROUPS[group] != s.tools.group {
        s.tools.group = GROUPS[group];
        clear_filter(s);
    }
}

/// Pick the focused row: write it to the form (Review writes the file), or say why not.
fn activate(s: &mut State) -> Effect {
    let Some(snap) = &s.snapshot else {
        return Effect::None;
    };
    let g = s.tools.group;
    let list = rows(g, snap, &s.tools, s.pipeline.form.as_ref());
    let Some(row) = list.get(s.tools.cursor[index(g)]).cloned() else {
        return Effect::None;
    };
    if let Some(i) = row.model {
        return super::install::pick_model(s, i);
    }
    if !row.pickable {
        s.toast = Some(format!("{}: {}.", row.name, row.status.word()));
        return Effect::None;
    }
    if let Some(t) = tool(row.id).filter(|t| !installed(t, snap, &s.tools)) {
        return super::install::offer(s, t);
    }
    if g == Group::Assistant {
        s.tools.assistant = if row.id == CLI_ONLY {
            AssistantChoice::CliOnly
        } else {
            AssistantChoice::Tool(row.id)
        };
        return connect_if_needed(s, &row);
    }
    let Some(form) = s.pipeline.form.as_mut() else {
        s.toast = Some(READ_ONLY.into());
        return Effect::None;
    };
    match g {
        Group::Workspace => {
            s.tools.machine = form.herdr_machine.clone();
            s.tools.edit = Some(Edit::Machine);
            return Effect::None;
        }
        Group::Builder | Group::Reviewer | Group::Decision => {
            let sel = match g {
                Group::Builder => &mut form.builder,
                Group::Reviewer => &mut form.reviewer,
                _ => &mut form.backend,
            };
            if let Some(i) = sel.items.iter().position(|it| it.label == row.id) {
                sel.choose(i);
            }
        }
        Group::Assistant => {}
    }
    super::pipeline_screen::refresh(s);
    connect_if_needed(s, &row)
}

/// A tool that is installed but not ready, and has a way to connect, opens its connect overlay.
fn connect_if_needed(s: &mut State, row: &Row) -> Effect {
    let needs = matches!(
        row.status,
        ToolStatus::NeedsStep | ToolStatus::InstalledLoginUnverified
    );
    if needs && tool(row.id).is_some_and(|t| !t.methods.is_empty() || t.check.is_some()) {
        super::connect::open(s, row.id);
    }
    Effect::None
}

/// `c`: the connect overlay of the focused row, also for a ready tool (to run its live check).
fn open_connect(s: &mut State) -> Effect {
    let Some(snap) = &s.snapshot else {
        return Effect::None;
    };
    let g = s.tools.group;
    let list = rows(g, snap, &s.tools, s.pipeline.form.as_ref());
    let Some(row) = list.get(s.tools.cursor[index(g)]).cloned() else {
        return Effect::None;
    };
    let Some(t) = tool(row.id).filter(|_| row.pickable) else {
        s.toast = Some(format!("{}: {}.", row.name, row.status.word()));
        return Effect::None;
    };
    if !installed(t, snap, &s.tools) {
        let how = if matches!(
            super::install::install_offer(t),
            super::install::Offer::None
        ) {
            "Install it first."
        } else {
            "Press Enter to install it."
        };
        s.toast = Some(format!("{} is not installed. {how}", row.name));
    } else if t.methods.is_empty() && t.check.is_none() {
        s.toast = Some(format!("{} has nothing to connect.", row.name));
    } else {
        super::connect::open(s, t.id);
    }
    Effect::None
}

/// Leave the edit: `keep` commits the machine label (the filter simply stays as typed).
fn end_edit(s: &mut State, keep: bool) {
    let Some(edit) = s.tools.edit.take() else {
        return;
    };
    match (edit, keep) {
        (Edit::Filter, false) => clear_filter(s),
        (Edit::Machine, true) => {
            let label = s.tools.machine.value().trim().to_string();
            if let Some(f) = s.pipeline.form.as_mut() {
                f.herdr_machine = Input::new(&label);
            }
            super::pipeline_screen::refresh(s);
        }
        _ => {}
    }
}

/// Keys while the filter or the machine label is being typed.
pub fn edit_key(s: &mut State, key: KeyEvent) {
    let Some(edit) = s.tools.edit else {
        return;
    };
    let input = match edit {
        Edit::Filter => &mut s.tools.filter,
        Edit::Machine => &mut s.tools.machine,
    };
    match input.handle(key) {
        InputEvent::Commit => end_edit(s, true),
        InputEvent::Cancel => end_edit(s, false),
        InputEvent::Changed if edit == Edit::Filter => s.tools.cursor[index(s.tools.group)] = 0,
        InputEvent::Changed | InputEvent::None => {}
    }
}

pub fn paste(s: &mut State, text: &str) {
    match s.tools.edit {
        Some(Edit::Filter) => s.tools.filter.paste(text),
        Some(Edit::Machine) => s.tools.machine.paste(text),
        None => {}
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::check::view::tests::{agent, real};
    use crate::tui::detect_worker::Msg;
    use igris_detect::Login;
    use ratatui::crossterm::event::{KeyCode, KeyModifiers};

    pub(crate) fn snap(agents: &[(&str, bool, Login)]) -> Snapshot {
        let mut s = real();
        s.agents = agents
            .iter()
            .map(|(k, f, l)| agent(k, *f, *l, None))
            .collect();
        s
    }

    pub(crate) fn both() -> Snapshot {
        snap(&[
            ("claude", true, Login::LoggedIn),
            ("codex", true, Login::LoggedIn),
        ])
    }

    /// The app after the first scan, on the Tools screen, from a repo with this file text (or none).
    pub(crate) fn app(snap: Snapshot, toml: Option<&str>) -> State {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let repo = std::env::temp_dir().join(format!("igris-tools-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&repo);
        std::fs::create_dir_all(&repo).unwrap();
        if let Some(t) = toml {
            std::fs::write(repo.join("igris.toml"), t).unwrap();
        }
        let mut s = State::new((100, 30));
        s.repo = repo.clone();
        s.apply(Msg::Detected(Box::new(snap)));
        s.screen = Screen::Tools;
        let _ = std::fs::remove_dir_all(&repo);
        s
    }

    fn list(s: &State, g: Group) -> Vec<Row> {
        rows(
            g,
            s.snapshot.as_ref().unwrap(),
            &s.tools,
            s.pipeline.form.as_ref(),
        )
    }

    fn ids(v: &[Row]) -> Vec<&'static str> {
        v.iter().map(|r| r.id).collect()
    }

    fn go(s: &mut State, g: Group, id: &str) {
        s.tools.group = g;
        let at = list(s, g).iter().position(|r| r.id == id).expect(id);
        s.tools.cursor[index(g)] = at;
    }

    fn key(s: &mut State, code: KeyCode) {
        edit_key(s, KeyEvent::new(code, KeyModifiers::NONE));
    }

    fn type_text(s: &mut State, text: &str) {
        for c in text.chars() {
            key(s, KeyCode::Char(c));
        }
    }

    #[test]
    fn groups_run_in_order_workspace_builder_reviewer_decision_assistant() {
        assert_eq!(
            GROUPS,
            [
                Group::Workspace,
                Group::Builder,
                Group::Reviewer,
                Group::Decision,
                Group::Assistant
            ]
        );
        let mut s = app(both(), None);
        let mut seen = vec![s.tools.group];
        for _ in 0..5 {
            update(&mut s, Action::NextField);
            seen.push(s.tools.group);
        }
        assert_eq!(seen[..5], GROUPS, "Tab walks the groups in order");
        assert_eq!(seen[5], Group::Workspace, "and wraps");
        update(&mut s, Action::PrevField);
        assert_eq!(s.tools.group, Group::Assistant);
    }

    #[test]
    fn detected_rows_come_first_then_dimmed_ones() {
        let s = app(
            snap(&[
                ("pi", true, Login::Unknown),
                ("claude", true, Login::LoggedIn),
                ("codex", true, Login::LoggedIn),
            ]),
            None,
        );
        let v = list(&s, Group::Builder);
        assert_eq!(ids(&v)[..3], ["claude", "codex", "pi"]);
        assert!(v[..3].iter().all(|r| r.status != ToolStatus::NotInstalled));
        assert!(
            v[3..].iter().all(|r| r.status == ToolStatus::NotInstalled),
            "every other agent is dimmed under the detected ones"
        );
        assert_eq!(v.len(), 24, "all 24 agent kinds are listed");
    }

    #[test]
    fn claude_is_the_builder_default_and_a_different_agent_the_reviewer_default() {
        let s = app(both(), None);
        let (b, r) = (list(&s, Group::Builder), list(&s, Group::Reviewer));
        assert_eq!((b[0].id, b[0].default), ("claude", true));
        assert_eq!((r[0].id, r[0].default), ("codex", true));
        assert_eq!(b.iter().filter(|x| x.default).count(), 1);

        let only = app(snap(&[("claude", true, Login::LoggedIn)]), None);
        let r = list(&only, Group::Reviewer);
        assert_eq!((r[0].id, r[0].default), ("claude", true));
        let form = only.pipeline.form.as_ref().unwrap();
        assert!(
            same_agent(form),
            "one agent does both jobs, and the screen says so"
        );
        assert!(!same_agent(
            app(both(), None).pipeline.form.as_ref().unwrap()
        ));
    }

    #[test]
    fn an_unreadable_login_is_installed_login_unverified_never_ready() {
        let s = app(
            snap(&[
                ("gemini", true, Login::Unknown),
                ("claude", true, Login::LoggedIn),
                ("codex", true, Login::LoggedOut),
            ]),
            None,
        );
        let of = |id: &str| {
            list(&s, Group::Builder)
                .into_iter()
                .find(|r| r.id == id)
                .unwrap()
                .status
        };
        assert_eq!(of("gemini"), ToolStatus::InstalledLoginUnverified);
        assert_eq!(of("claude"), ToolStatus::Ready);
        assert_eq!(of("codex"), ToolStatus::NeedsStep);
        assert_eq!(of("kimi"), ToolStatus::NotInstalled);
        assert_eq!(
            ToolStatus::InstalledLoginUnverified.word(),
            "installed · login unverified"
        );
    }

    #[test]
    fn the_status_words_are_exactly_the_specs() {
        let words: Vec<&str> = [
            ToolStatus::Ready,
            ToolStatus::InstalledLoginUnverified,
            ToolStatus::NeedsStep,
            ToolStatus::Checking,
            ToolStatus::Skipped,
            ToolStatus::NotInstalled,
            ToolStatus::NotSupportedYet,
            ToolStatus::NotSelectableYet,
        ]
        .iter()
        .map(|s| s.word())
        .collect();
        assert_eq!(
            words,
            [
                "ready",
                "installed · login unverified",
                "needs a step",
                "checking…",
                "skipped",
                "not installed",
                "not supported yet",
                "not selectable yet"
            ]
        );
    }

    #[test]
    fn a_live_check_decides_the_word_once_there_is_one() {
        let s = app(both(), None);
        let claude = tool("claude").unwrap();
        let snap = s.snapshot.as_ref().unwrap();
        let mut ui = ToolsUi::default();
        for (state, want) in [
            (CheckState::Running, ToolStatus::Checking),
            (CheckState::Passed { ms: 6800 }, ToolStatus::Ready),
            (CheckState::Skipped, ToolStatus::Skipped),
            (
                CheckState::Failed(Failure {
                    reason: "no reply".into(),
                    unchanged: "nothing".into(),
                    fix: None,
                }),
                ToolStatus::NeedsStep,
            ),
        ] {
            ui.checks.insert("claude".into(), state);
            assert_eq!(status_of(claude, snap, &ui), want);
        }
    }

    #[test]
    fn workspace_lists_herdr_and_dims_the_rest_not_supported_yet() {
        let s = app(both(), None);
        let v = list(&s, Group::Workspace);
        assert_eq!((v[0].id, v[0].status), ("herdr", ToolStatus::Ready));
        assert!(v[0].default && v[0].chosen && v[0].pickable);
        let orca = v.iter().find(|r| r.id == "orca").unwrap();
        assert_eq!(orca.status, ToolStatus::NotSupportedYet);
        assert!(!orca.pickable);
    }

    #[test]
    fn workspace_records_this_computer_or_a_saved_machine_label() {
        let mut s = app(both(), None);
        go(&mut s, Group::Workspace, "herdr");
        assert_eq!(update(&mut s, Action::Activate), Effect::None);
        assert!(editing(&s), "Enter on Herdr asks where it runs");
        type_text(&mut s, "devbox");
        key(&mut s, KeyCode::Enter);
        assert!(!editing(&s));
        assert_eq!(
            s.pipeline.form.as_ref().unwrap().herdr_machine.value(),
            "devbox"
        );
        assert!(s.dirty, "the change waits for Review");

        update(&mut s, Action::Activate);
        edit_key(
            &mut s,
            KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL),
        );
        key(&mut s, KeyCode::Enter);
        assert_eq!(
            s.pipeline.form.as_ref().unwrap().herdr_machine.value(),
            "",
            "empty means this computer"
        );
        update(&mut s, Action::Activate);
        type_text(&mut s, "nope");
        key(&mut s, KeyCode::Esc);
        assert_eq!(s.pipeline.form.as_ref().unwrap().herdr_machine.value(), "");
    }

    #[test]
    fn decision_shows_rules_ready_and_jev_laya_not_selectable_yet() {
        let s = app(both(), None);
        let v = list(&s, Group::Decision);
        assert_eq!(ids(&v), ["rules", "jev", "laya"]);
        assert_eq!(
            (v[0].status, v[0].default, v[0].chosen),
            (ToolStatus::Ready, true, true)
        );
        assert_eq!(v[1].status, ToolStatus::NotSelectableYet);
        assert_eq!(v[2].status, ToolStatus::NotSelectableYet);
        assert!(!v[1].pickable && !v[2].pickable);
    }

    #[test]
    fn typing_filters_the_focused_list_and_escape_clears_it() {
        let mut s = app(both(), None);
        s.tools.group = Group::Builder;
        update(&mut s, Action::Filter);
        assert!(editing(&s));
        type_text(&mut s, "cla");
        assert_eq!(ids(&list(&s, Group::Builder)), ["claude"]);
        assert_eq!(
            list(&s, Group::Reviewer).len(),
            24,
            "the other list is not filtered"
        );
        key(&mut s, KeyCode::Enter);
        assert!(!editing(&s) && ids(&list(&s, Group::Builder)) == ["claude"]);
        update(&mut s, Action::Cancel);
        assert_eq!(list(&s, Group::Builder).len(), 24);
    }

    #[test]
    fn no_group_has_a_skip_row_and_the_assistant_must_be_answered() {
        let s = app(both(), None);
        for g in GROUPS {
            assert!(
                list(&s, g).iter().all(|r| !r.name.starts_with("Skip")),
                "{g:?}"
            );
        }
        let cli = list(&s, Group::Assistant)
            .into_iter()
            .find(|r| r.id == CLI_ONLY)
            .unwrap();
        assert!(cli.pickable && cli.status == ToolStatus::Ready);
    }

    #[test]
    fn the_assistant_must_be_answered_and_cli_only_counts() {
        let mut s = app(both(), None);
        let (ui, form, snap) = (
            &s.tools,
            s.pipeline.form.as_ref().unwrap(),
            s.snapshot.as_ref().unwrap(),
        );
        assert!(!tools_done(ui, form, snap), "the assistant is unanswered");
        go(&mut s, Group::Assistant, CLI_ONLY);
        update(&mut s, Action::Activate);
        assert_eq!(s.tools.assistant, AssistantChoice::CliOnly);
        let (ui, form, snap) = (
            &s.tools,
            s.pipeline.form.as_ref().unwrap(),
            s.snapshot.as_ref().unwrap(),
        );
        assert!(tools_done(ui, form, snap));
        let mut none = snap.clone();
        none.herdr.found = false;
        assert!(!tools_done(ui, form, &none), "Herdr must be installed");
        let mut gone = snap.clone();
        gone.agents.clear();
        assert!(
            !tools_done(ui, form, &gone),
            "the chosen agents must be installed"
        );
    }

    #[test]
    fn picking_a_builder_writes_the_pipeline_form() {
        let mut s = app(both(), None);
        assert_eq!(
            s.pipeline
                .form
                .as_ref()
                .unwrap()
                .builder
                .current()
                .unwrap()
                .label,
            "claude"
        );
        go(&mut s, Group::Builder, "codex");
        update(&mut s, Action::Activate);
        let f = s.pipeline.form.as_ref().unwrap();
        assert_eq!(f.builder.current().unwrap().label, "codex");
        assert!(s.dirty);
        assert!(
            list(&s, Group::Builder)
                .iter()
                .find(|r| r.id == "codex")
                .unwrap()
                .chosen
        );
        go(&mut s, Group::Reviewer, "claude");
        update(&mut s, Action::Activate);
        assert_eq!(
            s.pipeline
                .form
                .as_ref()
                .unwrap()
                .reviewer
                .current()
                .unwrap()
                .label,
            "claude"
        );
    }

    #[test]
    fn a_row_that_is_not_installed_or_not_supported_is_refused_with_words() {
        let mut s = app(both(), None);
        go(&mut s, Group::Builder, "kimi");
        update(&mut s, Action::Activate);
        assert!(
            s.toast.as_deref().unwrap().contains("not installed"),
            "{:?}",
            s.toast
        );
        assert_eq!(
            s.pipeline
                .form
                .as_ref()
                .unwrap()
                .builder
                .current()
                .unwrap()
                .label,
            "claude"
        );
        go(&mut s, Group::Decision, "jev");
        update(&mut s, Action::Activate);
        assert!(s.toast.as_deref().unwrap().contains("not selectable yet"));
    }

    #[test]
    fn a_broken_igris_toml_is_never_overwritten_by_tools() {
        let mut s = app(both(), Some("this is = not [valid"));
        assert!(s.pipeline.broken.is_some() && s.pipeline.form.is_none());
        s.tools.group = Group::Builder;
        assert_eq!(update(&mut s, Action::Activate), Effect::None);
        assert!(
            s.pipeline.form.is_none(),
            "no form is invented over the broken file"
        );
        assert!(!s.dirty);
        assert!(
            s.toast.as_deref().unwrap().contains("igris.toml"),
            "{:?}",
            s.toast
        );
    }

    #[test]
    fn every_row_can_be_reached_by_key_and_by_click() {
        let mut s = app(both(), None);
        s.tools.group = Group::Builder;
        for _ in 0..40 {
            update(&mut s, Action::Down);
        }
        assert_eq!(
            s.tools.cursor[index(Group::Builder)],
            23,
            "stops on the last row"
        );
        update(&mut s, Action::Home);
        assert_eq!(s.tools.cursor[index(Group::Builder)], 0);
        // A click on a row of another group focuses that group and picks the row.
        update(&mut s, Action::Select(index(Group::Reviewer) * 1000 + 1));
        assert_eq!(s.tools.group, Group::Reviewer);
        assert_eq!(s.tools.cursor[index(Group::Reviewer)], 1);
        update(
            &mut s,
            Action::Select(index(Group::Decision) * 1000 + HEADER),
        );
        assert_eq!(
            s.tools.group,
            Group::Decision,
            "a click on a name only focuses the group"
        );
    }

    #[test]
    fn extra_ids_are_the_selectable_tools_the_scan_has_no_field_for() {
        assert_eq!(extra_ids(), ["openclaw"]);
        let hit = |n: &str| n == "openclaw";
        assert_eq!(found_on_path(&["openclaw", "orca"], hit), ["openclaw"]);
        assert_eq!(found_on_path(&["openclaw"], |_| false), Vec::<&str>::new());
    }

    #[test]
    fn a_found_assistant_is_installed_and_an_unfound_one_is_not() {
        let mut s = app(both(), None);
        let of = |s: &State| {
            list(s, Group::Assistant)
                .into_iter()
                .find(|r| r.id == "openclaw")
                .unwrap()
                .status
        };
        assert_eq!(of(&s), ToolStatus::NotInstalled);
        s.apply(Msg::Found(vec!["openclaw"]));
        assert_eq!(of(&s), ToolStatus::InstalledLoginUnverified);
    }
}
