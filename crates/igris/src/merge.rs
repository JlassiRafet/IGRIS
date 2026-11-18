//! `igris merge <id>`: the human's yes. Only an approved, green task merges, and only as a
//! fast-forward, so the reviewed commits are exactly what lands.

use crate::herdr_of;
use igris_core::config::Config;
use igris_core::decide::log_outcome;
use igris_core::git;
use igris_core::herdr::Herdr;
use igris_core::orchestrate::{Status, load_report, save_report};
use serde_json::{Value, json};
use std::error::Error;
use std::path::Path;

pub fn run(home: &Path, id: &str, config: Option<&Path>) -> Result<Value, Box<dyn Error>> {
    let mut r = load_report(home, id)?;
    let ready = r.status == Status::Done
        && r.checks.exit == Some(0)
        && r.verdict.as_deref() == Some("approve");
    if !ready {
        return Err(format!(
            "task {id} is not mergeable: it needs status done, checks exit 0 and a reviewer approval (it has status {:?}, checks exit {:?}, verdict {:?})",
            r.status, r.checks.exit, r.verdict
        )
        .into());
    }
    let on = git::current_branch(&r.repo)?;
    if on != r.base {
        return Err(format!("the repo is on '{on}': switch to {} before merging", r.base).into());
    }
    if git::is_dirty(&r.repo)? {
        return Err("the repo has uncommitted changes: commit or stash them before merging".into());
    }
    git::run(&r.repo, &["merge", "--ff-only", &r.branch]).map_err(|e| {
        format!("cannot fast-forward {} onto {}: {e}. The base moved: rebase the task branch in its worktree, then merge again", r.branch, r.base)
    })?;
    let commit = git::run(&r.repo, &["rev-parse", "--short", "HEAD"])?
        .trim()
        .to_string();

    // The work is merged; cleanup problems are reported, not fatal.
    let herdr = Config::load(&r.repo, config).map_or_else(|_| Herdr::local(), |c| herdr_of(&c));
    let cleanup = match herdr.remove_worktree(&r.workspace) {
        Err(e) => format!("worktree left open: {e}"),
        Ok(()) => match git::run(&r.repo, &["branch", "-d", &r.branch]) {
            Ok(_) => "ok".to_string(),
            Err(e) => format!("branch left: {e}"),
        },
    };

    r.status = Status::Merged;
    r.next.clear();
    save_report(home, &r)?;
    log_outcome(
        &home.join("decisions.jsonl"),
        id,
        &json!({"merged": true, "commit": commit, "checks_exit": 0, "verdict": "approve"}),
    )?;
    eprintln!(
        "merged {} into {} ({commit}); cleanup: {cleanup}",
        r.branch, r.base
    );
    Ok(
        json!({"id": id, "status": "merged", "branch": r.branch, "base": r.base, "commit": commit, "cleanup": cleanup}),
    )
}
