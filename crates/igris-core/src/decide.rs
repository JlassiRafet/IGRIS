//! Decisions and the decision log.
//!
//! The rules backend answers the same typed questions Jev and Laya will (a choice, a score, a
//! yes/no, each with a confidence and a source), so swapping the engine later does not touch the
//! loop. Every answer is logged with the task's outcome: that log is the data for comparing
//! engines and for training Laya.

use crate::config::Config;
use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Choice,
    Score,
    Noul,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Decision {
    pub question: String,
    pub kind: Kind,
    pub answer: String,
    pub confidence: f32,
    pub source: String,
}

/// Compile the risky-path patterns. Used by the config loader (so a bad pattern is a clear error
/// at load) and by `decide`.
pub fn risky_globs(patterns: &[String]) -> Result<GlobSet, String> {
    let mut b = GlobSetBuilder::new();
    for p in patterns {
        b.add(Glob::new(p).map_err(|e| format!("bad pattern '{p}': {}", e.kind()))?);
    }
    b.build().map_err(|e| e.to_string())
}

fn words(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
}

fn rules(question: &str, kind: Kind, answer: &str) -> Decision {
    Decision {
        question: question.into(),
        kind,
        answer: answer.into(),
        confidence: 1.0,
        source: "rules".into(),
    }
}

/// Answer the four questions for `task`, given the paths the work touched (empty before the work).
pub fn decide(task: &str, cfg: &Config, paths: &[String]) -> Vec<Decision> {
    let small = task.chars().count() < 80
        && words(task).any(|w| ["fix", "rename", "typo", "bump"].contains(&w.as_str()));
    // shortcut: a prefix match, so "auth" also flags "author"; that errs toward asking you, which
    // is the safe side. Replace with the decision model's score in a later milestone.
    let risky_word = words(task).any(|w| {
        cfg.policy
            .risky_words
            .iter()
            .any(|r| w.starts_with(&r.to_lowercase()))
    });
    let risky_path = match risky_globs(&cfg.policy.risky_paths) {
        Ok(set) => paths.iter().any(|p| {
            set.is_match(p)
                || Path::new(p)
                    .file_name()
                    .is_some_and(|n| set.is_match(Path::new(n)))
        }),
        Err(_) => true, // fail closed: patterns we cannot read are treated as a match
    };
    let high = risky_word || risky_path;
    vec![
        rules("agent", Kind::Choice, &cfg.agents.builder),
        rules("tier", Kind::Choice, if small { "small" } else { "mid" }),
        rules("risk", Kind::Score, if high { "high" } else { "low" }),
        rules(
            "needs_approval",
            Kind::Noul,
            if high { "yes" } else { "no" },
        ),
    ]
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Append one JSON line; the file is never rewritten.
fn append(path: &Path, v: &Value) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut line = serde_json::to_string(v).map_err(std::io::Error::other)?;
    line.push('\n');
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?
        .write_all(line.as_bytes())
}

pub fn log_decision(path: &Path, task_id: &str, d: &Decision) -> std::io::Result<()> {
    append(
        path,
        &json!({"ts": now(), "task_id": task_id, "kind": "decision", "decision": d}),
    )
}

pub fn log_outcome(path: &Path, task_id: &str, outcome: &Value) -> std::io::Result<()> {
    append(
        path,
        &json!({"ts": now(), "task_id": task_id, "kind": "outcome", "outcome": outcome}),
    )
}

/// `$IGRIS_HOME`, else `~/.igris`.
pub fn igris_home_from(env: Option<OsString>, home: Option<OsString>) -> PathBuf {
    match (env.filter(|e| !e.is_empty()), home) {
        (Some(e), _) => PathBuf::from(e),
        (None, Some(h)) => PathBuf::from(h).join(".igris"),
        (None, None) => PathBuf::from(".igris"),
    }
}

pub fn igris_home() -> PathBuf {
    igris_home_from(std::env::var_os("IGRIS_HOME"), std::env::var_os("HOME"))
}
