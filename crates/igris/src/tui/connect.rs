//! The connect step of the Tools screen: how a tool is logged in or given a key, and its live
//! check. A key goes to the stdin of the tool's own store-the-key command and nowhere else; a check
//! is a fixed command run as a background job, judged by its exit code.
use super::action::Action;
use super::detect_worker::Msg;
use super::input::{Input, InputEvent};
use super::state::{Effect, Overlay, State};
use super::tools_screen::{CheckState, Failure, login_read};
use crate::activity::Activity;
use crate::catalog::{MethodKind, PING, TOOLS, Tool, tool};
use crate::clean::strip_escapes;
use crate::job::{JobEnd, JobId, JobSet};
use crate::secret;
use igris_detect::Snapshot;
use ratatui::crossterm::event::KeyEvent;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

/// How long a live check may take before it is killed.
pub const CHECK_TIMEOUT: Duration = Duration::from_secs(90);
/// The same for a tool backed by a local model, which has to load first.
pub const CHECK_TIMEOUT_LOCAL: Duration = Duration::from_secs(300);

/// Hermes providers that run a model on this machine. `custom` is an endpoint the user typed in,
/// usually a local server; waiting longer for it costs nothing when it is not.
const LOCAL_PROVIDERS: [&str; 4] = ["ollama", "lmstudio", "llama-cpp", "custom"];

/// What a failure says was left alone: a check only reads, so this is always true.
const UNCHANGED: &str = "no setting was changed.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Methods,
    Hint,
    Entry,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Connect {
    pub tool: &'static str,
    pub step: Step,
    /// The focused method row.
    pub cursor: usize,
}

/// What choosing a method does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    /// Run the tool's live check.
    Check,
    /// Run the tool's own login command on the terminal, then scan again.
    Attach(&'static [&'static str]),
    /// Show the env var and the line to add to the shell profile, then offer a re-check.
    Env(&'static str),
    /// Masked entry: the key goes to the stdin of this command (the tool has a store-the-key command).
    Store(&'static [&'static str]),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MethodRow {
    pub label: &'static str,
    pub hint: &'static str,
    pub act: Act,
}

/// The ways to connect a tool: a login the scan read first, then the catalog's methods. A tool with
/// a live check but nothing detected gets a last row that runs the check.
pub fn methods_for(t: &Tool, snap: &Snapshot) -> Vec<MethodRow> {
    let mut rows: Vec<MethodRow> = t
        .methods
        .iter()
        .filter_map(|m| {
            let act = match m.kind {
                MethodKind::Existing => login_read(t, snap).then_some(Act::Check)?,
                MethodKind::Login | MethodKind::SetupToken => Act::Attach(m.login?),
                MethodKind::Key => match (m.key_stdin, m.env) {
                    (Some(argv), _) => Act::Store(argv),
                    (None, Some(name)) => Act::Env(name),
                    (None, None) => return None,
                },
                // A server found by a port probe: there is nothing to connect.
                MethodKind::Local => return None,
            };
            Some(MethodRow {
                label: m.label,
                hint: m.hint,
                act,
            })
        })
        .collect();
    if t.check.is_some() && !rows.iter().any(|r| r.act == Act::Check) {
        rows.push(MethodRow {
            label: "Check now",
            hint: "runs the tool's live check",
            act: Act::Check,
        });
    }
    rows
}

fn is_local(t: &Tool, snap: &Snapshot) -> bool {
    t.id == "hermes"
        && snap
            .hermes
            .provider
            .as_deref()
            .is_some_and(|p| LOCAL_PROVIDERS.contains(&p.to_lowercase().as_str()))
}

pub fn timeout_for(t: &Tool, snap: &Snapshot) -> Duration {
    if is_local(t, snap) {
        CHECK_TIMEOUT_LOCAL
    } else {
        CHECK_TIMEOUT
    }
}

/// The status word of a tool whose check is running.
pub fn running_word(t: &Tool, snap: &Snapshot) -> &'static str {
    if is_local(t, snap) {
        "loading model…"
    } else {
        "checking…"
    }
}

