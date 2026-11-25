//! The Verify step: scan again, then one live check for each tool you chose. Review stays locked
//! until every one of them passes (or, where allowed, is skipped). A check only reads: nothing here
//! changes a choice, and a tool that fails is never swapped for another.
use super::action::Action;
use super::connect::{passed_line, running_word};
use super::hit::HitMap;
use super::state::{Effect, State};
use super::style::Role;
use super::theme::Theme;
use super::tools_screen::{AssistantChoice, CheckState, installed};
use super::widgets::{detail_strip, fit, pad, status_glyph};
use crate::catalog::{Tool, tool};
use crate::check::view::Status;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct VerifyUi {
    pub cursor: usize,
    /// A scan was asked for by this step; when it lands, the checks that are due start.
    pub pending: bool,
    /// The scan was asked for by `r`: every check is due, not only the ones with no result yet.
    pub recheck_all: bool,
    /// Tools whose check starts now (the loop drains it: `apply` cannot start a job).
    pub queue: Vec<&'static str>,
}

/// One tool that was chosen on the Tools step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chosen {
    pub id: &'static str,
    pub roles: Vec<&'static str>,
    /// Workspace, builder and reviewer cannot be skipped: the pipeline cannot run without them.
    pub required: bool,
}

/// Herdr, the builder, the reviewer and the assistant (when it is a tool). The same tool in two
/// roles is one entry. `rules` is built in and has nothing to check.
pub fn chosen(s: &State) -> Vec<Chosen> {
    let (Some(_), Some(form)) = (&s.snapshot, &s.pipeline.form) else {
        return vec![];
    };
    let mut v: Vec<Chosen> = vec![];
    let mut add = |id: &'static str, role: &'static str, required: bool| match v
        .iter_mut()
        .find(|c| c.id == id)
    {
        Some(c) => {
            c.roles.push(role);
            c.required |= required;
        }
        None => v.push(Chosen {
            id,
            roles: vec![role],
            required,
        }),
    };
    add("herdr", "Workspace", true);
    for (sel, role) in [(&form.builder, "Builder"), (&form.reviewer, "Reviewer")] {
        if let Some(t) = sel.current().and_then(|i| tool(&i.label)) {
            add(t.id, role, true);
        }
    }
    if let AssistantChoice::Tool(id) = s.tools.assistant {
        add(id, "Assistant", false);
    }
    v
}

fn has_check(t: &Tool) -> bool {
    t.check.is_some()
}

/// Whether this tool lets Review open.
fn clear(s: &State, c: &Chosen) -> bool {
    let Some(t) = tool(c.id) else {
        return false;
    };
    match s.tools.checks.get(c.id) {
        Some(CheckState::Passed { .. }) => true,
        Some(CheckState::Skipped) if !c.required => true,
        // Nothing to run for this tool: a required one counts once it is on this machine, so Review
        // cannot be locked for good; an optional one must be skipped on purpose.
        _ if !has_check(t) => {
            c.required
                && s.snapshot
                    .as_ref()
                    .is_some_and(|snap| installed(t, snap, &s.tools))
        }
        _ => false,
    }
}

/// Every chosen tool is clear: Review may open.
pub fn verify_ready(s: &State) -> bool {
    chosen(s).iter().all(|c| clear(s, c))
}

/// The names of the chosen tools that still hold Review back.
pub fn pending_names(s: &State) -> Vec<&'static str> {
    chosen(s)
        .iter()
        .filter(|c| !clear(s, c))
        .filter_map(|c| tool(c.id).map(|t| t.name))
        .collect()
}

/// Why `id` cannot be skipped, when it is a chosen tool the pipeline needs.
pub fn refuse_skip(s: &State, id: &str) -> Option<String> {
    let c = chosen(s).into_iter().find(|c| c.id == id && c.required)?;
    let name = tool(id).map_or(id, |t| t.name);
    Some(format!(
        "{name} is your {}: it cannot be skipped.",
        c.roles.join(" and ").to_lowercase()
    ))
}

/// A scan is wanted (now, or after the one that is running), then the checks that are due.
fn ask_for_scan(s: &mut State, all: bool) -> Effect {
    s.verify.pending = true;
    s.verify.recheck_all |= all;
    if s.detecting {
        return Effect::None;
    }
    s.detecting = true;
    Effect::Rescan
}

