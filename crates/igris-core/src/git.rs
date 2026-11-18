//! The few git commands IGRIS runs itself (the agents run their own).

use std::path::{Path, PathBuf};
use std::process::Command;

/// Run `git -C <dir> <args>`; the error is git's own message.
pub fn run(dir: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .map_err(|e| format!("cannot run git: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

pub fn is_dirty(dir: &Path) -> Result<bool, String> {
    Ok(!run(dir, &["status", "--porcelain"])?.trim().is_empty())
}

/// Files that differ between `base` and the working tree.
pub fn changed_files(dir: &Path, base: &str) -> Result<Vec<String>, String> {
    Ok(run(dir, &["diff", "--name-only", base])?
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect())
}

pub fn toplevel(dir: &Path) -> Result<PathBuf, String> {
    run(dir, &["rev-parse", "--show-toplevel"]).map(|s| PathBuf::from(s.trim()))
}

pub fn current_branch(dir: &Path) -> Result<String, String> {
    run(dir, &["rev-parse", "--abbrev-ref", "HEAD"]).map(|s| s.trim().to_string())
}

pub fn has_branch(dir: &Path, name: &str) -> bool {
    run(
        dir,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/heads/{name}"),
        ],
    )
    .is_ok()
}
