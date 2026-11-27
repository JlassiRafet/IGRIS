//! The chain's state and rules, with no drawing and no I/O: what is asked, what the defaults are,
//! what a re-scan does to the answers.
use crate::catalog::tool;
use crate::tui::install::quote_cmd;
use igris_detect::{Agent, Login, Snapshot};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Step {
    /// Herdr is missing: nothing to ask until it is installed.
    #[default]
    Workspace,
    Builder,
    Reviewer,
    /// Slice 1 ends here.
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
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ChainUi {
    pub step: Step,
    pub builder: Option<String>,
    pub reviewer: Option<String>,
    pub cursor: usize,
}

impl ChainUi {
    pub fn new() -> Self {
        Self::default()
    }
    /// After every scan: skip Workspace once Herdr is found, drop answers whose agent is gone (and
    /// every later one), and put the cursor on the default.
    pub fn settle(&mut self, snap: &Snapshot) {
        if !snap.herdr.found {
            *self = Self::default();
            return;
        }
        if self.step == Step::Workspace {
            self.step = Step::Builder;
        }
        let found = |k: &str| snap.agents.iter().any(|a| a.kind == k && a.found);
        if self.builder.as_deref().is_some_and(|b| !found(b)) {
            self.builder = None;
            self.reviewer = None;
            self.step = Step::Builder;
        } else if self.reviewer.as_deref().is_some_and(|r| !found(r)) {
            self.reviewer = None;
            self.step = Step::Reviewer;
        }
        self.reset_cursor(snap);
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
        if !matches!(self.step, Step::Builder | Step::Reviewer) {
            return vec![];
        }
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
                }
            })
            .collect()
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

    /// Answer the active question with the highlighted option. A missing agent cannot be chosen.
    pub fn enter(&mut self, snap: &Snapshot) {
        let v = ordered(snap);
        let Some(a) = v.get(self.cursor).filter(|a| a.found) else {
            return;
        };
        match self.step {
            Step::Builder => {
                self.builder = Some(a.kind.clone());
                self.step = Step::Reviewer;
                self.reset_cursor(snap);
            }
            Step::Reviewer => {
                self.reviewer = Some(a.kind.clone());
                self.step = Step::End;
            }
            Step::Workspace | Step::End => {}
        }
    }

    /// One step back; the answer of the step landed on, and every later one, is cleared.
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
        let state_of = |kind: &str| {
            snap.agents
                .iter()
                .find(|a| a.kind == kind)
                .map_or("not installed", |a| status(a).0)
                .to_string()
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

    /// What stops the chain on this step, in words, with the tool's documented install command.
    pub fn blocker(&self, snap: &Snapshot) -> Option<String> {
        match self.step {
            Step::Workspace => Some(format!(
                "Herdr is not installed. Install it, then press r to re-scan. {}",
                install_line("herdr")
            )),
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

fn name(kind: &str) -> String {
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
        assert_eq!(u.step, Step::Builder, "a missing agent cannot be chosen");
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
        assert!(u.blocker(&s).unwrap().contains("Herdr"));
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
        // The scan finds the same agents again, and Codex is now logged out, so the order changes.
        let after = snap(&[
            ("claude", true, LoggedIn),
            ("codex", true, LoggedOut),
            ("pi", true, Unknown),
        ]);
        u.resettle(&after, &before);
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