/// Opening the step: scan, then check what has no result yet this session.
pub fn enter(s: &mut State) -> Effect {
    ask_for_scan(s, false)
}

/// `r`: scan again and check everything again.
pub fn recheck_all(s: &mut State) -> Effect {
    ask_for_scan(s, true)
}

/// A scan landed: queue the checks that are due.
pub fn after_scan(s: &mut State) {
    if !s.verify.pending {
        return;
    }
    s.verify.pending = false;
    let all = std::mem::take(&mut s.verify.recheck_all);
    let due: Vec<&'static str> = chosen(s)
        .iter()
        .filter(|c| tool(c.id).is_some_and(has_check))
        .filter(|c| {
            all || !matches!(
                s.tools.checks.get(c.id),
                Some(CheckState::Passed { .. } | CheckState::Failed(_) | CheckState::Skipped)
            )
        })
        .map(|c| c.id)
        .collect();
    s.verify.queue = due;
}

fn focused(s: &State) -> Option<Chosen> {
    let list = chosen(s);
    let at = s.verify.cursor.min(list.len().saturating_sub(1));
    list.into_iter().nth(at)
}

fn run_one(s: &mut State, i: usize) -> Effect {
    s.verify.cursor = i;
    let Some(c) = focused(s) else {
        return Effect::None;
    };
    match tool(c.id) {
        Some(t) if has_check(t) => super::connect::start_check(s, c.id),
        Some(t) => {
            s.toast = Some(format!("{} has no live check.", t.name));
            Effect::None
        }
        None => Effect::None,
    }
}

fn skip_one(s: &mut State) {
    let Some(c) = focused(s) else {
        return;
    };
    if let Some(why) = refuse_skip(s, c.id) {
        s.toast = Some(why);
    } else {
        s.tools.checks.insert(c.id.to_string(), CheckState::Skipped);
    }
}

pub fn update(s: &mut State, a: Action) -> Effect {
    let last = chosen(s).len().saturating_sub(1);
    let at = s.verify.cursor.min(last);
    match a {
        Action::Up => s.verify.cursor = at.saturating_sub(1),
        Action::Down => s.verify.cursor = (at + 1).min(last),
        Action::Home => s.verify.cursor = 0,
        Action::End => s.verify.cursor = last,
        Action::Activate | Action::Toggle => return run_one(s, at),
        Action::Select(i) => return run_one(s, i.min(last)),
        Action::Skip => skip_one(s),
        _ => {}
    }
    Effect::None
}

/// What a row says about its tool.
fn state_text(s: &State, c: &Chosen) -> (Status, String) {
    let Some(t) = tool(c.id) else {
        return (Status::Info, String::new());
    };
    match s.tools.checks.get(c.id) {
        Some(CheckState::Passed { ms }) => (Status::Ready, passed_line(t, *ms).replace("✓ ", "")),
        Some(CheckState::Failed(f)) => (Status::NeedsStep, format!("✗ {}", f.reason)),
        Some(CheckState::Skipped) => (Status::Optional, "skipped".into()),
        Some(CheckState::Running) => (
            Status::Info,
            s.snapshot
                .as_ref()
                .map_or("checking…", |snap| running_word(t, snap))
                .into(),
        ),
        _ if !has_check(t) && clear(s, c) => (Status::Ready, "no live check · installed".into()),
        _ if !has_check(t) => (Status::Optional, "no live check · press s to skip".into()),
        _ => (Status::Optional, "not checked yet".into()),
    }
}

fn role_of(s: Status) -> Role {
    match s {
        Status::Ready => Role::Ok,
        Status::NeedsStep => Role::Bad,
        Status::Optional => Role::Optional,
        _ => Role::Info,
    }
}

