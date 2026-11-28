//! The chain's state and rules, with no drawing and no I/O: what is asked, what the defaults are,
//! what a re-scan does to the answers.
use crate::catalog::{Tool, tool};
use crate::tui::connect::{Act, methods_for};
use crate::tui::install::{Offer, install_offer, quote_cmd};
use igris_detect::{Agent, Login, Snapshot};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Step {
    /// Herdr is missing: nothing to ask until it is installed.
    #[default]
    Workspace,
    Builder,
    Reviewer,
    /// How to get the `subject` agent installed (or why IGRIS cannot do it).
    Install,
    /// Make the `subject` agent ready: wait for its install, log in, check it live.
    Connect,
    /// Slice 2 ends here.
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    Ready,
    /// Usable, but not proven (the login could not be read).
    Warn,
    /// The user must do something first (log in).
    Step,
    /// Listed, but cannot be chosen yet (not installed).
    Dim,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Choice {
    pub id: String,
    pub label: String,
    pub note: String,
    pub mark: Mark,
    pub warn: Option<String>,
    /// The words of the call to action; `None` means `Use <label>`.
    pub cta: Option<String>,
}

/// What a choice asks the app to run. The model never runs anything itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Want {
    /// The tool's verified install command, as a background job.
    Install(&'static str),
    /// Put the tool's install command on the clipboard (it needs administrator rights).
    Copy(&'static str),
    /// The tool's live check.
    Check(&'static str),
    /// The tool's own login command, on the user's terminal.
    Attach(&'static [&'static str]),
}

/// What the install and check engines are doing for one agent (built by `State`).
#[derive(Debug, Clone, PartialEq)]
pub enum Work {
    Installing { secs: u64 },
    InstallFailed { reason: String, fix: Option<String> },
    InstallDone,
    Checking { secs: u64 },
    Passed { ms: u64 },
    CheckFailed { reason: String, fix: Option<String> },
}

pub type WorkMap = HashMap<String, Work>;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ChainUi {
    pub step: Step,
    pub builder: Option<String>,
    pub reviewer: Option<String>,
    pub cursor: usize,
    /// The agent the `Install` or `Connect` question is about.
    pub subject: Option<String>,
    /// A line shown in the note box (for example where to put an API key).
    pub hint: Option<String>,
    /// What the engines are doing; `State` sets it before every key, scan result and frame.
    pub work: WorkMap,
}

impl ChainUi {
    pub fn new() -> Self {
        Self::default()
    }
    /// After every scan: skip Workspace once Herdr is found, drop answers whose agent is gone (and
    /// every later one) unless its install is running, route to what still needs attention, and
    /// put the cursor on the default.
    pub fn settle(&mut self, snap: &Snapshot) {
        if !snap.herdr.found {
            let work = std::mem::take(&mut self.work);
            *self = Self::default();
            self.work = work;
            return;
        }
        if self.step == Step::Workspace {
            self.step = Step::Builder;
        }
        let here = |k: &str| {
            snap.agents.iter().any(|a| a.kind == k && a.found)
                || matches!(
                    self.work.get(k),
                    Some(Work::Installing { .. } | Work::InstallDone | Work::InstallFailed { .. })
                )
        };
        if self.builder.as_deref().is_some_and(|b| !here(b)) {
            self.builder = None;
            self.reviewer = None;
            self.step = Step::Builder;
            self.subject = None;
        } else if self.reviewer.as_deref().is_some_and(|r| !here(r)) {
            self.reviewer = None;
            self.step = Step::Reviewer;
            self.subject = None;
        }
        if self.step == Step::Install
            && self
                .subject
                .as_deref()
                .is_some_and(|k| snap.agents.iter().any(|a| a.kind == k && a.found))
        {
            // It was installed by hand and the scan found it: back to the list.
            self.subject = None;
            self.step = if self.builder.is_none() {
                Step::Builder
            } else {
                Step::Reviewer
            };
        }
        self.route(snap);
        self.reset_cursor(snap);
    }

    /// When both questions are answered, the first agent that still needs something becomes the
    /// `Connect` question; with none, the chain is at its end.
    fn route(&mut self, snap: &Snapshot) {
        if self.builder.is_none() || self.reviewer.is_none() {
            return;
        }
        let ids = [self.builder.clone(), self.reviewer.clone()];
        match ids
            .into_iter()
            .flatten()
            .find(|k| self.needs_attention(snap, k))
        {
            Some(k) => {
                self.step = Step::Connect;
                self.subject = Some(k);
            }
            None => {
                self.step = Step::End;
                self.subject = None;
            }
        }
    }

    /// An agent needs attention while it installs or failed to, or when it is installed but not
    /// proven and the catalog has a way to prove it (a login method or a live check).
    fn needs_attention(&self, snap: &Snapshot, kind: &str) -> bool {
        match self.work.get(kind) {
            Some(Work::Passed { .. }) => false,
            Some(_) => true,
            None => snap
                .agents
                .iter()
                .find(|a| a.kind == kind)
                .is_some_and(|a| {
                    !is_ready(a)
                        && tool(kind).is_some_and(|t| !a.found || !methods_for(t, snap).is_empty())
                }),
        }
    }

    /// A re-scan: like `settle`, but the cursor stays on the agent the user moved to when the
    /// question is the same and that agent is still installed.
    pub fn resettle(&mut self, snap: &Snapshot, prev: &Snapshot) {
        let step = self.step;
        let under = self.choices(prev).get(self.cursor).map(|c| c.id.clone());
        self.settle(snap);
        if self.step != step {
            return;
        }
        let at = under.and_then(|id| {
            self.choices(snap)
                .iter()
                .position(|c| c.id == id && c.mark != Mark::Dim)
        });
        if let Some(at) = at {
            self.cursor = at;
        }
    }

    /// On the default; with none (nothing is verified) on the first installed agent, and for the
    /// reviewer on one that is not the builder. Never a silent choice: this only places the cursor.
    fn reset_cursor(&mut self, snap: &Snapshot) {
        if !matches!(self.step, Step::Builder | Step::Reviewer) {
            self.cursor = 0;
            return;
        }
        let v = ordered(snap);
        let builder = self.builder.as_deref();
        let other = (self.step == Step::Reviewer)
            .then(|| {
                v.iter()
                    .position(|a| a.found && Some(a.kind.as_str()) != builder)
            })
            .flatten();
        let last = v.len().saturating_sub(1);
        self.cursor = self.default_index(snap).or(other).unwrap_or(0).min(last);
    }

    /// The options of the active question: installed agents first, then the missing ones (dim).
    pub fn choices(&self, snap: &Snapshot) -> Vec<Choice> {
        match self.step {
            Step::Builder | Step::Reviewer => self.agent_choices(snap),
            Step::Workspace if !snap.herdr.found => match self.work.get("herdr") {
                Some(Work::Installing { .. } | Work::InstallDone) => vec![],
                w => tool("herdr").map_or_else(Vec::new, |t| {
                    let mut v = install_choices(t, matches!(w, Some(Work::InstallFailed { .. })));
                    v.retain(|c| c.id != "another");
                    v
                }),
            },
            Step::Install => match self.subject.as_deref().and_then(tool) {
                Some(t) => install_choices(t, false),
                None => vec![],
            },
            Step::Connect => self.connect_choices(snap),
            Step::Workspace | Step::End => vec![],
        }
    }

    fn agent_choices(&self, snap: &Snapshot) -> Vec<Choice> {
        ordered(snap)
            .into_iter()
            .map(|a| {
                let (note, mark, mut warn) = status(a);
                let same = self.step == Step::Reviewer && self.builder.as_deref() == Some(&a.kind);
                if same && warn.is_none() {
                    warn = Some("same agent, new session".to_string());
                }
                Choice {
                    id: a.kind.clone(),
                    label: name(&a.kind),
                    note: note.to_string(),
                    mark,
                    warn,
                    cta: (!a.found).then(|| {
                        let n = name(&a.kind);
                        match tool(&a.kind).map(install_offer) {
                            Some(Offer::Run(_) | Offer::Copy(_)) => format!("Install {n}"),
                            _ => format!("How to install {n}"),
                        }
                    }),
                }
            })
            .collect()
    }

    /// What can be done for the `Connect` subject: wait, retry a failed install, or choose another.
    fn connect_choices(&self, snap: &Snapshot) -> Vec<Choice> {
        let Some(k) = self.subject.as_deref() else {
            return vec![];
        };
        let Some(t) = tool(k) else {
            return vec![];
        };
        match self.work.get(k) {
            Some(Work::Installing { .. } | Work::InstallDone | Work::Checking { .. }) => vec![],
            Some(Work::InstallFailed { .. }) => install_choices(t, true),
            w => {
                let mut v = Vec::new();
                if matches!(w, Some(Work::CheckFailed { .. })) {
                    v.push(pick(
                        "recheck",
                        "Check again".to_string(),
                        "runs the live check once more".to_string(),
                        Some("Check again".to_string()),
                    ));
                }
                for (i, r) in methods_for(t, snap).iter().enumerate() {
                    let cta = match r.act {
                        // "Check again" is already the first row after a failed check.
                        Act::Check if matches!(w, Some(Work::CheckFailed { .. })) => continue,
                        Act::Check => format!("Check {}", t.name),
                        Act::Attach(_) => r.label.to_string(),
                        Act::Env(_) => "Show how".to_string(),
                        Act::Store(_) => continue, // masked key entry comes with paste handling
                    };
                    v.push(pick(
                        &format!("method:{i}"),
                        r.label.to_string(),
                        r.hint.to_string(),
                        Some(cta),
                    ));
                }
                v.push(another());
                v
            }
        }
    }

    /// Builder: Claude Code when it is ready, else the first ready agent. Reviewer: a ready agent
    /// that is not the builder, else the builder. Never an agent that is logged out or unverified.
    pub fn default_index(&self, snap: &Snapshot) -> Option<usize> {
        let v = ordered(snap);
        match self.step {
            Step::Builder => v
                .iter()
                .position(|a| is_ready(a) && a.kind == "claude")
                .or_else(|| v.iter().position(|a| is_ready(a))),
            Step::Reviewer => {
                let b = self.builder.as_deref();
                v.iter()
                    .position(|a| is_ready(a) && Some(a.kind.as_str()) != b)
                    .or_else(|| {
                        v.iter()
                            .position(|a| is_ready(a) && Some(a.kind.as_str()) == b)
                    })
            }
            _ => None,
        }
    }

    /// Answer the active question with the highlighted option. Returns what the app should run.
    pub fn enter(&mut self, snap: &Snapshot) -> Option<Want> {
        match self.step {
            Step::Builder | Step::Reviewer => {
                let v = ordered(snap);
                let a = *v.get(self.cursor)?;
                if a.found || self.installing(&a.kind) {
                    self.answer(&a.kind, snap);
                } else {
                    self.step = Step::Install;
                    self.subject = Some(a.kind.clone());
                    self.cursor = 0;
                }
                None
            }
            Step::Workspace | Step::Install | Step::Connect => {
                let c = self.choices(snap).get(self.cursor)?.clone();
                let id = self.subject.as_deref().or(Some("herdr")).and_then(tool)?.id;
                match c.id.as_str() {
                    "install" => {
                        if self.step == Step::Install {
                            self.answer(id, snap);
                        }
                        Some(Want::Install(id))
                    }
                    "retry" => Some(Want::Install(id)),
                    "copy" => Some(Want::Copy(id)),
                    "recheck" => Some(Want::Check(id)),
                    m if m.starts_with("method:") => {
                        let i: usize = m["method:".len()..].parse().ok()?;
                        match methods_for(tool(id)?, snap).get(i)?.act {
                            Act::Check => Some(Want::Check(id)),
                            Act::Attach(argv) => Some(Want::Attach(argv)),
                            Act::Env(name) => {
                                self.hint = Some(env_hint(tool(id)?, name));
                                None
                            }
                            Act::Store(_) => None,
                        }
                    }
                    "another" => {
                        self.back(snap);
                        None
                    }
                    _ => None,
                }
            }
            Step::End => None,
        }
    }

    fn installing(&self, kind: &str) -> bool {
        matches!(
            self.work.get(kind),
            Some(Work::Installing { .. } | Work::InstallDone | Work::InstallFailed { .. })
        )
    }

    /// Record `kind` as the answer of the active question and go to the next one.
    fn answer(&mut self, kind: &str, snap: &Snapshot) {
        if self.builder.is_none() {
            self.builder = Some(kind.to_string());
            self.step = Step::Reviewer;
        } else {
            self.reviewer = Some(kind.to_string());
            self.step = Step::End;
        }
        self.subject = None;
        self.route(snap);
        self.reset_cursor(snap);
    }

    /// One step back; the answer of the step landed on, and every later one, is cleared. From an
    /// install or connect question it goes back to choose another agent.
    pub fn back(&mut self, snap: &Snapshot) {
        match self.step {
            Step::Reviewer => {
                self.builder = None;
                self.reviewer = None;
                self.step = Step::Builder;
            }
            Step::End => {
                self.reviewer = None;
                self.step = Step::Reviewer;
            }
            Step::Install => {
                self.step = if self.builder.is_none() {
                    Step::Builder
                } else {
                    Step::Reviewer
                };
                self.subject = None;
            }
            Step::Connect => {
                let subject = self.subject.take();
                if subject.is_some() && subject == self.builder {
                    self.builder = None;
                    self.reviewer = None;
                    self.step = Step::Builder;
                } else {
                    self.reviewer = None;
                    self.step = Step::Reviewer;
                }
            }
            Step::Workspace | Step::Builder => {}
        }
        self.reset_cursor(snap);
    }

    /// The answered questions as (label, answer, status), oldest first.
    pub fn history(&self, snap: &Snapshot) -> Vec<(String, String, String)> {
        let mut h = Vec::new();
        if snap.herdr.found {
            let v = snap.herdr.version.as_deref().unwrap_or("");
            h.push((
                "Workspace".to_string(),
                format!("Herdr {v}").trim_end().to_string(),
                "ready".to_string(),
            ));
        }
        let state_of = |kind: &str| match self.work.get(kind) {
            Some(Work::Installing { secs }) => format!("installing {}", clock(*secs)),
            Some(Work::InstallDone) => "installed, scanning".to_string(),
            Some(Work::InstallFailed { .. }) => "install failed".to_string(),
            Some(Work::Checking { secs }) => format!("checking {}", clock(*secs)),
            Some(Work::Passed { ms }) => format!("replied in {:.1} s", *ms as f64 / 1000.0),
            Some(Work::CheckFailed { .. }) => "check failed".to_string(),
            None => snap
                .agents
                .iter()
                .find(|a| a.kind == kind)
                .map_or("not installed", |a| status(a).0)
                .to_string(),
        };
        if let Some(b) = &self.builder {
            h.push(("Builder".to_string(), name(b), state_of(b)));
        }
        if let Some(r) = &self.reviewer {
            let s = if self.builder.as_ref() == Some(r) {
                "same agent, new session".to_string()
            } else {
                state_of(r)
            };
            h.push(("Reviewer".to_string(), name(r), s));
        }
        h
    }

    /// A dim line for a question with nothing to pick because the app is working: an install or a
    /// live check that is running.
    pub fn wait_line(&self, _snap: &Snapshot) -> Option<String> {
        let k = self.subject.as_deref().or(match self.step {
            Step::Workspace => Some("herdr"),
            _ => None,
        })?;
        let n = name(k);
        match self.work.get(k)? {
            Work::Installing { secs } => Some(format!(
                "Installing {n}: {}. This moves on by itself when the scan finds it.",
                clock(*secs)
            )),
            Work::Checking { secs } => Some(format!(
                "Checking {n}: {}. A live call to the agent can take a minute.",
                clock(*secs)
            )),
            _ => None,
        }
    }

    /// The three lines of a failed check or install: what happened, what stayed unchanged, the fix.
    fn failure(&self, k: &str) -> Option<String> {
        let (reason, unchanged, fix) = match self.work.get(k)? {
            Work::CheckFailed { reason, fix } => (reason, "no setting was changed.", fix),
            Work::InstallFailed { reason, fix } => (
                reason,
                "IGRIS changed no setting; the installer may have left files.",
                fix,
            ),
            _ => return None,
        };
        let mut text = format!("{}: {}\nUnchanged: {unchanged}", name(k), sentence(reason));
        if let Some(f) = fix {
            text.push_str(&format!("\nFix: {f}"));
        }
        Some(text)
    }

    /// What stops the chain on this step, in words, with the tool's documented install command.
    pub fn blocker(&self, snap: &Snapshot) -> Option<String> {
        match self.step {
            // A command that can run is on the option row, so no box repeats it; a failed run is
            // the exception, because the row cannot say why it failed.
            Step::Workspace
                if tool("herdr").is_some_and(|t| matches!(install_offer(t), Offer::Run(_))) =>
            {
                self.failure("herdr")
            }
            Step::Workspace => Some(format!(
                "Herdr is not installed. Install it, then press r to re-scan. {}",
                install_line("herdr")
            )),
            Step::Connect => {
                let failure = self.failure(self.subject.as_deref()?);
                match (failure, &self.hint) {
                    (Some(f), Some(h)) => Some(format!("{f}\n{h}")),
                    (Some(f), None) => Some(f),
                    (None, h) => h.clone(),
                }
            }
            Step::Install => {
                let t = self.subject.as_deref().and_then(tool)?;
                match install_offer(t) {
                    Offer::None => Some(format!(
                        "{} has no install command IGRIS can verify. {} Install it yourself, then press r to re-scan.",
                        t.name,
                        if t.docs.is_empty() {
                            String::new()
                        } else {
                            format!("See {}.", t.docs)
                        }
                    )),
                    Offer::Copy(cmd) => Some(format!(
                        "{} needs administrator rights: run this yourself, then press r to re-scan. {cmd}",
                        t.name
                    )),
                    Offer::Run(_) => None,
                }
            }
            Step::Builder | Step::Reviewer if !snap.agents.iter().any(|a| a.found) => {
                Some(format!(
                    "No coding agent is installed. Install one, then press r to re-scan. {}",
                    install_line("claude")
                ))
            }
            _ => None,
        }
    }
}

fn pick(id: &str, label: String, note: String, cta: Option<String>) -> Choice {
    Choice {
        id: id.to_string(),
        label,
        note,
        mark: Mark::Warn,
        warn: None,
        cta,
    }
}

fn another() -> Choice {
    pick(
        "another",
        "Choose another agent".to_string(),
        String::new(),
        Some("Choose another agent".to_string()),
    )
}

/// What can be done about installing `t`: run its verified command in the background, copy it when
/// it needs administrator rights, or neither when IGRIS has no verified command (then only
/// "Choose another agent"). After a failure the run is offered again as "Try again".
pub fn install_choices(t: &Tool, failed: bool) -> Vec<Choice> {
    let mut v = Vec::new();
    match install_offer(t) {
        Offer::Run(argv) if failed => v.push(pick(
            "retry",
            "Try again".to_string(),
            quote_cmd(argv),
            Some("Try again".to_string()),
        )),
        Offer::Run(argv) => v.push(pick(
            "install",
            format!("Install {} in the background", t.name),
            quote_cmd(argv),
            Some(format!("Install {}", t.name)),
        )),
        Offer::Copy(cmd) => v.push(pick(
            "copy",
            "Copy the command".to_string(),
            format!("needs administrator rights: {cmd}"),
            Some("Copy the command".to_string()),
        )),
        Offer::None => {}
    }
    v.push(another());
    v
}

/// Where an API key goes: the agent reads it from the environment IGRIS starts in, so the user
/// sets it, opens a new terminal, starts igris there and re-scans.
fn env_hint(t: &Tool, name: &str) -> String {
    let line = if cfg!(windows) {
        format!("setx {name} \"<your key>\"")
    } else {
        format!("export {name}=<your key>")
    };
    format!(
        "{} reads its key from {name}. Add `{line}` to your shell profile, open a new terminal, start igris there and press r to re-scan.",
        t.name
    )
}

/// `reason` as a sentence: one trailing full stop.
fn sentence(reason: &str) -> String {
    format!("{}.", reason.trim_end_matches('.'))
}

fn clock(secs: u64) -> String {
    format!("{:02}:{:02}", secs / 60, secs % 60)
}

pub fn name(kind: &str) -> String {
    tool(kind).map_or_else(|| kind.to_string(), |t| t.name.to_string())
}

/// The documented install command (shown, never run here), or the docs page when there is none.
fn install_line(id: &str) -> String {
    let Some(t) = tool(id) else {
        return String::new();
    };
    match t.install {
        Some(argv) if t.install_needs_admin => {
            format!(
                "Needs administrator rights, run it yourself: {}",
                quote_cmd(argv)
            )
        }
        Some(argv) => format!("Install command: {}", quote_cmd(argv)),
        None => format!("See {}", t.docs),
    }
}

/// How many agents that are not installed are listed: the recommended ones (the rest come with
/// the install step, as "More...").
const MISSING_LISTED: usize = 3;

/// Installed agents first, then up to three missing ones; the recommended kinds lead each group
/// (Claude Code, Codex, Gemini), the others keep the scan's order.
fn ordered(snap: &Snapshot) -> Vec<&Agent> {
    let rank = |a: &Agent| match a.kind.as_str() {
        "claude" => 0,
        "codex" => 1,
        "gemini" => 2,
        _ => 3,
    };
    let mut v: Vec<&Agent> = snap.agents.iter().collect();
    v.sort_by_key(|a| (!a.found, rank(a)));
    let installed = v.iter().filter(|a| a.found).count();
    v.truncate(installed + MISSING_LISTED);
    v
}

fn is_ready(a: &Agent) -> bool {
    a.found && a.login == Login::LoggedIn
}

/// Status words (setup flow v2), the row's mark and the warning a chosen row carries.
fn status(a: &Agent) -> (&'static str, Mark, Option<String>) {
    if !a.found {
        return ("not installed", Mark::Dim, None);
    }
    match a.login {
        Login::LoggedIn => ("ready", Mark::Ready, None),
        Login::Unknown => ("installed · login unverified", Mark::Warn, None),
        Login::LoggedOut => (
            "needs a step: log in",
            Mark::Step,
            Some(
                "log in before the first task: the check in Verify fails until you do".to_string(),
            ),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::tools_screen::tests::snap;
    use igris_detect::Login::{LoggedIn, LoggedOut, Unknown};

    fn ready() -> Snapshot {
        snap(&[("claude", true, LoggedIn), ("codex", true, LoggedIn)])
    }

    fn started(s: &Snapshot) -> ChainUi {
        let mut u = ChainUi::new();
        u.settle(s);
        u
    }

    #[test]
    fn b1_two_ready_agents_finish_on_two_enters() {
        let s = ready();
        let mut u = started(&s);
        assert_eq!(u.step, Step::Builder);
        let d = u.default_index(&s).unwrap();
        assert_eq!(u.choices(&s)[d].id, "claude");
        assert_eq!(u.cursor, d);
        u.enter(&s);
        assert_eq!(
            (u.step, u.builder.as_deref()),
            (Step::Reviewer, Some("claude"))
        );
        let d = u.default_index(&s).unwrap();
        assert_eq!(u.choices(&s)[d].id, "codex");
        assert_eq!(u.cursor, d);
        u.enter(&s);
        assert_eq!((u.step, u.reviewer.as_deref()), (Step::End, Some("codex")));
        assert_eq!(u.history(&s).len(), 3);
    }

    #[test]
    fn b2_one_agent_is_both_builder_and_reviewer_with_a_neutral_note() {
        let s = snap(&[("claude", true, LoggedIn)]);
        let mut u = started(&s);
        u.enter(&s);
        assert_eq!(u.step, Step::Reviewer);
        let c = u.choices(&s);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].warn.as_deref(), Some("same agent, new session"));
        assert_ne!(c[0].mark, Mark::Warn);
        u.enter(&s);
        assert_eq!(u.step, Step::End);
    }

    #[test]
    fn w4_none_installed_has_no_default_and_a_blocker() {
        let s = snap(&[("claude", false, LoggedOut), ("codex", false, LoggedOut)]);
        let mut u = started(&s);
        assert_eq!(u.step, Step::Builder);
        assert!(u.choices(&s).iter().all(|c| c.mark == Mark::Dim));
        assert_eq!(u.default_index(&s), None);
        let b = u.blocker(&s).unwrap();
        assert!(b.contains("No coding agent is installed"), "{b}");
        let t = tool("claude").unwrap();
        let want = t.install.map_or(t.docs.to_string(), quote_cmd);
        assert!(b.contains(&want), "{b}");
        u.enter(&s);
        assert_eq!(u.builder, None, "a missing agent is never chosen as it is");
        assert_eq!(
            u.step,
            Step::Install,
            "it opens its install question instead"
        );
    }

    #[test]
    fn w3_logged_out_agent_is_never_the_default() {
        let s = snap(&[("claude", true, LoggedOut), ("codex", true, LoggedIn)]);
        let mut u = started(&s);
        let c = u.choices(&s);
        assert_eq!(c[u.default_index(&s).unwrap()].id, "codex");
        let at = c.iter().position(|c| c.id == "claude").unwrap();
        assert_eq!(c[at].note, "needs a step: log in");
        assert!(c[at].warn.is_some());
        assert_eq!(c[at].mark, Mark::Step);
        u.cursor = at;
        u.enter(&s);
        assert_eq!(u.builder.as_deref(), Some("claude"));
    }

    #[test]
    fn a_missing_herdr_keeps_the_chain_on_workspace() {
        let mut s = ready();
        s.herdr.found = false;
        s.herdr.version = None;
        let mut u = ChainUi::new();
        u.settle(&s);
        assert_eq!(u.step, Step::Workspace);
        assert!(
            u.choices(&s)[0].label.contains("Herdr"),
            "the question offers Herdr's install"
        );
        s.herdr.found = true;
        s.herdr.version = Some("0.9.1".into());
        u.settle(&s);
        assert_eq!(u.step, Step::Builder);
    }

    #[test]
    fn a_rescan_that_removes_the_answered_agent_clears_it_and_the_later_answers() {
        let s = ready();
        let mut u = started(&s);
        u.enter(&s);
        let s2 = snap(&[("claude", false, LoggedOut), ("codex", true, LoggedIn)]);
        u.settle(&s2);
        assert_eq!((u.builder.clone(), u.reviewer.clone()), (None, None));
        assert_eq!(u.step, Step::Builder);
        assert!(u.cursor < u.choices(&s2).len());
    }

    fn on(u: &ChainUi, s: &Snapshot) -> String {
        u.choices(s)[u.cursor].id.clone()
    }

    #[test]
    fn a_rescan_keeps_the_cursor_on_the_agent_the_user_moved_to() {
        let before = snap(&[
            ("claude", true, LoggedIn),
            ("codex", true, LoggedIn),
            ("pi", true, Unknown),
        ]);
        let mut u = started(&before);
        u.cursor = u
            .choices(&before)
            .iter()
            .position(|c| c.id == "pi")
            .unwrap();
        // The scan no longer finds Codex, so Pi moves from index 2 to index 1: a cursor that kept
        // only its number would now sit on the missing Codex.
        let after = snap(&[
            ("claude", true, LoggedIn),
            ("codex", false, LoggedIn),
            ("pi", true, Unknown),
        ]);
        assert_eq!(u.cursor, 2, "before: Pi is third");
        u.resettle(&after, &before);
        assert_eq!(u.cursor, 1, "after: Pi is second");
        assert_eq!(on(&u, &after), "pi");
    }

    #[test]
    fn a_rescan_that_removes_the_agent_under_the_cursor_goes_back_to_the_default() {
        let before = ready();
        let mut u = started(&before);
        u.cursor = u
            .choices(&before)
            .iter()
            .position(|c| c.id == "codex")
            .unwrap();
        let after = snap(&[("claude", true, LoggedIn), ("codex", false, LoggedIn)]);
        u.resettle(&after, &before);
        assert_eq!(on(&u, &after), "claude");
    }

    #[test]
    fn a_rescan_that_changes_the_step_puts_the_cursor_on_the_new_default() {
        let before = ready();
        let mut u = started(&before);
        u.enter(&before);
        u.cursor = u
            .choices(&before)
            .iter()
            .position(|c| c.id == "claude")
            .unwrap();
        // Claude, the answered builder, is gone: back to the Builder question, on its default.
        let after = snap(&[("claude", false, LoggedIn), ("codex", true, LoggedIn)]);
        u.resettle(&after, &before);
        assert_eq!(u.step, Step::Builder);
        assert_eq!(on(&u, &after), "codex");
    }

    fn moved_to(u: &mut ChainUi, s: &Snapshot, id: &str) {
        u.cursor = u.choices(s).iter().position(|c| c.id == id).unwrap();
    }

    fn labels(u: &ChainUi, s: &Snapshot) -> Vec<String> {
        u.choices(s).into_iter().map(|c| c.label).collect()
    }

    static ADMIN: crate::catalog::Tool = crate::catalog::Tool {
        id: "fake-admin",
        name: "Fake Admin",
        groups: &[],
        install: Some(&["apt-get", "install", "fake admin"]),
        install_needs_admin: true,
        check: None,
        methods: &[],
        docs: "",
        connects: "",
        selectable: true,
        verified: None,
    };

    #[test]
    fn herdr_missing_offers_its_verified_install_in_the_workspace_question() {
        let mut s = snap(&[("claude", true, LoggedIn)]);
        s.herdr.found = false;
        let mut u = started(&s);
        assert_eq!(u.step, Step::Workspace);
        let cmd = quote_cmd(tool("herdr").unwrap().install.unwrap());
        let c = u.choices(&s);
        assert_eq!(c[0].label, "Install Herdr in the background");
        assert!(c[0].note.contains(&cmd), "{c:?}");
        assert_eq!(u.enter(&s), Some(Want::Install("herdr")));
        u.work.insert("herdr".into(), Work::Installing { secs: 3 });
        assert!(
            u.choices(&s).is_empty(),
            "while it installs there is nothing to pick"
        );
        assert_eq!(u.step, Step::Workspace, "the chain waits for Herdr");
    }

    #[test]
    fn an_agent_without_a_verified_command_gets_guidance_not_an_install() {
        let s = snap(&[("claude", false, LoggedIn), ("codex", true, LoggedIn)]);
        let mut u = started(&s);
        moved_to(&mut u, &s, "claude");
        assert_eq!(u.enter(&s), None, "nothing is run");
        assert_eq!(u.step, Step::Install);
        assert_eq!(u.subject.as_deref(), Some("claude"));
        assert_eq!(labels(&u, &s), ["Choose another agent"]);
        let b = u.blocker(&s).unwrap();
        assert!(
            b.contains("docs.anthropic.com") && b.contains("press r"),
            "{b}"
        );
    }

    #[test]
    fn an_install_that_needs_admin_rights_offers_to_copy_never_to_run() {
        let c = install_choices(&ADMIN, false);
        let ids: Vec<_> = c.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, ["copy", "another"]);
        assert!(c[0].note.contains("apt-get install 'fake admin'"), "{c:?}");
    }

    #[test]
    fn installing_in_the_background_answers_the_question_and_moves_on() {
        let mut s = snap(&[("claude", false, LoggedIn), ("codex", true, LoggedIn)]);
        s.herdr.found = true;
        // Give the catalog tool a runnable install the way Herdr has one: use herdr as the agent.
        let mut u = started(&s);
        u.step = Step::Install;
        u.subject = Some("herdr".into());
        assert_eq!(u.choices(&s)[0].id, "install");
        assert_eq!(u.enter(&s), Some(Want::Install("herdr")));
    }

    #[test]
    fn a_missing_answered_agent_survives_a_scan_while_its_install_runs() {
        let s = snap(&[("claude", false, LoggedIn), ("codex", true, LoggedIn)]);
        let mut u = started(&s);
        u.builder = Some("claude".into());
        u.step = Step::Reviewer;
        u.work.insert("claude".into(), Work::Installing { secs: 9 });
        u.settle(&s);
        assert_eq!(
            u.builder.as_deref(),
            Some("claude"),
            "the answer stays while it installs"
        );
        u.work.clear();
        u.settle(&s);
        assert_eq!(u.builder, None, "no install running: the stale answer goes");
    }

    #[test]
    fn the_same_missing_agent_as_builder_and_reviewer_is_installed_once() {
        let s = snap(&[("claude", false, LoggedIn), ("codex", true, LoggedIn)]);
        let mut u = started(&s);
        u.builder = Some("claude".into());
        u.step = Step::Reviewer;
        u.work.insert("claude".into(), Work::Installing { secs: 2 });
        moved_to(&mut u, &s, "claude");
        assert_eq!(u.enter(&s), None, "its install already runs");
        assert_eq!(u.reviewer.as_deref(), Some("claude"));
    }

    #[test]
    fn a_finished_install_that_is_not_logged_in_becomes_a_connect_question() {
        let before = snap(&[("claude", false, LoggedIn), ("codex", true, LoggedIn)]);
        let mut u = started(&before);
        u.builder = Some("claude".into());
        u.reviewer = Some("codex".into());
        u.step = Step::End;
        u.work
            .insert("claude".into(), Work::Installing { secs: 30 });
        u.settle(&before);
        assert_eq!(
            (u.step, u.subject.as_deref()),
            (Step::Connect, Some("claude"))
        );
        let after = snap(&[("claude", true, Unknown), ("codex", true, LoggedIn)]);
        u.work.clear();
        u.settle(&after);
        assert_eq!(
            (u.step, u.subject.as_deref()),
            (Step::Connect, Some("claude"))
        );
    }

    #[test]
    fn ready_agents_go_straight_to_the_end() {
        let s = ready();
        let mut u = started(&s);
        u.enter(&s);
        u.enter(&s);
        assert_eq!(u.step, Step::End);
    }

    #[test]
    fn a_failed_install_keeps_the_subject_and_offers_retry_and_another_agent() {
        let s = snap(&[("claude", false, LoggedIn), ("codex", true, LoggedIn)]);
        let mut u = started(&s);
        u.builder = Some("claude".into());
        u.reviewer = Some("codex".into());
        u.step = Step::End;
        u.work.insert(
            "claude".into(),
            Work::InstallFailed {
                reason: "did not finish".into(),
                fix: None,
            },
        );
        u.settle(&s);
        assert_eq!(
            (u.step, u.subject.as_deref()),
            (Step::Connect, Some("claude"))
        );
        assert_eq!(
            labels(&u, &s),
            ["Choose another agent"],
            "no verified command, no retry"
        );
    }

    /// Claude Code (or any agent) installed and not logged in, the other one ready, both answered.
    fn connecting(kind: &str) -> (Snapshot, ChainUi) {
        let other = if kind == "claude" { "codex" } else { "claude" };
        let s = snap(&[(kind, true, LoggedOut), (other, true, LoggedIn)]);
        let mut u = started(&s);
        u.builder = Some(kind.into());
        u.reviewer = Some(other.into());
        u.step = Step::End;
        u.settle(&s);
        assert_eq!((u.step, u.subject.as_deref()), (Step::Connect, Some(kind)));
        (s, u)
    }

    fn row_of(u: &mut ChainUi, s: &Snapshot, label: &str) {
        u.cursor = u
            .choices(s)
            .iter()
            .position(|c| c.label == label)
            .unwrap_or_else(|| panic!("no row {label}: {:?}", labels(u, s)));
    }

    #[test]
    fn connect_lists_the_catalogs_methods_without_masked_key_entry() {
        let (s, u) = connecting("codex");
        let rows = methods_for(tool("codex").unwrap(), &s);
        let stores = rows
            .iter()
            .filter(|r| matches!(r.act, crate::tui::connect::Act::Store(_)))
            .count();
        assert!(
            stores > 0,
            "codex has a key-entry method, so this test means something"
        );
        assert_eq!(
            u.choices(&s).len(),
            rows.len() - stores + 1,
            "{:?}",
            labels(&u, &s)
        );
        assert_eq!(
            labels(&u, &s).last().map(String::as_str),
            Some("Choose another agent")
        );
    }

    #[test]
    fn w3_logged_out_agent_offers_log_in_first() {
        let (s, u) = connecting("claude");
        assert_eq!(labels(&u, &s)[0], "Log in");
    }

    #[test]
    fn log_in_returns_the_tools_own_login_command_and_the_next_scan_decides() {
        let (s, mut u) = connecting("claude");
        row_of(&mut u, &s, "Log in");
        assert_eq!(
            u.enter(&s),
            Some(Want::Attach(&["claude", "auth", "login"]))
        );
        assert_eq!(
            u.step,
            Step::Connect,
            "nothing is assumed: the scan decides"
        );
    }

    #[test]
    fn an_api_key_method_shows_where_to_put_it_and_runs_nothing() {
        let (s, mut u) = connecting("claude");
        row_of(&mut u, &s, "API key");
        assert_eq!(u.enter(&s), None);
        let h = u.hint.clone().unwrap();
        assert!(
            h.contains("ANTHROPIC_API_KEY") && h.contains("press r"),
            "{h}"
        );
    }

    #[test]
    fn check_now_returns_the_live_check() {
        let (s, mut u) = connecting("claude");
        row_of(&mut u, &s, "Check now");
        assert_eq!(u.enter(&s), Some(Want::Check("claude")));
    }

    #[test]
    fn a_passed_check_collapses_to_a_history_line_with_the_latency() {
        let (s, mut u) = connecting("claude");
        u.work.insert("claude".into(), Work::Passed { ms: 6800 });
        u.settle(&s);
        assert_eq!(u.step, Step::End);
        assert!(
            u.history(&s)
                .iter()
                .any(|h| h.0 == "Builder" && h.2 == "replied in 6.8 s"),
            "{:?}",
            u.history(&s)
        );
    }

    #[test]
    fn a_failed_check_keeps_the_question_and_never_picks_another_agent() {
        let (s, mut u) = connecting("claude");
        u.work.insert(
            "claude".into(),
            Work::CheckFailed {
                reason: "not logged in".into(),
                fix: Some("claude auth login".into()),
            },
        );
        u.settle(&s);
        assert_eq!(
            (u.step, u.subject.as_deref()),
            (Step::Connect, Some("claude"))
        );
        assert_eq!(
            u.builder.as_deref(),
            Some("claude"),
            "the choice is unchanged"
        );
        assert_eq!(labels(&u, &s)[0], "Check again");
        u.cursor = 0;
        assert_eq!(u.enter(&s), Some(Want::Check("claude")));
    }

    #[test]
    fn while_a_check_runs_there_is_nothing_to_pick() {
        let (s, mut u) = connecting("claude");
        u.work.insert("claude".into(), Work::Checking { secs: 2 });
        assert!(u.choices(&s).is_empty());
    }

    #[test]
    fn choosing_another_agent_from_connect_clears_that_answer() {
        let (s, mut u) = connecting("claude");
        row_of(&mut u, &s, "Choose another agent");
        assert_eq!(u.enter(&s), None);
        assert_eq!(
            (u.step, u.builder.clone(), u.reviewer.clone()),
            (Step::Builder, None, None)
        );
    }

    #[test]
    fn a_missing_agent_says_how_to_install_when_nothing_can_be_run() {
        let s = snap(&[("claude", false, LoggedIn), ("codex", true, LoggedIn)]);
        let u = started(&s);
        let c = u
            .choices(&s)
            .into_iter()
            .find(|c| c.id == "claude")
            .unwrap();
        assert_eq!(c.cta.as_deref(), Some("How to install Claude Code"));
    }

    #[test]
    fn a_running_install_and_a_running_check_have_a_wait_line_with_the_clock() {
        let (s, mut u) = connecting("claude");
        assert_eq!(u.wait_line(&s), None, "nothing runs yet");
        u.work.insert("claude".into(), Work::Checking { secs: 42 });
        assert!(u.wait_line(&s).unwrap().contains("00:42"));
        u.work
            .insert("claude".into(), Work::Installing { secs: 75 });
        let w = u.wait_line(&s).unwrap();
        assert!(w.contains("01:15") && w.contains("Claude Code"), "{w}");
    }

    #[test]
    fn a_failure_is_three_lines_with_the_exact_fix_and_a_hint_is_one_line() {
        let (s, mut u) = connecting("claude");
        u.work.insert(
            "claude".into(),
            Work::CheckFailed {
                reason: "not logged in".into(),
                fix: Some("claude auth login".into()),
            },
        );
        u.settle(&s);
        let b = u.blocker(&s).unwrap();
        let lines: Vec<_> = b.lines().collect();
        assert_eq!(lines.len(), 3, "{b}");
        assert!(lines[0].starts_with("Claude Code: not logged in"), "{b}");
        assert!(lines[1].starts_with("Unchanged:"), "{b}");
        assert_eq!(lines[2], "Fix: claude auth login");
        u.work.clear();
        u.settle(&s);
        row_of(&mut u, &s, "API key");
        u.enter(&s);
        assert!(u.blocker(&s).unwrap().contains("ANTHROPIC_API_KEY"));
    }

    #[test]
    fn herdr_needs_no_box_because_its_install_row_already_shows_the_command() {
        let mut s = snap(&[("claude", true, LoggedIn)]);
        s.herdr.found = false;
        let mut u = started(&s);
        assert_eq!(u.blocker(&s), None, "the command is on the row");
        u.work.insert("herdr".into(), Work::Installing { secs: 1 });
        assert_eq!(
            u.blocker(&s),
            None,
            "and a box must not contradict a running install"
        );
    }

    #[test]
    fn a_failed_herdr_install_shows_its_reason_and_fix_where_the_running_one_shows_nothing() {
        let mut s = snap(&[("claude", true, LoggedIn)]);
        s.herdr.found = false;
        let mut u = started(&s);
        u.work.insert(
            "herdr".into(),
            Work::InstallFailed {
                reason: "download failed".into(),
                fix: Some("check the network, then retry".into()),
            },
        );
        let b = u.blocker(&s).expect("the failure is shown");
        assert!(b.contains("Herdr: download failed."), "{b}");
        assert!(b.contains("Fix: check the network, then retry"), "{b}");
    }

    #[test]
    fn a_hint_chosen_after_a_failed_check_is_shown_below_the_failure() {
        let (s, mut u) = connecting("claude");
        u.work.insert(
            "claude".into(),
            Work::CheckFailed {
                reason: "not logged in".into(),
                fix: Some("claude auth login".into()),
            },
        );
        u.settle(&s);
        row_of(&mut u, &s, "API key");
        u.enter(&s);
        let b = u.blocker(&s).unwrap();
        assert!(
            b.contains("Fix: claude auth login"),
            "the failure stays: {b}"
        );
        assert!(b.contains("ANTHROPIC_API_KEY"), "and the hint follows: {b}");
    }

    #[test]
    fn a_failed_check_offers_check_again_once_not_check_now_as_well() {
        let (s, mut u) = connecting("claude");
        u.work.insert(
            "claude".into(),
            Work::CheckFailed {
                reason: "not logged in".into(),
                fix: None,
            },
        );
        u.settle(&s);
        let l = labels(&u, &s);
        assert!(
            l.contains(&"Check again".to_string()) && !l.contains(&"Check now".to_string()),
            "{l:?}"
        );
    }

    #[test]
    fn choose_another_agent_has_no_note_to_crowd_its_label() {
        let (s, u) = connecting("claude");
        let c = u.choices(&s).pop().unwrap();
        assert_eq!(
            (c.label.as_str(), c.note.as_str()),
            ("Choose another agent", "")
        );
    }

    #[test]
    fn the_recommended_agents_come_first_among_the_installed() {
        let s = snap(&[
            ("pi", true, Unknown),
            ("codex", true, LoggedIn),
            ("claude", true, LoggedIn),
        ]);
        let ids: Vec<_> = started(&s).choices(&s).into_iter().map(|c| c.id).collect();
        assert_eq!(ids, ["claude", "codex", "pi"]);
    }

    #[test]
    fn the_reviewer_cursor_avoids_the_builder_when_nothing_is_verified() {
        let s = snap(&[("claude", true, Unknown), ("codex", true, Unknown)]);
        let mut u = started(&s);
        assert_eq!(u.default_index(&s), None, "never a silent default");
        u.enter(&s);
        assert_eq!(u.builder.as_deref(), Some("claude"));
        assert_eq!(u.choices(&s)[u.cursor].id, "codex");
    }

    #[test]
    fn only_the_three_recommended_missing_agents_are_listed() {
        let s = snap(&[
            ("pi", false, Unknown),
            ("amp", false, Unknown),
            ("gemini", false, Unknown),
            ("cursor", false, Unknown),
            ("codex", false, Unknown),
            ("claude", false, Unknown),
        ]);
        let ids: Vec<_> = started(&s).choices(&s).into_iter().map(|c| c.id).collect();
        assert_eq!(ids, ["claude", "codex", "gemini"]);
    }

    #[test]
    fn back_goes_one_step_and_clears_that_answer_and_the_later_ones() {
        let s = ready();
        let mut u = started(&s);
        u.enter(&s);
        u.back(&s);
        assert_eq!(
            (u.step, u.builder.clone(), u.reviewer.clone()),
            (Step::Builder, None, None)
        );
        u.enter(&s);
        u.enter(&s);
        assert_eq!(u.step, Step::End);
        u.back(&s);
        assert_eq!(u.step, Step::Reviewer);
        assert_eq!(
            (u.builder.as_deref(), u.reviewer.clone()),
            (Some("claude"), None)
        );
    }
}
