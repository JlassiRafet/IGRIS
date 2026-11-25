//! Installs and model downloads of the Tools step. Both run as background jobs after a yes, from
//! fixed commands: a tool's install argv from the catalog, a model's pull argv from the fit
//! module's own table. Nothing here builds a command from scanned or llmfit data.
use super::detect_worker::Msg;
use super::state::{Confirm, Effect, Overlay, State};
use super::tools_screen::{Failure, Install, Pull, Row, ToolStatus, ToolsUi, installed};
use crate::activity::Activity;
use crate::catalog::{TOOLS, Tool, tool};
use crate::job::{JobEnd, JobId, JobSet};
use igris_detect::Snapshot;
use igris_detect::fit::{CANDIDATES, Fit, Report, Source, Suggestion};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Job ids: a check is its catalog index, an install is `INSTALL_BASE` plus it, a model pull is
/// `PULL_BASE` plus the candidate's place in the fit table.
pub const INSTALL_BASE: u32 = 1000;
pub const PULL_BASE: u32 = 2000;

/// An installer that has not finished in ten minutes is stuck (a download that hangs).
pub const INSTALL_TIMEOUT: Duration = Duration::from_secs(600);
/// A model is several gigabytes: an hour.
pub const PULL_TIMEOUT: Duration = Duration::from_secs(3600);

/// What the app does with a tool's install command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Offer {
    /// Run it in the background, after a yes.
    Run(&'static [&'static str]),
    /// It needs administrator rights: show it for the user to run (copied), never run it.
    Copy(String),
    /// No verified command.
    None,
}

pub fn install_offer(t: &Tool) -> Offer {
    match t.install {
        None => Offer::None,
        Some(argv) if t.install_needs_admin => Offer::Copy(quote_cmd(argv)),
        Some(argv) => Offer::Run(argv),
    }
}