pub fn draw(f: &mut Frame, area: Rect, s: &State, t: &Theme, hits: &mut HitMap, clickable: bool) {
    let list = chosen(s);
    let dim = t.style(Role::Dim);
    if list.is_empty() {
        let msg = if s.snapshot.is_none() {
            "Scanning this machine…"
        } else {
            "Nothing is chosen yet. Choose your tools first (key 2, Tools)."
        };
        f.render_widget(Paragraph::new(msg), area);
        return;
    }
    let w = area.width as usize;
    let head = format!(
        "Verify · {} tool{}",
        list.len(),
        if list.len() == 1 { "" } else { "s" }
    );
    f.render_widget(
        Paragraph::new(vec![
            Line::styled(head, t.style(Role::Text).add_modifier(Modifier::BOLD)),
            Line::styled(
                fit(
                    t,
                    "Enter checks one tool, r checks them all again, s skips (not your builder, reviewer or Herdr).",
                    w,
                ),
                dim,
            ),
        ]),
        Rect::new(area.x, area.y, area.width, 2.min(area.height)),
    );
    let strip = 4u16.min(area.height / 3);
    let rows_h = area.height.saturating_sub(3 + strip) as usize;
    let at = s.verify.cursor.min(list.len() - 1);
    let first = (at + 1).saturating_sub(rows_h.max(1));
    for (n, c) in list.iter().enumerate().skip(first).take(rows_h) {
        let (status, text) = state_text(s, c);
        let y = area.y + 3 + (n - first) as u16;
        let name = tool(c.id).map_or(c.id, |t| t.name);
        let roles = c.roles.join(" + ");
        let used = 3 + 18 + 1 + 20 + 1;
        let line = Line::from(vec![
            Span::styled(if n == at { "▌" } else { " " }, t.style(Role::BorderFocus)),
            Span::styled(
                format!("{} ", status_glyph(t, status)),
                t.style(role_of(status)),
            ),
            Span::styled(
                pad(t, name, 18),
                t.style(Role::Text).add_modifier(if n == at {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
            ),
            Span::raw(" "),
            Span::styled(pad(t, &roles, 20), dim),
            Span::raw(" "),
            Span::styled(
                fit(t, &text, w.saturating_sub(used)),
                t.style(role_of(status)),
            ),
        ]);
        let row = Rect::new(area.x, y, area.width, 1);
        f.render_widget(Paragraph::new(line), row);
        if clickable {
            hits.add(row, Action::Select(n));
        }
    }
    if strip > 0
        && let Some(c) = focused(s)
    {
        let mut lines = vec![Line::styled(
            tool(c.id).map_or(c.id, |t| t.name).to_string(),
            t.style(Role::Text).add_modifier(Modifier::BOLD),
        )];
        match s.tools.checks.get(c.id) {
            Some(CheckState::Failed(f)) => {
                for l in super::connect::failure_lines(c.id, f) {
                    lines.push(Line::styled(fit(t, &l, w * 2), t.style(Role::Bad)));
                }
            }
            _ => lines.push(Line::styled(state_text(s, &c).1, dim)),
        }
        let r = Rect::new(area.x, area.bottom() - strip, area.width, strip);
        f.render_widget(detail_strip(t, lines), r);
    }
}

/// For tests of other screens: mark every chosen tool as having passed, so Review opens.
#[cfg(test)]
pub(crate) fn clear_all(s: &mut State) {
    for c in chosen(s) {
        s.tools
            .checks
            .insert(c.id.to_string(), CheckState::Passed { ms: 1 });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::job::{JobEnd, JobId};
    use crate::tui::detect_worker::Msg;
    use crate::tui::state::{Screen, update as step};
    use crate::tui::tools_screen::tests::{app, both, snap};
    use igris_detect::Login;

    fn ended(code: Option<i32>, lines: &[&str]) -> JobEnd {
        JobEnd {
            code,
            tail: lines.iter().map(|l| (*l).to_string()).collect(),
            timed_out: false,
            started: true,
        }
    }

    fn job(id: &str) -> JobId {
        JobId(
            crate::catalog::TOOLS
                .iter()
                .position(|t| t.id == id)
                .unwrap() as u32,
        )
    }

    fn ids(s: &State) -> Vec<&'static str> {
        chosen(s).iter().map(|c| c.id).collect()
    }

    /// The loop's part: start every queued check, as the loop does after each batch of messages.
    fn drain(s: &mut State) -> Vec<&'static str> {
        let mut started = vec![];
        for id in std::mem::take(&mut s.verify.queue) {
            if let Effect::Check(id) = super::super::connect::start_check(s, id) {
                started.push(id);
            }
        }
        started
    }

    /// An app on the Tools screen with the assistant answered (CLI only).
    fn settled() -> State {
        let mut s = app(both(), None);
        s.tools.assistant = AssistantChoice::CliOnly;
        s
    }

    #[test]
    fn the_rail_has_six_steps_in_order() {
        let titles: Vec<&str> = Screen::ALL.iter().map(|s| s.title()).collect();
        assert_eq!(
            titles,
            ["Scan", "Tools", "Pipeline", "Verify", "Review", "Test"]
        );
    }

    #[test]
    fn the_chosen_tools_are_one_entry_each_with_their_roles() {
        let mut s = settled();
        assert_eq!(ids(&s), ["herdr", "claude", "codex"]);
        let both_roles = chosen(&s);
        assert_eq!(both_roles[1].roles, ["Builder"]);
        // The same tool as builder and reviewer, and a tool as the assistant.
        let f = s.pipeline.form.as_mut().unwrap();
        let at = f
            .reviewer
            .items
            .iter()
            .position(|i| i.label == "claude")
            .unwrap();
        f.reviewer.choose(at);
        s.tools.assistant = AssistantChoice::Tool("hermes");
        assert_eq!(ids(&s), ["herdr", "claude", "hermes"]);
        assert_eq!(chosen(&s)[1].roles, ["Builder", "Reviewer"]);
        assert!(!chosen(&s)[2].required, "the assistant may be skipped");
    }

    #[test]
    fn verify_rescans_then_runs_one_live_check_per_chosen_tool_with_latency() {
        let mut s = settled();
        s.screen = Screen::Pipeline;
        assert_eq!(step(&mut s, Action::Goto(Screen::Verify)), Effect::Rescan);
        assert!(s.detecting && s.verify.pending);
        assert!(s.verify.queue.is_empty(), "no check before the scan lands");
        s.apply(Msg::Detected(Box::new(both())));
        let started = drain(&mut s);
        assert_eq!(
            started,
            ["herdr", "claude", "codex"],
            "one check each, no duplicates"
        );
        for id in &started {
            s.apply(Msg::Job(job(id), ended(Some(0), &["ok"])));
        }
        let text =
            |id: &str| state_text(&s, &chosen(&s).into_iter().find(|c| c.id == id).unwrap()).1;
        assert!(
            text("claude").starts_with("Claude Code replied in "),
            "{}",
            text("claude")
        );
        assert!(
            text("herdr").starts_with("Herdr answered in "),
            "{}",
            text("herdr")
        );
        assert!(verify_ready(&s));
    }

    #[test]
    fn entering_again_checks_only_what_has_no_result_but_r_checks_everything() {
        let mut s = settled();
        for id in ["herdr", "claude"] {
            s.tools
                .checks
                .insert(id.into(), CheckState::Passed { ms: 5 });
        }
        s.screen = Screen::Pipeline;
        step(&mut s, Action::Goto(Screen::Verify));
        s.apply(Msg::Detected(Box::new(both())));
        assert_eq!(
            drain(&mut s),
            ["codex"],
            "a model call is not repeated for nothing"
        );
        s.apply(Msg::Job(job("codex"), ended(Some(0), &["ok"])));
        step(&mut s, Action::Rescan);
        s.apply(Msg::Detected(Box::new(both())));
        assert_eq!(drain(&mut s), ["herdr", "claude", "codex"]);
        assert!(!s.verify.recheck_all && !s.verify.pending);
    }

    #[test]
    fn review_is_locked_until_every_chosen_tool_passes_or_is_skipped() {
        let mut s = settled();
        s.tools.assistant = AssistantChoice::Tool("openclaw");
        s.screen = Screen::Verify;
        step(&mut s, Action::Goto(Screen::Review));
        assert_eq!(s.screen, Screen::Verify, "locked: you stay where you were");
        let toast = s.toast.clone().unwrap();
        for name in ["Herdr", "Claude Code", "Codex", "OpenClaw"] {
            assert!(toast.contains(name), "{toast}");
        }
        for id in ["herdr", "claude", "codex"] {
            s.tools
                .checks
                .insert(id.into(), CheckState::Passed { ms: 1 });
        }
        assert_eq!(
            pending_names(&s),
            ["OpenClaw"],
            "no live check: it must be skipped on purpose"
        );
        step(&mut s, Action::Goto(Screen::Review));
        assert_eq!(s.screen, Screen::Verify);
        s.verify.cursor = 3;
        step(&mut s, Action::Skip);
        step(&mut s, Action::Goto(Screen::Review));
        assert_eq!(s.screen, Screen::Review);
    }

    #[test]
    fn the_lock_holds_for_every_way_into_review() {
        let mut s = settled();
        s.screen = Screen::Verify;
        step(&mut s, Action::NextScreen);
        assert_eq!(s.screen, Screen::Verify, "] cannot step around the lock");
        s.screen = Screen::Test;
        step(&mut s, Action::PrevScreen);
        assert_eq!(s.screen, Screen::Test, "[ from Test cannot either");
        step(&mut s, Action::Goto(Screen::Check));
        assert_eq!(s.screen, Screen::Check, "every other screen stays open");
    }

    #[test]
    fn builder_and_reviewer_cannot_be_skipped() {
        let mut s = settled();
        s.screen = Screen::Verify;
        for at in 0..3 {
            s.verify.cursor = at;
            step(&mut s, Action::Skip);
        }
        assert!(s.tools.checks.is_empty(), "{:?}", s.tools.checks);
        assert!(s.toast.as_deref().unwrap().contains("cannot be skipped"));
        // The same refusal in the failure box of a check.
        s.tools.checks.insert(
            "claude".into(),
            CheckState::Failed(crate::tui::tools_screen::Failure {
                reason: "not logged in".into(),
                unchanged: "x".into(),
                fix: None,
            }),
        );
        s.overlay = Some(crate::tui::state::Overlay::Failure("claude"));
        step(&mut s, Action::Skip);
        assert!(matches!(
            s.tools.checks.get("claude"),
            Some(CheckState::Failed(_))
        ));
        assert_eq!(
            s.overlay,
            Some(crate::tui::state::Overlay::Failure("claude")),
            "the box stays"
        );
        // A stored skip does not open Review for a required tool.
        s.tools.checks.insert("claude".into(), CheckState::Skipped);
        assert!(pending_names(&s).contains(&"Claude Code"));
    }

    #[test]
    fn a_failed_check_never_changes_the_configured_choice() {
        let mut s = settled();
        let before = (
            s.pipeline.form.clone(),
            s.tools.assistant,
            s.dirty,
            s.applied,
        );
        s.screen = Screen::Verify;
        s.tools.checks.insert("codex".into(), CheckState::Running);
        s.apply(Msg::Job(job("codex"), ended(Some(1), &["Not logged in"])));
        assert_eq!(
            (
                s.pipeline.form.clone(),
                s.tools.assistant,
                s.dirty,
                s.applied
            ),
            before
        );
        assert_eq!(
            ids(&s),
            ["herdr", "claude", "codex"],
            "codex is still the reviewer"
        );
        assert!(!verify_ready(&s));
    }

    #[test]
    fn nothing_locks_before_the_first_scan() {
        let mut s = State::new((100, 30));
        assert!(verify_ready(&s));
        s.screen = Screen::Verify;
        step(&mut s, Action::Goto(Screen::Review));
        assert_eq!(s.screen, Screen::Review);
    }

    #[test]
    fn a_required_tool_with_no_live_check_counts_once_it_is_installed() {
        let mut s = app(
            snap(&[
                ("claude", true, Login::LoggedIn),
                ("gemini", true, Login::Unknown),
            ]),
            None,
        );
        s.tools.assistant = AssistantChoice::CliOnly;
        let f = s.pipeline.form.as_mut().unwrap();
        let at = f
            .reviewer
            .items
            .iter()
            .position(|i| i.label == "gemini")
            .unwrap();
        f.reviewer.choose(at);
        s.tools
            .checks
            .insert("herdr".into(), CheckState::Passed { ms: 1 });
        s.tools
            .checks
            .insert("claude".into(), CheckState::Passed { ms: 1 });
        assert!(verify_ready(&s), "{:?}", pending_names(&s));
    }
}
