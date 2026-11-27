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

/// Files that are modified, staged or untracked, as `XY path` (`??` is untracked). Read with
/// `-z`, so a name with a space or non-ASCII characters comes through as it is on disk.
pub fn dirty_files(dir: &Path) -> Result<Vec<String>, String> {
    let out = run(dir, &["status", "--porcelain", "-z"])?;
    let mut entries = out.split('\0').filter(|e| !e.is_empty());
    let mut files = Vec::new();
    while let Some(e) = entries.next() {
        if e.len() >= 4 && e[..2].contains(['R', 'C']) {
            entries.next(); // a rename or copy is followed by the original path
        }
        files.push(e.to_string());
    }
    Ok(files)
}

pub fn is_dirty(dir: &Path) -> Result<bool, String> {
    Ok(!dirty_files(dir)?.is_empty())
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