/// Text from a tool, safe to draw: no escape sequence, no control character, one line.
fn clean(text: &str) -> String {
    let one: String = strip_escapes(text)
        .chars()
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .collect();
    one.trim().to_string()
}

fn reason_of(end: &JobEnd, ms: u64) -> String {
    let last = || {
        end.tail
            .iter()
            .rev()
            .map(|l| clean(l))
            .find(|l| !l.is_empty())
    };
    if end.timed_out {
        return format!("no answer after {} s", (ms + 500) / 1000);
    }
    if !end.started {
        return last().unwrap_or_else(|| "could not start".into());
    }
    let all = clean(&end.tail.join(" ")).to_lowercase();
    if all.contains("not logged in") || all.contains("please run /login") {
        return "not logged in".into();
    }
    match (last(), end.code) {
        (Some(line), _) => line.chars().take(100).collect(),
        (None, Some(code)) => format!("it exited with code {code}"),
        (None, None) => "it ended without a result".into(),
    }
}

/// Map how a check ended. A command that was killed without a timeout is a cancel, not a failure.
pub fn check_outcome(t: &Tool, end: &JobEnd, ms: u64) -> CheckState {
    if end.code == Some(0) {
        return CheckState::Passed { ms };
    }
    if end.started && !end.timed_out && end.code.is_none() {
        return CheckState::Pending;
    }
    let fix = end
        .started
        .then(|| t.methods.iter().find_map(|m| m.login))
        .flatten()
        .map(|argv| argv.join(" "));
    CheckState::Failed(Failure {
        reason: reason_of(end, ms),
        unchanged: UNCHANGED.into(),
        fix,
    })
}

/// The failure box: why, what was left alone, the command that fixes it (when there is one).
pub fn failure_lines(t: &Tool, f: &Failure) -> Vec<String> {
    let mut v = vec![
        format!("✗ {}: {}.", t.id, clean(&f.reason).trim_end_matches('.')),
        format!("Unchanged: {}", clean(&f.unchanged)),
    ];
    if let Some(fix) = &f.fix {
        v.push(format!("Fix: {}", clean(fix)));
    }
    v
}

/// `✓ Claude Code replied in 6.8 s`; a tool whose check does not ask a model "answered".
pub fn passed_line(t: &Tool, ms: u64) -> String {
    let asks_a_model = t.check.is_some_and(|c| c.contains(&PING));
    format!(
        "✓ {} {} in {:.1} s",
        t.name,
        if asks_a_model { "replied" } else { "answered" },
        ms as f64 / 1000.0
    )
}

fn rows_of(s: &State, t: &Tool) -> Vec<MethodRow> {
    s.snapshot
        .as_ref()
        .map(|snap| methods_for(t, snap))
        .unwrap_or_default()
}

/// Rows of the method list that a click can pick.
pub fn clickable_rows(s: &State, c: &Connect) -> usize {
    match (c.step, tool(c.tool)) {
        (Step::Methods, Some(t)) => rows_of(s, t).len(),
        _ => 0,
    }
}

/// The title and lines of the connect overlay.
pub fn overlay_text(s: &State, c: &Connect, ascii: bool) -> (String, Vec<String>) {
    let Some(t) = tool(c.tool) else {
        return ("Connect".into(), vec![]);
    };
    let rows = rows_of(s, t);
    let title = format!("Connect {}", t.name);
    let row = rows.get(c.cursor);
    let lines = match (c.step, row.map(|r| r.act)) {
        (Step::Hint, Some(Act::Env(name))) => {
            let line = if cfg!(windows) {
                format!("  setx {name} \"<your key>\"")
            } else {
                format!("  export {name}=<your key>")
            };
            vec![
                format!("{} reads its key from {name}.", t.name),
                "Add this line to your shell profile, open a new terminal and start igris:".into(),
                line,
                String::new(),
                "In that terminal press r to check. Esc goes back.".into(),
            ]
        }
        (Step::Entry, Some(Act::Store(argv))) => {
            let bullet = if ascii { '*' } else { '•' };
            vec![
                format!(
                    "Paste your key. It goes to `{}` and nowhere else.",
                    argv.join(" ")
                ),
                format!("Key: {}_", s.tools.key.shown(bullet)),
                String::new(),
                "Enter stores it. Esc goes back.".into(),
            ]
        }
        _ => {
            let mut v = vec![format!("How should {} connect?", t.name), String::new()];
            v.extend(rows.iter().enumerate().map(|(i, r)| {
                format!(
                    "{} {}. {} · {}",
                    if i == c.cursor { ">" } else { " " },
                    i + 1,
                    r.label,
                    r.hint
                )
            }));
            if rows.is_empty() {
                v.push("  Nothing to connect.".into());
            }
            v.push(String::new());
            v.push("Enter chooses. Esc closes.".into());
            v
        }
    };
    (title, lines)
}

