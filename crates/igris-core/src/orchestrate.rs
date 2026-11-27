//! The loop: decide, build, review, check, report.
//!
//! Every step is saved to `<home>/tasks/<id>.json` before and after it runs, so a blocked or
//! interrupted run can be continued with `resume` without creating a second worktree or sending
//! the same prompt twice. The shell, not an agent, decides when the work is done: a task is `Done`
//! only when the reviewer approved and the check command exited 0.

use crate::config::Config;
use crate::decide::{self, Decision};
use crate::herdr::{AgentState, Herdr, HerdrError};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, thiserror::Error)]
pub enum OrchError {
    #[error(transparent)]
    Herdr(#[from] HerdrError),
    #[error("{0}")]
    Git(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("no task '{0}' (see `igris status`)")]
    NoTask(String),
    #[error("{0}")]
    Invalid(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Running,
    Done,
    Merged,
    Blocked,
    Failed,
    NeedsApproval,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Build,
    Review,
    Checks,
    Report,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Panes {
    pub builder: Option<String>,
    pub reviewer: Option<String>,
    pub checks: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Rounds {
    pub review: u32,
    pub fix: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CheckResult {
    pub exit: Option<i32>,
    pub tail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Report {
    pub id: String,
    pub task: String,
    pub status: Status,
    pub stage: Stage,
    pub repo: PathBuf,
    pub base: String,
    pub branch: String,
    pub workspace: String,
    pub worktree: PathBuf,
    pub panes: Panes,
    pub files_changed: Vec<String>,
    pub checks: CheckResult,
    pub verdict: Option<String>,
    pub findings: String,
    pub rounds: Rounds,
    pub decisions: Vec<Decision>,
    pub reason: String,
    pub next: String,
    // Resume bookkeeping.
    pub builder_started: bool,
    pub reviewer_started: bool,
    /// The agent (`builder` or `reviewer`) we sent a prompt to and have not seen settle yet.
    pub pending: Option<String>,
    pub review_tag: Option<String>,
    /// The `--config` file the run was started with, so the printed next step can repeat it.
    #[serde(default)]
    pub config: Option<PathBuf>,
}

/// The command that continues task `r`, as it can be pasted into a shell: the `--config` the run
/// was given is repeated, quoted.
fn continue_command(r: &Report) -> String {
    let mut c = format!("igris continue {}", r.id);
    if let Some(p) = &r.config {
        c.push_str(&format!(
            " --config {}",
            crate::herdr::sh_quote(&p.to_string_lossy())
        ));
    }
    c
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Approve,
    Changes,
}

impl Verdict {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Approve => "approve",
            Self::Changes => "changes",
        }
    }
}

/// Find the reviewer's verdict: the last line that starts with `<tag>:` followed by APPROVE or
/// CHANGES, ignoring leading terminal bullets. The tag is unique per request, so neither the
/// echoed instruction (the tag sits mid-sentence there) nor an earlier round's verdict can match.
/// Returns the verdict and everything after it (the findings).
pub fn parse_verdict(text: &str, tag: &str) -> (Option<Verdict>, String) {
    let lines: Vec<&str> = text.lines().collect();
    let mut found: Option<(usize, Verdict, String)> = None;
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim_start_matches(|c: char| !c.is_alphanumeric());
        let Some(rest) = t.strip_prefix(tag).and_then(|r| r.strip_prefix(':')) else {
            continue;
        };
        let rest = rest.trim();
        let verdict = if rest.starts_with("APPROVE") {
            Verdict::Approve
        } else if rest.starts_with("CHANGES") {
            Verdict::Changes
        } else {
            continue;
        };
        let after = rest
            .trim_start_matches(|c: char| c.is_alphabetic())
            .trim()
            .to_string();
        found = Some((i, verdict, after));
    }
    match found {
        None => (None, String::new()),
        Some((i, v, same_line)) => {
            let mut out = vec![same_line];
            out.extend(lines[i + 1..].iter().map(|l| l.to_string()));
            (Some(v), out.join("\n").trim().to_string())
        }
    }
}

fn hex4() -> String {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    let n = nanos ^ COUNTER.fetch_add(1, Ordering::SeqCst).wrapping_mul(7919) ^ std::process::id();
    format!("{:04x}", n & 0xffff)
}

fn is_dirty(dir: &Path) -> Result<bool, OrchError> {
    crate::git::is_dirty(dir).map_err(OrchError::Git)
}

fn changed_files(dir: &Path, base: &str) -> Result<Vec<String>, OrchError> {
    crate::git::changed_files(dir, base).map_err(OrchError::Git)
}

fn last_lines(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

fn task_path(home: &Path, id: &str) -> PathBuf {
    home.join("tasks").join(format!("{id}.json"))
}

/// Write the report atomically (temp file, then rename).
pub fn save_report(home: &Path, r: &Report) -> Result<(), OrchError> {
    let path = task_path(home, &r.id);
    std::fs::create_dir_all(path.parent().expect("task path has a parent"))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(r)?)?;
    Ok(std::fs::rename(tmp, path)?)
}

/// Ids are checked so a crafted id cannot leave the tasks folder.
pub fn load_report(home: &Path, id: &str) -> Result<Report, OrchError> {
    let ok = !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    let text = if ok {
        std::fs::read_to_string(task_path(home, id)).ok()
    } else {
        None
    };
    let text = text.ok_or_else(|| OrchError::NoTask(id.into()))?;
    Ok(serde_json::from_str(&text)?)
}

/// Every saved report, oldest first (ids start with the start time).
pub fn list_reports(home: &Path) -> Result<Vec<Report>, OrchError> {
    let dir = home.join("tasks");
    let mut ids: Vec<String> = match std::fs::read_dir(&dir) {
        Ok(rd) => rd
            .filter_map(|e| {
                e.ok()?
                    .file_name()
                    .to_str()?
                    .strip_suffix(".json")
                    .map(String::from)
            })
            .collect(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => vec![],
        Err(e) => return Err(e.into()),
    };
    ids.sort();
    ids.iter().map(|id| load_report(home, id)).collect()
}

pub struct Orchestrator<'a> {
    pub herdr: Herdr,
    pub cfg: &'a Config,
    /// Where tasks and the decision log live (`IGRIS_HOME`).
    pub home: PathBuf,
    /// The `--config` file given on the command line, if any (repeated in the next-step line).
    pub config_path: Option<PathBuf>,
}

impl Orchestrator<'_> {
    fn log_path(&self) -> PathBuf {
        self.home.join("decisions.jsonl")
    }

    fn save(&self, r: &Report) -> Result<(), OrchError> {
        save_report(&self.home, r)
    }

    pub fn load(&self, id: &str) -> Result<Report, OrchError> {
        load_report(&self.home, id)
    }

    fn agent_name(r: &Report, who: &str) -> String {
        let suffix = r.id.rsplit('-').next().unwrap_or("x");
        format!("{}-{suffix}", if who == "builder" { "ib" } else { "ir" })
    }

    pub fn run(&self, repo: &Path, task: &str, approved: bool) -> Result<Report, OrchError> {
        let id = format!(
            "t{}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_secs()),
            hex4()
        );
        let decisions = decide::decide(task, self.cfg, &[]);
        for d in &decisions {
            decide::log_decision(&self.log_path(), &id, d)?;
        }
        let answer = |q: &str| {
            decisions
                .iter()
                .find(|d| d.question == q)
                .map_or(String::new(), |d| d.answer.clone())
        };
        let (needs_approval, risk) = (answer("needs_approval"), answer("risk"));
        let mut r = Report {
            id: id.clone(),
            task: task.into(),
            status: Status::Running,
            stage: Stage::Build,
            repo: repo.to_path_buf(),
            base: self.cfg.policy.base.clone(),
            branch: format!("igris/{id}"),
            workspace: String::new(),
            worktree: PathBuf::new(),
            panes: Panes::default(),
            files_changed: vec![],
            checks: CheckResult::default(),
            verdict: None,
            findings: String::new(),
            rounds: Rounds::default(),
            decisions,
            reason: String::new(),
            next: String::new(),
            builder_started: false,
            reviewer_started: false,
            pending: None,
            review_tag: None,
            config: self.config_path.clone(),
        };
        if needs_approval == "yes" && !approved {
            r.status = Status::NeedsApproval;
            r.reason = format!("risk is {risk}");
            let again = r.config.as_ref().map_or(String::new(), |p| {
                format!(" --config {}", crate::herdr::sh_quote(&p.to_string_lossy()))
            });
            r.next = format!(
                "this task looks risky; run it again with --approved{again} to start it ({})",
                r.reason
            );
            self.save(&r)?;
            return Ok(r);
        }
        let wt = self.herdr.worktree_create(repo, &r.branch, &r.base)?;
        r.workspace = wt.workspace_id;
        r.panes.builder = Some(wt.root_pane);
        r.worktree = wt.path;
        self.save(&r)?;
        self.drive(&mut r)?;
        Ok(r)
    }

    /// Continue a blocked or interrupted task from where it stopped.
    pub fn resume(&self, id: &str) -> Result<Report, OrchError> {
        let mut r = self.load(id)?;
        match r.status {
            Status::Done | Status::Merged => return Ok(r),
            Status::NeedsApproval => {
                return Err(OrchError::Invalid(format!(
                    "task {id} has not started: run it again with --approved"
                )));
            }
            Status::Failed if r.pending.is_none() => {
                return Err(OrchError::Invalid(format!(
                    "task {id} failed ({}): start a new task",
                    r.reason
                )));
            }
            _ => {}
        }
        self.drive(&mut r)?;
        Ok(r)
    }

    fn drive(&self, r: &mut Report) -> Result<(), OrchError> {
        r.status = Status::Running;
        r.reason.clear();
        self.save(r)?;
        // A fix prompt was sent before we stopped: wait for it instead of sending it again.
        if r.pending.as_deref() == Some("builder")
            && r.stage != Stage::Build
            && !self.ask(r, "builder", "", self.cfg.policy.build_timeout_s)?
        {
            return Ok(());
        }
        loop {
            let advanced = match r.stage {
                Stage::Build => self.build(r)?,
                Stage::Review => self.review(r)?,
                Stage::Checks => self.checks(r)?,
                Stage::Report => return self.finish(r),
            };
            if !advanced {
                return Ok(());
            }
        }
    }

    /// Send `text` to `who` and wait for it to settle. `Ok(false)` means the run stopped and the
    /// report already says why (blocked, failed). When `who` still has an unanswered prompt from a
    /// previous run, wait for that one instead of sending another.
    fn ask(
        &self,
        r: &mut Report,
        who: &'static str,
        text: &str,
        timeout_s: u64,
    ) -> Result<bool, OrchError> {
        let name = Self::agent_name(r, who);
        let ms = timeout_s * 1000;
        let resuming = r.pending.as_deref() == Some(who);
        let res = if resuming {
            self.herdr.wait(&name, ms)
        } else {
            r.pending = Some(who.into());
            self.save(r)?;
            self.herdr.prompt(&name, text, ms)
        };
        match res {
            Ok(AgentState::Blocked) => {
                self.blocked(r, who)?;
                Ok(false)
            }
            Ok(_) => {
                r.pending = None;
                self.save(r)?;
                Ok(true)
            }
            // Refused before any input was sent, so the prompt is still to be delivered.
            Err(e)
                if !resuming && matches!(e.code.as_str(), "agent_blocked" | "agent_not_ready") =>
            {
                r.pending = None;
                self.blocked(r, who)?;
                Ok(false)
            }
            Err(e) if e.code == "timeout" => {
                let why = format!(
                    "timed out waiting for the {who}; it may still be working: `{}` waits for it",
                    continue_command(r)
                );
                self.fail(r, why)?;
                Ok(false)
            }
            Err(e) => Err(e.into()),
        }
    }

    fn blocked(&self, r: &mut Report, who: &str) -> Result<bool, OrchError> {
        let name = Self::agent_name(r, who);
        r.checks.tail = self.herdr.read(&name, 40).unwrap_or_default();
        r.status = Status::Blocked;
        r.reason = format!(
            "the {who} is waiting for input (Claude Code asks once per new folder: trust it in Herdr)"
        );
        r.next = format!("resolve it in Herdr, then run `{}`", continue_command(r));
        self.save(r)?;
        Ok(false)
    }

    fn fail(&self, r: &mut Report, reason: String) -> Result<bool, OrchError> {
        r.status = Status::Failed;
        r.reason = reason;
        r.next = format!(
            "see `igris status {}`; the worktree and panes are left open",
            r.id
        );
        self.save(r)?;
        self.outcome(r)?;
        Ok(false)
    }

    fn outcome(&self, r: &Report) -> Result<(), OrchError> {
        let o = json!({"status": r.status, "stage": r.stage, "checks_exit": r.checks.exit,
                       "verdict": r.verdict, "rounds": r.rounds, "reason": r.reason});
        Ok(decide::log_outcome(&self.log_path(), &r.id, &o)?)
    }

    /// Start `who`'s agent in its pane. `Ok(false)`: blocked at startup (folder trust, most often).
    fn start(&self, r: &mut Report, who: &'static str) -> Result<bool, OrchError> {
        let (kind, args, pane) = if who == "builder" {
            (
                &self.cfg.agents.builder,
                self.cfg.agents.builder_args.clone(),
                r.panes.builder.clone(),
            )
        } else {
            (
                &self.cfg.agents.reviewer,
                self.cfg.agents.reviewer_args(),
                r.panes.reviewer.clone(),
            )
        };
        let pane = pane.expect("pane is created before its agent starts");
        match self
            .herdr
            .start_agent(&Self::agent_name(r, who), kind, &pane, &args)
        {
            Ok(_) => {}
            // The agent is registered but waiting for input; once the user answers, it is ready.
            Err(e) if e.code == "agent_not_ready" => {
                self.mark_started(r, who);
                return self.blocked(r, who);
            }
            Err(e) => return Err(e.into()),
        }
        self.mark_started(r, who);
        self.save(r)?;
        Ok(true)
    }

    fn mark_started(&self, r: &mut Report, who: &str) {
        if who == "builder" {
            r.builder_started = true;
        } else {
            r.reviewer_started = true;
        }
    }

    fn build(&self, r: &mut Report) -> Result<bool, OrchError> {
        let timeout = self.cfg.policy.build_timeout_s;
        if !r.builder_started && !self.start(r, "builder")? {
            return Ok(false);
        }
        let card = format!(
            "Task: {}\n\nWork only in this worktree (branch {}). Make the change, commit it on this branch (do not push), then reply DONE.",
            r.task, r.branch
        );
        if !self.ask(r, "builder", &card, timeout)? {
            return Ok(false);
        }
        if is_dirty(&r.worktree)? {
            let nudge = "You left uncommitted changes. Commit all of your work now with git commit (do not push), then reply DONE.";
            if !self.ask(r, "builder", nudge, timeout)? {
                return Ok(false);
            }
            if is_dirty(&r.worktree)? {
                return self.fail(r, "the builder left uncommitted changes".into());
            }
        }
        r.files_changed = changed_files(&r.worktree, &r.base)?;
        if r.files_changed.is_empty() {
            return self.fail(r, "the builder made no changes".into());
        }
        r.stage = Stage::Review;
        self.save(r)?;
        Ok(true)
    }

    fn verdict(&self, r: &Report, tag: &str) -> Result<(Option<Verdict>, String), OrchError> {
        let out = self.herdr.read(&Self::agent_name(r, "reviewer"), 300)?;
        Ok(parse_verdict(&out, tag))
    }

    fn review(&self, r: &mut Report) -> Result<bool, OrchError> {
        let timeout = self.cfg.policy.build_timeout_s;
        if r.panes.reviewer.is_none() {
            let from = r.panes.builder.clone().expect("builder pane exists");
            r.panes.reviewer = Some(self.herdr.split(&from, &r.worktree, "right")?);
            self.save(r)?;
        }
        if !r.reviewer_started && !self.start(r, "reviewer")? {
            return Ok(false);
        }
        loop {
            let tag = match (r.pending.as_deref(), &r.review_tag) {
                (Some("reviewer"), Some(t)) => t.clone(),
                _ => {
                    let t = format!("VERDICT-{}", hex4());
                    r.review_tag = Some(t.clone());
                    t
                }
            };
            let prompt = format!(
                "Review the changes on this branch: run `git diff {}` and read the changed files. This is a read-only review: do not edit, create or delete anything.\n\nFinish your reply with one line that starts with {tag}: and then the word APPROVE or the word CHANGES. If you ask for changes, list each finding on the lines after it.",
                r.base
            );
            if !self.ask(r, "reviewer", &prompt, timeout)? {
                return Ok(false);
            }
            let (mut verdict, mut findings) = self.verdict(r, &tag)?;
            if verdict.is_none() {
                let again = format!(
                    "Reply with only your verdict: one line that starts with {tag}: and then the word APPROVE or the word CHANGES, followed by your findings."
                );
                if !self.ask(r, "reviewer", &again, timeout)? {
                    return Ok(false);
                }
                (verdict, findings) = self.verdict(r, &tag)?;
            }
            let Some(verdict) = verdict else {
                return self.fail(r, "the reviewer gave no verdict".into());
            };
            r.verdict = Some(verdict.as_str().into());
            r.findings = findings;
            if verdict == Verdict::Approve {
                r.stage = Stage::Checks;
                self.save(r)?;
                return Ok(true);
            }
            if r.rounds.review >= self.cfg.policy.max_review_rounds {
                let why = format!(
                    "the reviewer still asks for changes after {} fix rounds",
                    r.rounds.review
                );
                return self.fail(r, why);
            }
            r.rounds.review += 1;
            self.save(r)?;
            let fix = format!(
                "A reviewer asked for changes. Fix these findings, commit your work, then reply DONE.\n\n{}",
                r.findings
            );
            if !self.ask(r, "builder", &fix, timeout)? {
                return Ok(false);
            }
            r.files_changed = changed_files(&r.worktree, &r.base)?;
            self.save(r)?;
        }
    }

    fn checks(&self, r: &mut Report) -> Result<bool, OrchError> {
        if r.panes.checks.is_none() {
            let from = r
                .panes
                .reviewer
                .clone()
                .or_else(|| r.panes.builder.clone())
                .expect("a pane exists");
            r.panes.checks = Some(self.herdr.split(&from, &r.worktree, "down")?);
            self.save(r)?;
        }
        let pane = r.panes.checks.clone().expect("just created");
        let secs = self.cfg.checks.timeout_s;
        loop {
            let (code, tail) =
                self.herdr
                    .run_check(&pane, &self.cfg.checks.command, secs * 1000)?;
            r.checks = CheckResult {
                exit: code,
                tail: tail.clone(),
            };
            self.save(r)?;
            match code {
                Some(0) => {
                    r.stage = Stage::Report;
                    self.save(r)?;
                    return Ok(true);
                }
                None => {
                    return self.fail(
                        r,
                        format!("the check timed out after {secs}s; its pane is left open"),
                    );
                }
                Some(n) => {
                    if r.rounds.fix >= self.cfg.policy.max_fix_rounds {
                        let why = format!(
                            "the checks still fail (exit {n}) after {} fix rounds",
                            r.rounds.fix
                        );
                        return self.fail(r, why);
                    }
                    r.rounds.fix += 1;
                    self.save(r)?;
                    let fix = format!(
                        "The check `{}` failed with exit {n}. Last output:\n{}\n\nFix it, commit your work, then reply DONE.",
                        self.cfg.checks.command,
                        last_lines(&tail, 60)
                    );
                    if !self.ask(r, "builder", &fix, self.cfg.policy.build_timeout_s)? {
                        return Ok(false);
                    }
                }
            }
        }
    }

    fn finish(&self, r: &mut Report) -> Result<(), OrchError> {
        r.files_changed = changed_files(&r.worktree, &r.base)?;
        r.decisions = decide::decide(&r.task, self.cfg, &r.files_changed);
        for d in &r.decisions {
            decide::log_decision(&self.log_path(), &r.id, d)?;
        }
        let high = r
            .decisions
            .iter()
            .any(|d| d.question == "risk" && d.answer == "high");
        r.status = Status::Done;
        r.next = format!(
            "igris merge {}{}",
            r.id,
            if high {
                " (high risk: read the diff before you merge)"
            } else {
                ""
            }
        );
        self.save(r)?;
        self.outcome(r)
    }
}