/// The command as a shell would take it, so what the dialog shows is what runs: an argument with
/// a space or a shell character is put in single quotes.
pub fn quote_cmd(argv: &[&str]) -> String {
    let plain = |a: &str| {
        !a.is_empty()
            && a.chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_./:=@%+,".contains(c))
    };
    argv.iter()
        .map(|a| {
            if plain(a) {
                (*a).to_string()
            } else {
                format!("'{}'", a.replace('\'', "'\\''"))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Enter on a tool that is not on this machine: offer its install, or say where to get it.
pub fn offer(s: &mut State, t: &Tool) -> Effect {
    if matches!(
        s.tools.installs.get(t.id),
        Some(Install::Running(_) | Install::Done(_))
    ) {
        s.toast = Some(format!("{} is installing.", t.name));
        return Effect::None;
    }
    match install_offer(t) {
        Offer::Run(_) => s.overlay = Some(Overlay::Confirm(Confirm::InstallTool(t.id))),
        Offer::Copy(cmd) => {
            s.toast = Some(format!(
                "{} needs administrator rights: run this yourself, then press r.",
                t.name
            ));
            return Effect::Copy(cmd);
        }
        Offer::None if t.docs.is_empty() => {
            s.toast = Some(format!(
                "{} is not installed. Install it, then press r to scan again.",
                t.name
            ));
        }
        Offer::None => {
            s.toast = Some(format!(
                "{} is not installed. See {}, then press r to scan again.",
                t.name, t.docs
            ));
        }
    }
    Effect::None
}

/// The yes of the install dialog.
pub fn start_install(s: &mut State) -> Effect {
    let Some(Overlay::Confirm(Confirm::InstallTool(id))) = s.overlay.take() else {
        return Effect::None;
    };
    if tool(id).and_then(|t| t.install).is_none() {
        return Effect::None;
    }
    s.tools
        .installs
        .insert(id, Install::Running(Instant::now()));
    Effect::Install(id)
}

/// The yes of the pull dialog.
pub fn start_pull(s: &mut State) -> Effect {
    let Some(Overlay::Confirm(Confirm::Pull(i))) = s.overlay.take() else {
        return Effect::None;
    };
    let Some(c) = CANDIDATES.get(i) else {
        return Effect::None;
    };
    s.tools.pulls.insert(c.tag, Pull::Running(Instant::now()));
    Effect::Pull(i)
}

/// Run the install of `id` as a background job (the loop calls this for `Effect::Install`).
pub fn spawn_install(
    jobs: &mut JobSet,
    tx: &Sender<Msg>,
    log: &Arc<Mutex<Activity>>,
    id: &'static str,
) {
    let (Some(t), Some(at)) = (tool(id), TOOLS.iter().position(|t| t.id == id)) else {
        return;
    };
    if let Some(argv) = t.install {
        jobs.spawn(
            tx.clone(),
            JobId(INSTALL_BASE + at as u32),
            argv,
            INSTALL_TIMEOUT,
            log.clone(),
        );
    }
}

/// Run the pull of candidate `i` as a background job (the loop calls this for `Effect::Pull`).
pub fn spawn_pull(jobs: &mut JobSet, tx: &Sender<Msg>, log: &Arc<Mutex<Activity>>, i: usize) {
    if let Some(c) = CANDIDATES.get(i) {
        jobs.spawn(
            tx.clone(),
            JobId(PULL_BASE + i as u32),
            c.pull,
            PULL_TIMEOUT,
            log.clone(),
        );
    }
}

/// Route a finished job by its id range.
pub fn dispatch(s: &mut State, id: JobId, end: JobEnd) {
    match id.0 {
        n if n >= PULL_BASE => on_pull(s, (n - PULL_BASE) as usize, &end),
        n if n >= INSTALL_BASE => on_install(s, (n - INSTALL_BASE) as usize, &end),
        _ => super::connect::on_job(s, id, end),
    }
}

fn cancelled(end: &JobEnd) -> bool {
    end.started && !end.timed_out && end.code.is_none()
}

fn on_install(s: &mut State, at: usize, end: &JobEnd) {
    let Some(t) = TOOLS.get(at) else {
        return;
    };
    let ms = match s.tools.installs.get(t.id) {
        Some(Install::Running(since)) => since.elapsed().as_millis() as u64,
        _ => 0,
    };
    if end.code == Some(0) {
        s.tools
            .installs
            .insert(t.id, Install::Done(end.tail.clone()));
        s.toast = Some(format!("{} installed. Scanning again.", t.name));
        s.rescan_wanted = true;
    } else if cancelled(end) {
        s.tools.installs.remove(t.id);
        s.toast = Some("Cancelled".into());
    } else {
        let reason = if end.timed_out {
            format!("did not finish after {} s", (ms + 500) / 1000)
        } else {
            super::connect::reason_of(end, ms)
        };
        let failure = Failure {
            reason,
            unchanged: "IGRIS changed no setting; the installer may have left files.".into(),
            fix: t.install.map(quote_cmd),
        };
        s.tools.installs.insert(t.id, Install::Failed(failure));
        super::connect::show_failure(s, t.id);
    }
}

fn on_pull(s: &mut State, i: usize, end: &JobEnd) {
    let Some(c) = CANDIDATES.get(i) else {
        return;
    };
    if end.code == Some(0) {
        s.tools.pulls.remove(c.tag);
        s.toast = Some(format!("{} pulled. Scanning again.", c.tag));
        s.rescan_wanted = true;
    } else if cancelled(end) {
        s.tools.pulls.remove(c.tag);
        s.toast = Some("Cancelled".into());
    } else {
        let why = if end.timed_out {
            "did not finish in time".to_string()
        } else {
            super::connect::reason_of(end, 0)
        };
        s.toast = Some(format!(
            "✗ {}: {why}. Run it yourself: {}",
            c.tag,
            quote_cmd(c.pull)
        ));
        s.tools.pulls.insert(c.tag, Pull::Failed(why));
    }
}

/// After a scan: an install that finished is either there now (say how to connect it) or is not
/// on PATH yet (say so, with the installer's own words); a failed install of a tool the scan now
/// finds is stale and goes.
pub fn after_scan(s: &mut State) {
    let Some(snap) = s.snapshot.clone() else {
        return;
    };
    // A scan that began before the install ended cannot judge it: the scan wanted next will.
    let settled = !s.rescan_wanted;
    let ids: Vec<&'static str> = s.tools.installs.keys().copied().collect();
    for id in ids {
        let Some(t) = tool(id) else {
            continue;
        };
        let found = installed(t, &snap, &s.tools);
        match s.tools.installs.get(id).cloned() {
            Some(Install::Done(_)) if found && settled => {
                s.tools.installs.remove(id);
                s.toast = Some(if t.methods.is_empty() && t.check.is_none() {
                    format!("{} installed.", t.name)
                } else {
                    format!("{} installed. Press c to connect or check it.", t.name)
                });
            }
            Some(Install::Done(tail)) if settled => {
                let reason = tail
                    .iter()
                    .rev()
                    .find(|l| l.contains("PATH"))
                    .cloned()
                    .unwrap_or_else(|| "installed, but IGRIS cannot find it on PATH".into());
                let failure = Failure {
                    reason,
                    unchanged: "IGRIS changed no setting; the installer put the program in a folder PATH does not list."
                        .into(),
                    fix: None,
                };
                s.tools.installs.insert(id, Install::Failed(failure));
                super::connect::show_failure(s, id);
            }
            Some(Install::Failed(_)) if found => {
                s.tools.installs.remove(id);
                if s.overlay == Some(Overlay::Failure(id)) {
                    s.overlay = None;
                }
            }
            _ => {}
        }
    }
}

fn ollama_of(snap: &Snapshot) -> Option<&igris_detect::LocalLlm> {
    snap.local_llm
        .iter()
        .find(|l| l.name.eq_ignore_ascii_case("ollama"))
}

fn pulled(snap: &Snapshot, tag: &str) -> bool {
    ollama_of(snap).is_some_and(|l| l.models.iter().any(|m| m == tag))
}

/// Enter on a model row: confirm the pull, or say what is missing first.
pub fn pick_model(s: &mut State, i: usize) -> Effect {
    let (Some(c), Some(snap)) = (CANDIDATES.get(i), s.snapshot.as_ref()) else {
        return Effect::None;
    };
    let toast = if matches!(s.tools.pulls.get(c.tag), Some(Pull::Running(_))) {
        Some(format!("{} is downloading.", c.tag))
    } else if pulled(snap, c.tag) {
        Some(format!("{} is already pulled.", c.tag))
    } else if !ollama_of(snap).is_some_and(|l| l.up) {
        Some(format!(
            "Start Ollama first (ollama serve), then pick this again. The command: {}",
            quote_cmd(c.pull)
        ))
    } else {
        None
    };
    match toast {
        Some(t) => s.toast = Some(t),
        None => s.overlay = Some(Overlay::Confirm(Confirm::Pull(i))),
    }
    Effect::None
}

fn verdict(f: Fit) -> &'static str {
    match f {
        Fit::Perfect => "fits well",
        Fit::Good => "fits",
        Fit::Marginal => "fits tightly",
        Fit::TooBig => "too big for this machine",
        Fit::Unknown => "fit not checked",
    }
}

fn gb(n: f32) -> String {
    format!("{n:.1} GB")
}

fn llmfit_docs() -> &'static str {
    tool("llmfit").map_or("", |t| t.docs)
}

/// The lines under a model row: why it is listed, how it fits, the exact pull command.
fn fit_detail(report: &Report, sg: &Suggestion, pulled: bool, pull: Option<&Pull>) -> String {
    let c = sg.candidate;
    let mut sizes = vec![verdict(sg.fit).to_string()];
    match (sg.needs_gb, sg.available_gb) {
        (Some(n), Some(a)) => sizes.push(format!("needs {}, {} available", gb(n), gb(a))),
        (Some(n), None) => sizes.push(format!("needs {}", gb(n))),
        _ => {}
    }
    if let Some(tps) = sg.tps {
        sizes.push(format!("about {tps:.0} tok/s"));
    }
    let mut v = vec![c.why.to_string(), sizes.join(" · ")];
    v.push(if pulled {
        "Already pulled.".to_string()
    } else {
        format!("Pull: {} (Ollama must be running)", quote_cmd(c.pull))
    });
    if let Some(Pull::Failed(why)) = pull {
        v.push(format!("Last pull failed: {why}."));
    }
    if let Source::Fallback { reason } = &report.source {
        v.push(format!(
            "Fallback: {reason}, so this is IGRIS's own list and the fit is not checked. See {}",
            llmfit_docs()
        ));
    }
    v.join("\n")
}

/// The rows the fit report adds to the Assistant group.
pub fn model_rows(snap: &Snapshot, ui: &ToolsUi) -> Vec<Row> {
    let Some(report) = &ui.fit else {
        return vec![];
    };
    report
        .suggestions
        .iter()
        .filter_map(|sg| {
            let c = sg.candidate;
            let model = CANDIDATES.iter().position(|x| x.tag == c.tag)?;
            let is_pulled = pulled(snap, c.tag);
            // A model that is there is ready, whatever an earlier pull said (it may be pulled by hand).
            let pull = ui
                .pulls
                .get(c.tag)
                .filter(|p| matches!(p, Pull::Running(_)) || !is_pulled);
            let status = match pull {
                Some(Pull::Running(_)) => ToolStatus::Checking,
                Some(Pull::Failed(_)) => ToolStatus::NeedsStep,
                None if is_pulled => ToolStatus::Ready,
                None => ToolStatus::NeedsStep,
            };
            Some(Row {
                id: c.tag,
                name: c.hf_name,
                status,
                connects: "",
                default: false,
                chosen: false,
                pickable: true,
                detail: fit_detail(report, sg, is_pulled, pull),
                model: Some(model),
            })
        })
        .collect()
}

/// The one fit line under "Your machine" on the Scan screen.
pub fn fit_line(report: &Report) -> String {
    match &report.source {
        Source::Llmfit { version } => {
            let first = report.suggestions.first().map_or_else(
                || "no model listed".to_string(),
                |sg| {
                    let name = sg.candidate.hf_name.rsplit('/').next().unwrap_or("");
                    format!("{name} {}", verdict(sg.fit))
                },
            );
            let more = report.suggestions.len().saturating_sub(1);
            let more = if more > 0 {
                format!(", +{more} more")
            } else {
                String::new()
            };
            format!("{first}{more} (llmfit {version})")
        }
        Source::Fallback { reason } => format!(
            "fit not checked: {reason}. The fallback is IGRIS's own list (see {})",
            llmfit_docs()
        ),
    }
}

/// The title and lines of the install and pull dialogs.
pub fn dialog_text(c: &Confirm) -> (String, Vec<String>) {
    match c {
        Confirm::InstallTool(id) => {
            let Some(t) = tool(id) else {
                return ("Install".into(), vec![]);
            };
            let cmd = t.install.map(quote_cmd).unwrap_or_default();
            (
                format!("Install {}?", t.name),
                vec![
                    "This runs in the background; the app stays usable:".into(),
                    format!("  {cmd}"),
                    format!("Source: {}", t.docs),
                    "Quitting stops it. Run it? (y / n)".into(),
                ],
            )
        }
        Confirm::Pull(i) => {
            let Some(c) = CANDIDATES.get(*i) else {
                return ("Download".into(), vec![]);
            };
            (
                format!("Download {}?", c.tag),
                vec![
                    "This downloads a model with Ollama, in the background:".into(),
                    format!("  {}", quote_cmd(c.pull)),
                    "It is several gigabytes. Quitting stops it. Run it? (y / n)".into(),
                ],
            )
        }
        _ => (String::new(), vec![]),
    }
}

/// The running install or download, for the line above the key hints.
pub fn progress_line(s: &State, ascii: bool) -> Option<String> {
    let spin = if ascii {
        ["-", "\\", "|", "/"][s.tick % 4]
    } else {
        ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"][s.tick % 10]
    };
    let running = |since: &Instant| since.elapsed().as_secs();
    for (id, st) in &s.tools.installs {
        if let Install::Running(since) = st {
            let name = tool(id).map_or(*id, |t| t.name);
            return Some(format!("{spin} Installing {name}… {} s", running(since)));
        }
    }
    for (tag, st) in &s.tools.pulls {
        if let Pull::Running(since) = st {
            return Some(format!("{spin} Downloading {tag}… {} s", running(since)));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::action::Action;
    use crate::tui::state::update as step;
    use crate::tui::tools_screen::tests::{app, both};
    use crate::tui::tools_screen::{rows, status_of};
    use igris_detect::fit::Candidate;
    use igris_detect::{LocalLlm, Login};

    fn without_herdr() -> Snapshot {
        let mut s = both();
        s.herdr.found = false;
        s.herdr.ok = false;
        s
    }

    fn ended(code: Option<i32>, lines: &[&str]) -> JobEnd {
        JobEnd {
            code,
            tail: lines.iter().map(|l| (*l).to_string()).collect(),
            timed_out: false,
            started: true,
        }
    }

    fn herdr_job() -> JobId {
        JobId(INSTALL_BASE + TOOLS.iter().position(|t| t.id == "herdr").unwrap() as u32)
    }

    fn report(source: Source) -> Report {
        Report {
            source,
            suggestions: vec![Suggestion {
                candidate: &CANDIDATES[0],
                fit: Fit::Marginal,
                needs_gb: Some(4.36),
                available_gb: Some(4.5),
                tps: Some(60.2),
            }],
        }
    }

    fn llmfit() -> Source {
        Source::Llmfit {
            version: "1.1.21".into(),
        }
    }

    fn ollama(up: bool, models: &[&str]) -> Vec<LocalLlm> {
        vec![LocalLlm {
            name: "ollama".into(),
            port: 11434,
            up,
            models: models.iter().map(|m| (*m).to_string()).collect(),
        }]
    }

    /// The app on the Tools screen with Herdr missing, focused on its row.
    fn herdr_missing() -> State {
        app(without_herdr(), None)
    }

    #[test]
    fn a_dimmed_tool_with_a_verified_install_offers_it_after_a_yes() {
        let mut s = herdr_missing();
        assert_eq!(
            status_of(
                tool("herdr").unwrap(),
                s.snapshot.as_ref().unwrap(),
                &s.tools
            ),
            ToolStatus::NotInstalled
        );
        assert_eq!(step(&mut s, Action::Activate), Effect::None);
        let Some(Overlay::Confirm(c)) = s.overlay.clone() else {
            panic!("{:?}", s.overlay)
        };
        assert_eq!(c, Confirm::InstallTool("herdr"));
        let (_, lines) = dialog_text(&c);
        let want = format!("  {}", quote_cmd(tool("herdr").unwrap().install.unwrap()));
        assert!(lines.contains(&want), "{lines:?}");
        assert_eq!(
            s.tools.installs.get("herdr"),
            None,
            "nothing runs before the yes"
        );
        assert_eq!(step(&mut s, Action::Yes), Effect::Install("herdr"));
        assert!(matches!(
            s.tools.installs.get("herdr"),
            Some(Install::Running(_))
        ));
        assert_eq!(s.overlay, None);
    }

    #[test]
    fn the_command_is_shown_as_a_shell_would_take_it() {
        assert_eq!(
            quote_cmd(&["sh", "-c", "curl -fsSL https://herdr.dev/install.sh | sh"]),
            "sh -c 'curl -fsSL https://herdr.dev/install.sh | sh'"
        );
        assert_eq!(quote_cmd(&["echo", "it's"]), "echo 'it'\\''s'");
        assert_eq!(quote_cmd(&["a", ""]), "a ''");
    }

    #[test]
    fn a_no_to_the_install_runs_nothing() {
        let mut s = herdr_missing();
        step(&mut s, Action::Activate);
        assert_eq!(step(&mut s, Action::No), Effect::None);
        assert_eq!((s.overlay.clone(), s.tools.installs.len()), (None, 0));
    }

    static ADMIN: Tool = Tool {
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
    fn an_install_that_needs_admin_is_shown_to_copy_not_run() {
        assert_eq!(
            install_offer(&ADMIN),
            Offer::Copy("apt-get install 'fake admin'".into())
        );
        let mut s = herdr_missing();
        assert_eq!(
            offer(&mut s, &ADMIN),
            Effect::Copy("apt-get install 'fake admin'".into())
        );
        assert_eq!(s.overlay, None, "no dialog, no job: the user runs it");
        assert!(s.toast.as_deref().unwrap().contains("administrator"));
        assert!(s.tools.installs.is_empty());
    }

    #[test]
    fn a_tool_with_no_install_command_says_where_to_get_it() {
        let mut s = herdr_missing();
        let t = tool("codex").unwrap();
        assert_eq!(install_offer(t), Offer::None);
        assert_eq!(offer(&mut s, t), Effect::None);
        assert!(s.toast.as_deref().unwrap().contains(t.docs));
    }

    #[test]
    fn a_finished_install_flips_the_row_and_shows_the_connect_hint() {
        let mut s = herdr_missing();
        step(&mut s, Action::Activate);
        step(&mut s, Action::Yes);
        s.apply(Msg::Job(herdr_job(), ended(Some(0), &["installed herdr"])));
        assert!(s.rescan_wanted, "the loop scans again");
        s.rescan_wanted = false; // the loop started that scan
        assert_eq!(
            status_of(
                tool("herdr").unwrap(),
                s.snapshot.as_ref().unwrap(),
                &s.tools
            ),
            ToolStatus::Checking
        );
        s.apply(Msg::Detected(Box::new(both())));
        assert_eq!(s.tools.installs.get("herdr"), None);
        assert_eq!(
            status_of(
                tool("herdr").unwrap(),
                s.snapshot.as_ref().unwrap(),
                &s.tools
            ),
            ToolStatus::Ready
        );
        assert!(
            s.toast
                .as_deref()
                .unwrap()
                .contains("Press c to connect or check it")
        );
    }

    #[test]
    fn an_install_the_scan_cannot_find_says_so_with_the_installers_words() {
        let mut s = herdr_missing();
        s.tools
            .installs
            .insert("herdr", Install::Running(Instant::now()));
        s.apply(Msg::Job(
            herdr_job(),
            ended(
                Some(0),
                &["installed herdr", "/home/x/.local/bin is not in your PATH"],
            ),
        ));
        s.rescan_wanted = false; // the loop started the scan this install asked for
        s.apply(Msg::Detected(Box::new(without_herdr())));
        assert_eq!(s.overlay, Some(Overlay::Failure("herdr")));
        let (_, text) = crate::tui::connect::failure_text(&s, "herdr", false);
        assert!(text.join("\n").contains("is not in your PATH"), "{text:?}");
    }

    #[test]
    fn a_failed_install_stays_needs_a_step_and_shows_the_tail() {
        let mut s = herdr_missing();
        step(&mut s, Action::Activate);
        step(&mut s, Action::Yes);
        s.apply(Msg::Job(
            herdr_job(),
            ended(Some(6), &["step one", "curl: (6) Could not resolve host"]),
        ));
        assert_eq!(s.overlay, Some(Overlay::Failure("herdr")));
        assert_eq!(
            status_of(
                tool("herdr").unwrap(),
                s.snapshot.as_ref().unwrap(),
                &s.tools
            ),
            ToolStatus::NeedsStep
        );
        let (_, text) = crate::tui::connect::failure_text(&s, "herdr", false);
        let text = text.join("\n");
        assert!(
            text.contains("✗ herdr: curl: (6) Could not resolve host."),
            "{text}"
        );
        let cmd = quote_cmd(tool("herdr").unwrap().install.unwrap());
        assert!(
            text.contains(&format!("Fix: {cmd}")),
            "the command to run yourself: {text}"
        );
        assert!(!s.rescan_wanted);
        // The failure is stale once a scan finds the tool.
        s.apply(Msg::Detected(Box::new(both())));
        assert_eq!(s.tools.installs.get("herdr"), None);
        assert_eq!(s.overlay, None);
    }

    #[test]
    fn a_hung_install_times_out_and_changes_nothing() {
        let mut s = herdr_missing();
        step(&mut s, Action::Activate);
        step(&mut s, Action::Yes);
        let hung = JobEnd {
            timed_out: true,
            code: None,
            ..ended(None, &[])
        };
        s.apply(Msg::Job(herdr_job(), hung));
        let Some(Install::Failed(f)) = s.tools.installs.get("herdr").cloned() else {
            panic!("{:?}", s.tools.installs)
        };
        assert!(f.reason.starts_with("did not finish after"), "{}", f.reason);
        assert!(f.unchanged.contains("no setting"));
        assert!(!s.dirty && !s.rescan_wanted);
        assert!(INSTALL_TIMEOUT > crate::tui::connect::CHECK_TIMEOUT_LOCAL);
        assert!(PULL_TIMEOUT > INSTALL_TIMEOUT);
    }

    #[test]
    fn a_scan_that_began_before_the_install_ended_does_not_judge_it() {
        let mut s = herdr_missing();
        s.tools
            .installs
            .insert("herdr", Install::Running(Instant::now()));
        s.apply(Msg::Job(herdr_job(), ended(Some(0), &["installed herdr"])));
        assert!(s.rescan_wanted, "the next scan is still wanted");
        s.apply(Msg::Detected(Box::new(without_herdr()))); // the old scan lands first
        assert!(matches!(
            s.tools.installs.get("herdr"),
            Some(Install::Done(_))
        ));
        assert_eq!(
            s.overlay, None,
            "no failure box from a scan that predates the install"
        );
    }

    #[test]
    fn a_second_enter_during_an_install_does_not_offer_it_again() {
        let mut s = herdr_missing();
        step(&mut s, Action::Activate);
        step(&mut s, Action::Yes);
        assert_eq!(step(&mut s, Action::Activate), Effect::None);
        assert_eq!(s.overlay, None, "no second confirm, so no second installer");
        assert!(s.toast.as_deref().unwrap().contains("is installing"));
    }

    #[test]
    fn a_model_pulled_by_hand_after_a_failed_pull_is_ready() {
        let mut s = app(both(), None);
        s.apply(Msg::Fit(Box::new(report(llmfit()))));
        s.tools
            .pulls
            .insert(CANDIDATES[0].tag, Pull::Failed("no network".into()));
        let mut snap = both();
        snap.local_llm = ollama(true, &["qwen3:4b-instruct-2507-q4_K_M"]);
        s.apply(Msg::Detected(Box::new(snap)));
        let row = assistant_rows(&s)
            .into_iter()
            .find(|r| r.model.is_some())
            .unwrap();
        assert_eq!(row.status, ToolStatus::Ready);
        assert!(!row.detail.contains("Last pull failed"), "{}", row.detail);
    }

    #[test]
    fn a_killed_install_reads_cancelled() {
        let mut s = herdr_missing();
        s.tools
            .installs
            .insert("herdr", Install::Running(Instant::now()));
        s.apply(Msg::Job(herdr_job(), ended(None, &[])));
        assert_eq!(s.toast.as_deref(), Some("Cancelled"));
        assert!(s.tools.installs.is_empty() && s.overlay.is_none());
    }

    fn assistant_rows(s: &State) -> Vec<Row> {
        rows(
            crate::catalog::Group::Assistant,
            s.snapshot.as_ref().unwrap(),
            &s.tools,
            s.pipeline.form.as_ref(),
        )
    }

    #[test]
    fn the_assistant_group_lists_fit_suggestions_with_verdict_size_and_pull_command() {
        let mut s = app(both(), None);
        s.apply(Msg::Fit(Box::new(report(llmfit()))));
        let list = assistant_rows(&s);
        let row = list
            .iter()
            .find(|r| r.model.is_some())
            .expect("a model row");
        assert_eq!(row.id, "qwen3:4b-instruct-2507-q4_K_M");
        assert!(
            row.pickable && !row.chosen,
            "a model is never the assistant answer"
        );
        for want in [
            "fits tightly",
            "needs 4.4 GB, 4.5 GB available",
            "about 60 tok/s",
            "Pull: ollama pull qwen3:4b-instruct-2507-q4_K_M",
        ] {
            assert!(row.detail.contains(want), "{want} missing: {}", row.detail);
        }
        let cli = list
            .iter()
            .position(|r| r.id == crate::tui::tools_screen::CLI_ONLY);
        let model = list.iter().position(|r| r.model.is_some());
        assert!(cli < model, "the model rows come after the answers");
        assert!(s.tools.assistant == crate::tui::tools_screen::AssistantChoice::Unset);
    }

    fn on_model_row(s: &mut State) {
        s.tools.group = crate::catalog::Group::Assistant;
        let at = assistant_rows(s)
            .iter()
            .position(|r| r.model.is_some())
            .unwrap();
        s.tools.cursor[crate::tui::tools_screen::index(crate::catalog::Group::Assistant)] = at;
    }

    #[test]
    fn a_pull_runs_after_a_confirm_with_the_tag_from_the_built_in_table() {
        let mut snap = both();
        snap.local_llm = ollama(true, &[]);
        let mut s = app(snap, None);
        s.apply(Msg::Fit(Box::new(report(llmfit()))));
        on_model_row(&mut s);
        assert_eq!(step(&mut s, Action::Activate), Effect::None);
        assert_eq!(s.overlay, Some(Overlay::Confirm(Confirm::Pull(0))));
        let (_, lines) = dialog_text(&Confirm::Pull(0));
        assert!(
            lines.contains(&"  ollama pull qwen3:4b-instruct-2507-q4_K_M".to_string()),
            "{lines:?}"
        );
        assert!(s.tools.pulls.is_empty(), "nothing runs before the yes");
        assert_eq!(step(&mut s, Action::Yes), Effect::Pull(0));
        assert_eq!(
            CANDIDATES[0].pull,
            ["ollama", "pull", "qwen3:4b-instruct-2507-q4_K_M"]
        );
        assert!(
            progress_line(&s, false)
                .unwrap()
                .contains("Downloading qwen3")
        );
        // Done: the row waits for the scan to list the model.
        s.apply(Msg::Job(JobId(PULL_BASE), ended(Some(0), &[])));
        assert!(s.tools.pulls.is_empty() && s.rescan_wanted);
    }

    #[test]
    fn a_pull_waits_for_a_running_ollama_and_skips_a_model_already_there() {
        let mut snap = both();
        snap.local_llm = ollama(false, &[]);
        let mut s = app(snap, None);
        s.apply(Msg::Fit(Box::new(report(llmfit()))));
        on_model_row(&mut s);
        step(&mut s, Action::Activate);
        assert_eq!(s.overlay, None);
        let toast = s.toast.clone().unwrap();
        assert!(
            toast.contains("ollama serve") && toast.contains("ollama pull qwen3"),
            "{toast}"
        );
        let mut snap = both();
        snap.local_llm = ollama(true, &["qwen3:4b-instruct-2507-q4_K_M"]);
        s.apply(Msg::Detected(Box::new(snap)));
        step(&mut s, Action::Activate);
        assert_eq!(s.overlay, None);
        assert!(s.toast.as_deref().unwrap().contains("already pulled"));
        let row = assistant_rows(&s)
            .into_iter()
            .find(|r| r.model.is_some())
            .unwrap();
        assert_eq!(row.status, ToolStatus::Ready);
    }

    #[test]
    fn a_failed_pull_says_so_and_gives_the_command() {
        let mut s = app(both(), None);
        s.apply(Msg::Fit(Box::new(report(llmfit()))));
        s.tools
            .pulls
            .insert(CANDIDATES[0].tag, Pull::Running(Instant::now()));
        s.apply(Msg::Job(
            JobId(PULL_BASE),
            ended(Some(1), &["Error: could not connect"]),
        ));
        let toast = s.toast.clone().unwrap();
        assert!(
            toast.contains("could not connect") && toast.contains("ollama pull qwen3"),
            "{toast}"
        );
        let row = assistant_rows(&s)
            .into_iter()
            .find(|r| r.model.is_some())
            .unwrap();
        assert_eq!(row.status, ToolStatus::NeedsStep);
        assert!(row.detail.contains("Last pull failed"));
    }

    #[test]
    fn the_scan_shows_one_fit_line_under_your_machine() {
        let fact = |s: &State| {
            s.check
                .view
                .as_ref()
                .and_then(|v| v.groups.iter().find(|g| g.id == "machine"))
                .and_then(|g| g.facts.iter().find(|f| f.label == "Local models"))
                .map(|f| f.value.clone())
        };
        // The scan first, then the fit.
        let mut s = app(both(), None);
        assert_eq!(fact(&s), None);
        s.apply(Msg::Fit(Box::new(report(llmfit()))));
        assert_eq!(
            fact(&s).as_deref(),
            Some("Qwen3-4B-Instruct-2507 fits tightly (llmfit 1.1.21)")
        );
        // A rescan rebuilds the view; the line stays.
        s.apply(Msg::Detected(Box::new(both())));
        assert!(fact(&s).is_some());
        // The fit first, then the scan.
        let mut s = State::new((100, 30));
        s.apply(Msg::Fit(Box::new(report(llmfit()))));
        s.apply(Msg::Detected(Box::new(both())));
        assert!(fact(&s).is_some());
    }

    #[test]
    fn a_missing_llmfit_offers_its_verified_install_or_its_docs_and_says_the_fallback_is_used() {
        let missing = Source::Fallback {
            reason: "llmfit is not installed".into(),
        };
        let line = fit_line(&report(missing.clone()));
        let docs = tool("llmfit").unwrap().docs;
        assert!(line.contains("fallback") && line.contains(docs), "{line}");
        // The catalog has no verified install for llmfit, so the docs are what is offered.
        assert_eq!(install_offer(tool("llmfit").unwrap()), Offer::None);
        let mut s = app(both(), None);
        s.apply(Msg::Fit(Box::new(report(missing))));
        let row = assistant_rows(&s)
            .into_iter()
            .find(|r| r.model.is_some())
            .unwrap();
        assert!(
            row.detail.contains("Fallback") && row.detail.contains(docs),
            "{}",
            row.detail
        );
        assert!(row.detail.contains("fit is not checked"));
    }

    #[test]
    fn a_model_row_cannot_hold_a_command_that_is_not_in_the_table() {
        // The row's id and the job's argv both come from `CANDIDATES`: a report that names a
        // candidate outside the table gets no row.
        static STRANGER: Candidate = Candidate {
            hf_name: "Evil/Model",
            tag: "evil:1b; rm -rf ~",
            pull: &["ollama", "pull", "evil:1b; rm -rf ~"],
            why: "x",
        };
        let mut s = app(both(), None);
        let mut r = report(llmfit());
        r.suggestions[0].candidate = &STRANGER;
        s.apply(Msg::Fit(Box::new(r)));
        assert!(assistant_rows(&s).iter().all(|row| row.model.is_none()));
    }

    #[test]
    fn a_logged_out_agent_still_opens_connect_and_a_missing_one_offers_docs() {
        let mut s = app(
            crate::tui::tools_screen::tests::snap(&[("claude", true, Login::LoggedOut)]),
            None,
        );
        s.tools.group = crate::catalog::Group::Builder;
        step(&mut s, Action::Activate);
        assert!(
            matches!(s.overlay, Some(Overlay::Connect(_))),
            "installed: connect, not install"
        );
    }
}