/// The title and lines of the failure box of `id`.
pub fn failure_text(s: &State, id: &'static str, ascii: bool) -> (String, Vec<String>) {
    let title = "Check failed".to_string();
    let (Some(t), Some(CheckState::Failed(f))) = (tool(id), s.tools.checks.get(id)) else {
        return (title, vec!["Nothing failed.".into()]);
    };
    let mut v: Vec<String> = failure_lines(t, f)
        .into_iter()
        .map(|l| if ascii { l.replace('✗', "x") } else { l })
        .collect();
    v.push(String::new());
    v.push("r checks again, s skips this tool, Esc goes back.".into());
    (title, v)
}

/// Open the connect overlay of `tool` on its method list.
pub fn open(s: &mut State, tool: &'static str) {
    s.tools.key = Input::masked();
    s.overlay = Some(Overlay::Connect(Connect {
        tool,
        step: Step::Methods,
        cursor: 0,
    }));
}

/// Start the live check of `id`. A check that is already running is not started twice.
pub fn begin_check(s: &mut State, id: &'static str) -> Effect {
    s.overlay = None;
    if tool(id).and_then(|t| t.check).is_none() {
        s.toast = Some("This tool has no live check.".into());
        return Effect::None;
    }
    if s.tools.checks.get(id) == Some(&CheckState::Running) {
        return Effect::None;
    }
    s.tools.checks.insert(id.to_string(), CheckState::Running);
    s.tools.started.insert(id, Instant::now());
    Effect::Check(id)
}

/// Keys and clicks while the connect overlay or the failure box is open.
pub fn update(s: &mut State, a: Action) -> Effect {
    match s.overlay.clone() {
        Some(Overlay::Failure(id)) => match a {
            Action::Rescan => begin_check(s, id),
            Action::Skip => {
                s.tools.checks.insert(id.to_string(), CheckState::Skipped);
                s.overlay = None;
                Effect::None
            }
            Action::Cancel | Action::No => {
                open(s, id);
                Effect::None
            }
            _ => Effect::None,
        },
        Some(Overlay::Connect(c)) => connect_key(s, c, a),
        _ => Effect::None,
    }
}

fn connect_key(s: &mut State, mut c: Connect, a: Action) -> Effect {
    let Some(t) = tool(c.tool) else {
        s.overlay = None;
        return Effect::None;
    };
    let rows = rows_of(s, t);
    let last = rows.len().saturating_sub(1);
    match (c.step, a) {
        (Step::Methods, Action::Up) => c.cursor = c.cursor.saturating_sub(1),
        (Step::Methods, Action::Down) => c.cursor = (c.cursor + 1).min(last),
        (Step::Methods, Action::Home) => c.cursor = 0,
        (Step::Methods, Action::End) => c.cursor = last,
        (Step::Methods, Action::Activate) => {
            let at = c.cursor;
            return pick(s, c, at, &rows);
        }
        (Step::Methods, Action::Select(i)) => return pick(s, c, i, &rows),
        (Step::Methods, Action::Cancel | Action::No) => {
            s.overlay = None;
            return Effect::None;
        }
        (Step::Hint, Action::Rescan) => return begin_check(s, c.tool),
        (Step::Hint | Step::Entry, Action::Cancel | Action::No) => {
            s.tools.key = Input::masked(); // going back empties the key
            c.step = Step::Methods;
        }
        (Step::Entry, Action::Activate) => {
            let Some(Act::Store(argv)) = rows.get(c.cursor).map(|r| r.act) else {
                return Effect::None;
            };
            if s.tools.key.value().is_empty() {
                s.toast = Some("Paste the key first, or press Esc.".into());
                return Effect::None;
            }
            s.tools.storing = Some(c.tool);
            s.overlay = None;
            return Effect::StoreKey(argv);
        }
        _ => return Effect::None,
    }
    s.overlay = Some(Overlay::Connect(c));
    Effect::None
}

fn pick(s: &mut State, mut c: Connect, i: usize, rows: &[MethodRow]) -> Effect {
    let Some(row) = rows.get(i) else {
        return Effect::None;
    };
    c.cursor = i;
    match row.act {
        Act::Check => return begin_check(s, c.tool),
        Act::Attach(argv) => {
            // The login may fix what the last check found; the next scan decides, not the old result.
            s.tools.checks.remove(c.tool);
            s.overlay = None;
            return Effect::RunAttached(argv);
        }
        Act::Env(_) => c.step = Step::Hint,
        Act::Store(_) => {
            s.tools.key = Input::masked();
            c.step = Step::Entry;
        }
    }
    s.overlay = Some(Overlay::Connect(c));
    Effect::None
}

/// A background check ended: record it, say so, and open the failure box when it failed.
pub fn on_job(s: &mut State, id: JobId, end: JobEnd) {
    // Ids from 1000 up are installs (a later step); a check's id is its place in the catalog.
    let Some(t) = TOOLS.get(id.0 as usize) else {
        return;
    };
    let ms = s
        .tools
        .started
        .remove(t.id)
        .map_or(0, |at| at.elapsed().as_millis() as u64);
    let outcome = check_outcome(t, &end, ms);
    match &outcome {
        CheckState::Passed { ms } => s.toast = Some(passed_line(t, *ms)),
        CheckState::Failed(f) => {
            if s.overlay.is_none() {
                s.overlay = Some(Overlay::Failure(t.id));
            } else {
                s.toast = failure_lines(t, f).into_iter().next();
            }
        }
        CheckState::Pending => s.toast = Some("Cancelled".into()),
        CheckState::Running | CheckState::Skipped => {}
    }
    if outcome == CheckState::Pending {
        s.tools.checks.remove(t.id);
    } else {
        s.tools.checks.insert(t.id.to_string(), outcome);
    }
}

/// The key entry step is open: typed letters are text.
pub fn entering(s: &State) -> bool {
    matches!(&s.overlay, Some(Overlay::Connect(c)) if c.step == Step::Entry)
}

/// A key typed in the entry step. Enter and Esc become actions.
pub fn entry_key(s: &mut State, key: KeyEvent) -> Option<Action> {
    match s.tools.key.handle(key) {
        InputEvent::Commit => Some(Action::Activate),
        InputEvent::Cancel => Some(Action::Cancel),
        InputEvent::Changed | InputEvent::None => None,
    }
}

pub fn paste(s: &mut State, text: &str) {
    if entering(s) {
        // A copied key often carries a line break or a space at the end; it is not part of the key.
        s.tools.key.paste(text.trim());
    }
}

/// Hand the typed key to the stdin of `argv` (the loop calls this for `Effect::StoreKey`). The input
/// is emptied first, so no path leaves the key in the state. Returns the tool to check when the
/// store command succeeded.
pub fn store_key(
    s: &mut State,
    argv: &'static [&'static str],
    log: &Mutex<Activity>,
) -> Option<&'static str> {
    let key = s.tools.key.take_secret();
    let id = s.tools.storing.take()?;
    let name = tool(id).map_or(id, |t| t.name);
    let out = {
        let mut log = log.lock().unwrap_or_else(PoisonError::into_inner);
        secret::run_with_stdin(argv, key, &mut log)
    };
    if out.code == Some(0) {
        s.toast = Some(format!("{name}: key stored. Checking."));
        return Some(id);
    }
    let code = out
        .code
        .map_or_else(|| "no exit code".to_string(), |c| format!("exit {c}"));
    let why = out
        .tail
        .iter()
        .rev()
        .map(|l| clean(l))
        .find(|l| !l.is_empty())
        .map(|l| format!(": {l}"))
        .unwrap_or_default();
    s.toast = Some(format!("{name} did not store the key ({code}){why}"));
    None
}

/// Start the live check of `id` as a background job (the loop calls this for `Effect::Check`).
pub fn spawn_check(
    jobs: &mut JobSet,
    tx: &Sender<Msg>,
    log: &Arc<Mutex<Activity>>,
    s: &State,
    id: &'static str,
) {
    let (Some(t), Some(at)) = (tool(id), TOOLS.iter().position(|t| t.id == id)) else {
        return;
    };
    let Some(argv) = t.check else {
        return;
    };
    let timeout = s
        .snapshot
        .as_ref()
        .map_or(CHECK_TIMEOUT, |snap| timeout_for(t, snap));
    jobs.spawn(tx.clone(), JobId(at as u32), argv, timeout, log.clone());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{TOOLS, tool};
    use crate::tui::detect_worker::Msg;
    use crate::tui::state::{Overlay, update as step};
    use crate::tui::tools_screen::tests::{app, both, snap};
    use igris_detect::Login;
    use ratatui::crossterm::event::{KeyCode, KeyModifiers};

    fn t(id: &str) -> &'static Tool {
        tool(id).unwrap()
    }

    fn job(id: &str) -> JobId {
        JobId(TOOLS.iter().position(|t| t.id == id).unwrap() as u32)
    }

    fn ended(code: Option<i32>, lines: &[&str]) -> JobEnd {
        JobEnd {
            code,
            tail: lines.iter().map(|l| (*l).to_string()).collect(),
            timed_out: false,
            started: true,
        }
    }

    fn key(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    /// Move to the method row named `label` and press Enter on it.
    fn pick(s: &mut State, label: &str) -> Effect {
        let Some(Overlay::Connect(c)) = s.overlay.clone() else {
            panic!("no connect overlay: {:?}", s.overlay)
        };
        let rows = methods_for(t(c.tool), s.snapshot.as_ref().unwrap());
        let at = rows
            .iter()
            .position(|r| r.label == label)
            .unwrap_or_else(|| panic!("no row {label}: {rows:?}"));
        step(s, Action::Home);
        for _ in 0..at {
            step(s, Action::Down);
        }
        step(s, Action::Activate)
    }

    fn builder(s: &State) -> Option<String> {
        s.pipeline
            .form
            .as_ref()
            .and_then(|f| f.builder.current().map(|i| i.label.clone()))
    }

    fn failed(s: &mut State, id: &'static str, lines: &[&str]) {
        s.tools.checks.insert(id.into(), CheckState::Running);
        s.apply(Msg::Job(job(id), ended(Some(1), lines)));
    }

    #[test]
    fn an_existing_login_is_the_first_method_when_detected() {
        let logged_in = both();
        let rows = methods_for(t("claude"), &logged_in);
        assert_eq!(rows[0].label, "Existing login");
        assert_eq!(rows[0].act, Act::Check);
        let logged_out = snap(&[("claude", true, Login::LoggedOut)]);
        let rows = methods_for(t("claude"), &logged_out);
        assert_eq!(rows[0].label, "Log in");
        assert!(rows.iter().all(|r| r.label != "Existing login"));
    }

    #[test]
    fn log_in_runs_the_tools_own_login_attached_then_rescans() {
        let mut s = app(snap(&[("claude", true, Login::LoggedOut)]), None);
        step(&mut s, Action::NextField); // the Builder group, Claude Code first
        assert_eq!(step(&mut s, Action::Activate), Effect::None);
        assert!(matches!(&s.overlay, Some(Overlay::Connect(c)) if c.tool == "claude"));
        assert_eq!(
            pick(&mut s, "Log in"),
            Effect::RunAttached(&["claude", "auth", "login"])
        );
        assert_eq!(
            s.overlay, None,
            "the loop rescans after an attached command"
        );
    }

    #[test]
    fn a_key_method_without_key_stdin_shows_the_env_var_and_a_recheck() {
        let mut s = app(both(), None);
        open(&mut s, "claude");
        assert_eq!(pick(&mut s, "API key"), Effect::None);
        let Some(Overlay::Connect(c)) = s.overlay.clone() else {
            panic!()
        };
        assert_eq!(c.step, Step::Hint);
        let (_, lines) = overlay_text(&s, &c, false);
        let text = lines.join("\n");
        assert!(text.contains("ANTHROPIC_API_KEY"), "{text}");
        assert!(text.contains("press r"), "{text}");
        assert!(
            !entering(&s),
            "no masked entry for a tool with no key command"
        );
        assert_eq!(step(&mut s, Action::Rescan), Effect::Check("claude"));
        assert_eq!(s.tools.checks.get("claude"), Some(&CheckState::Running));
    }

    #[test]
    fn masked_entry_is_offered_only_when_the_tool_has_key_stdin() {
        let key_row = |id: &str| {
            *methods_for(t(id), &both())
                .iter()
                .find(|r| r.label == "API key")
                .unwrap()
        };
        assert_eq!(
            key_row("codex").act,
            Act::Store(&["codex", "login", "--with-api-key"])
        );
        assert_eq!(key_row("claude").act, Act::Env("ANTHROPIC_API_KEY"));
    }

    #[test]
    fn the_entry_step_takes_typed_and_pasted_text_and_esc_empties_it() {
        let mut s = app(both(), None);
        open(&mut s, "codex");
        pick(&mut s, "API key");
        assert!(entering(&s));
        paste(&mut s, "sk-pasted\n");
        entry_key(&mut s, key('x'));
        assert_eq!(
            s.tools.key.value(),
            "sk-pastedx",
            "a pasted line break is dropped"
        );
        assert_eq!(
            entry_key(&mut s, KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            Some(Action::Cancel)
        );
        step(&mut s, Action::Cancel);
        assert!(s.tools.key.value().is_empty(), "going back empties the key");
        assert!(matches!(&s.overlay, Some(Overlay::Connect(c)) if c.step == Step::Methods));
        step(&mut s, Action::Cancel);
        assert_eq!(s.overlay, None, "Esc in the list closes it");
    }

    #[test]
    fn enter_in_the_entry_step_sends_the_key_to_the_store_command() {
        let mut s = app(both(), None);
        open(&mut s, "codex");
        pick(&mut s, "API key");
        assert_eq!(
            step(&mut s, Action::Activate),
            Effect::None,
            "empty: nothing sent"
        );
        assert!(s.toast.is_some());
        entry_key(&mut s, key('k'));
        assert_eq!(
            entry_key(&mut s, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            Some(Action::Activate)
        );
        assert_eq!(
            step(&mut s, Action::Activate),
            Effect::StoreKey(&["codex", "login", "--with-api-key"])
        );
        assert_eq!((s.overlay.clone(), s.tools.storing), (None, Some("codex")));
    }

    #[cfg(unix)]
    #[test]
    fn the_key_goes_to_the_stores_stdin_and_the_input_is_emptied() {
        static OK: &[&str] = &[
            "sh",
            "-c",
            "read k && [ \"$k\" = \"sk-test-$((120 + 3))\" ]",
        ];
        static BAD: &[&str] = &["sh", "-c", "cat > /dev/null; echo bad key >&2; exit 3"];
        let mut s = app(both(), None);
        let log = Mutex::new(Activity::default());
        for c in "sk-test-123".chars() {
            s.tools.key.insert(c);
        }
        s.tools.storing = Some("codex");
        assert_eq!(store_key(&mut s, OK, &log), Some("codex"));
        assert!(s.tools.key.value().is_empty(), "the input is emptied");
        let seen = format!("{:?} {:?}", log.lock().unwrap(), s.toast);
        assert!(!seen.contains("sk-test-123"), "{seen}");
        assert_eq!(log.lock().unwrap().entries().len(), 1);
        for c in "sk-test-123".chars() {
            s.tools.key.insert(c);
        }
        s.tools.storing = Some("codex");
        assert_eq!(store_key(&mut s, BAD, &log), None);
        let toast = s.toast.clone().unwrap();
        assert!(toast.contains("Codex") && toast.contains("exit 3") && toast.contains("bad key"));
        assert!(s.tools.key.value().is_empty());
    }

    #[test]
    fn a_failed_check_shows_the_three_line_box_and_changes_nothing() {
        let mut s = app(both(), None);
        let before = (s.pipeline.clone(), s.dirty, s.applied);
        open(&mut s, "claude");
        assert_eq!(pick(&mut s, "Existing login"), Effect::Check("claude"));
        s.apply(Msg::Job(
            job("claude"),
            ended(Some(1), &["Not logged in · Please run /login"]),
        ));
        assert_eq!(s.overlay, Some(Overlay::Failure("claude")));
        let Some(CheckState::Failed(f)) = s.tools.checks.get("claude").cloned() else {
            panic!("{:?}", s.tools.checks)
        };
        assert_eq!(
            failure_lines(t("claude"), &f),
            [
                "✗ claude: not logged in.",
                "Unchanged: no setting was changed.",
                "Fix: claude auth login"
            ]
        );
        let (_, text) = failure_text(&s, "claude", false);
        let text = text.join("\n");
        assert!(
            text.contains("r ") && text.contains("s ") && text.contains("Esc"),
            "{text}"
        );
        assert_eq!((s.pipeline.clone(), s.dirty, s.applied), before);

        // r checks again, s skips, Esc goes back to the methods.
        assert_eq!(step(&mut s, Action::Rescan), Effect::Check("claude"));
        assert_eq!(
            (s.overlay.clone(), s.tools.checks.get("claude")),
            (None, Some(&CheckState::Running))
        );
        s.apply(Msg::Job(job("claude"), ended(Some(1), &["Not logged in"])));
        step(&mut s, Action::Skip);
        assert_eq!(
            (s.overlay.clone(), s.tools.checks.get("claude")),
            (None, Some(&CheckState::Skipped))
        );
        failed(&mut s, "claude", &["boom"]);
        step(&mut s, Action::Cancel);
        assert!(
            matches!(&s.overlay, Some(Overlay::Connect(c)) if c.tool == "claude" && c.step == Step::Methods)
        );
    }

    #[test]
    fn a_login_clears_the_old_failure_so_the_next_scan_decides() {
        let mut s = app(snap(&[("claude", true, Login::LoggedOut)]), None);
        failed(&mut s, "claude", &["Not logged in"]);
        step(&mut s, Action::Cancel);
        assert_eq!(
            pick(&mut s, "Log in"),
            Effect::RunAttached(&["claude", "auth", "login"])
        );
        s.apply(Msg::Detected(Box::new(both())));
        let status = crate::tui::tools_screen::status_of(
            t("claude"),
            s.snapshot.as_ref().unwrap(),
            &s.tools,
        );
        assert_eq!(status, crate::tui::tools_screen::ToolStatus::Ready);
        assert!(!s.tools.checks.contains_key("claude"));
    }

    #[test]
    fn a_check_job_comes_back_by_its_catalog_index() {
        // Herdr is on a developer machine and not on CI: either way the job must come back as
        // this tool's own result (no model is called).
        let (tx, rx) = std::sync::mpsc::channel();
        let (mut jobs, log) = (JobSet::default(), Arc::new(Mutex::new(Activity::default())));
        let mut s = app(both(), None);
        s.tools.checks.insert("herdr".into(), CheckState::Running);
        spawn_check(&mut jobs, &tx, &log, &s, "herdr");
        s.apply(rx.recv_timeout(Duration::from_secs(60)).unwrap());
        let got = s.tools.checks.get("herdr");
        assert!(
            matches!(got, Some(CheckState::Passed { .. } | CheckState::Failed(_))),
            "{got:?}"
        );
        assert_eq!(s.tools.checks.len(), 1);
    }

    #[test]
    fn a_cancel_reads_cancelled() {
        let mut s = app(both(), None);
        s.tools.checks.insert("claude".into(), CheckState::Running);
        s.apply(Msg::Job(job("claude"), ended(None, &[])));
        assert_eq!(s.toast.as_deref(), Some("Cancelled"));
        assert_eq!(s.overlay, None, "a cancel opens no box");
        assert_eq!(s.tools.checks.get("claude"), None);
        assert_eq!(
            check_outcome(t("claude"), &ended(None, &[]), 5),
            CheckState::Pending
        );
    }

    #[test]
    fn a_passing_check_shows_latency() {
        assert_eq!(
            passed_line(t("claude"), 6800),
            "✓ Claude Code replied in 6.8 s"
        );
        assert_eq!(passed_line(t("herdr"), 100), "✓ Herdr answered in 0.1 s");
        let mut s = app(both(), None);
        s.tools.checks.insert("claude".into(), CheckState::Running);
        s.apply(Msg::Job(job("claude"), ended(Some(0), &["ok"])));
        assert!(
            s.toast
                .as_deref()
                .unwrap()
                .starts_with("✓ Claude Code replied in ")
        );
        assert!(matches!(
            s.tools.checks.get("claude"),
            Some(CheckState::Passed { .. })
        ));
        assert_eq!(s.overlay, None);
    }

    #[test]
    fn a_local_model_tool_uses_the_long_timeout_and_shows_loading_model() {
        let mut local = both();
        local.hermes.found = true;
        local.hermes.provider = Some("ollama".into());
        assert_eq!(timeout_for(t("hermes"), &local), CHECK_TIMEOUT_LOCAL);
        assert_eq!(running_word(t("hermes"), &local), "loading model…");
        assert_eq!(timeout_for(t("claude"), &local), CHECK_TIMEOUT);
        assert_eq!(running_word(t("claude"), &local), "checking…");
        let mut hosted = both();
        hosted.hermes.provider = Some("openrouter".into());
        assert_eq!(timeout_for(t("hermes"), &hosted), CHECK_TIMEOUT);
    }

    #[test]
    fn a_timeout_and_a_missing_program_say_so() {
        let slow = JobEnd {
            timed_out: true,
            code: None,
            ..ended(None, &[])
        };
        let CheckState::Failed(f) = check_outcome(t("claude"), &slow, 90_000) else {
            panic!()
        };
        assert_eq!(f.reason, "no answer after 90 s");
        let gone = JobEnd {
            started: false,
            code: None,
            ..ended(None, &["could not start claude: not found"])
        };
        let CheckState::Failed(f) = check_outcome(t("claude"), &gone, 0) else {
            panic!()
        };
        assert_eq!(
            (f.reason.as_str(), f.fix),
            ("could not start claude: not found", None)
        );
    }

    #[test]
    fn escape_sequences_in_check_output_never_reach_the_screen() {
        let evil = "\x1b[31mboom\x1b]52;c;aGk=\x07\x1b[0m done";
        let mut s = app(both(), None);
        failed(&mut s, "claude", &[evil]);
        let Some(CheckState::Failed(f)) = s.tools.checks.get("claude").cloned() else {
            panic!()
        };
        let (_, shown) = failure_text(&s, "claude", false);
        for line in failure_lines(t("claude"), &f).iter().chain(&shown) {
            assert!(!line.chars().any(char::is_control), "{line:?}");
        }
        assert!(f.reason.contains("boom"));
    }

    #[test]
    fn a_failed_check_never_picks_another_tool() {
        let mut s = app(both(), None);
        let (builder_before, assistant) = (builder(&s), s.tools.assistant);
        failed(&mut s, "codex", &["Not logged in"]);
        assert_eq!(builder(&s), builder_before);
        assert_eq!(s.tools.assistant, assistant);
        assert_eq!(
            s.tools.checks.len(),
            1,
            "only the tool that failed has a result"
        );
        failed(&mut s, "claude", &["Not logged in"]);
        assert_eq!(
            builder(&s),
            builder_before,
            "a failed builder is not swapped for the other agent"
        );
    }
}
