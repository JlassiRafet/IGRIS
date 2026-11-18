//! `igris doctor`: is this machine ready to run a task? Each check names its fix.

use crate::{Out, herdr_of};
use igris_core::config::Config;
use igris_core::git;
use igris_core::herdr::Herdr;
use serde_json::json;
use std::path::{Path, PathBuf};

struct Check {
    id: &'static str,
    ok: bool,
    detail: String,
    fix: String,
}

fn ok(id: &'static str, detail: impl Into<String>) -> Check {
    Check {
        id,
        ok: true,
        detail: detail.into(),
        fix: String::new(),
    }
}

fn fail(id: &'static str, detail: impl Into<String>, fix: impl Into<String>) -> Check {
    Check {
        id,
        ok: false,
        detail: detail.into(),
        fix: fix.into(),
    }
}

#[cfg(unix)]
fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    p.metadata()
        .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(p: &Path) -> bool {
    p.is_file()
}

fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join(name))
        .find(|p| is_executable(p))
}

fn checks(repo: &Path, config: Option<&Path>) -> Vec<Check> {
    let top = git::toplevel(repo);
    let root = top.clone().unwrap_or_else(|_| repo.to_path_buf());
    let cfg = Config::load(&root, config);
    let mut out = vec![];

    out.push(match &cfg {
        Ok(c) => ok(
            "config",
            format!(
                "builder {}, reviewer {}, check `{}`",
                c.agents.builder, c.agents.reviewer, c.checks.command
            ),
        ),
        Err(e) => fail(
            "config",
            e.to_string(),
            "copy examples/igris.example.toml to <repo>/igris.toml and set checks.command",
        ),
    });

    let herdr = cfg.as_ref().map_or_else(|_| Herdr::local(), herdr_of);
    out.push(match herdr.call(&["agent", "list"]) {
        Ok(v) => ok(
            "herdr",
            format!(
                "answering ({} agents running)",
                v["agents"].as_array().map_or(0, Vec::len)
            ),
        ),
        Err(e) => fail("herdr", e.to_string(), e.hint),
    });

    for (id, kind) in [
        ("builder", cfg.as_ref().map(|c| c.agents.builder.clone())),
        ("reviewer", cfg.as_ref().map(|c| c.agents.reviewer.clone())),
    ] {
        out.push(match kind {
            Err(_) => fail(id, "not checked: the config is not valid", "fix the config first"),
            Ok(k) => match which(&k) {
                Some(p) => ok(id, format!("{k} at {}", p.display())),
                None => fail(
                    id,
                    format!("'{k}' is not on PATH"),
                    format!("install {k} and log in with your subscription or key; run it once in the parent folder of your worktrees so it trusts them"),
                ),
            },
        });
    }

    out.push(match &top {
        Ok(t) => ok("repo", t.display().to_string()),
        Err(_) => fail(
            "repo",
            format!("{} is not a git repository", repo.display()),
            "run igris inside a git repository, or pass --repo",
        ),
    });
    out.push(match &top {
        Err(_) => fail("clean", "not checked: no repository", "see the repo check"),
        Ok(t) => match git::is_dirty(t) {
            Ok(false) => ok("clean", "no uncommitted changes"),
            Ok(true) => fail(
                "clean",
                "the repo has uncommitted changes",
                "commit or stash them: IGRIS branches from the committed base",
            ),
            Err(e) => fail("clean", e, "check that git works in this folder"),
        },
    });
    out.push(match (&top, &cfg) {
        (Ok(t), Ok(c)) if git::has_branch(t, &c.policy.base) => {
            ok("base", format!("branch {} exists", c.policy.base))
        }
        (Ok(_), Ok(c)) => fail(
            "base",
            format!("branch '{}' does not exist", c.policy.base),
            "create it, or set policy.base in igris.toml",
        ),
        _ => fail(
            "base",
            "not checked: needs a repository and a valid config",
            "see the checks above",
        ),
    });
    out
}

pub fn run(repo: &Path, config: Option<&Path>) -> Out {
    let list = checks(repo, config);
    for c in &list {
        if c.ok {
            eprintln!("ok    {:<9} {}", c.id, c.detail);
        } else {
            eprintln!("fail  {:<9} {} -> {}", c.id, c.detail, c.fix);
        }
    }
    let all = list.iter().all(|c| c.ok);
    let v = json!({
        "ok": all,
        "checks": list.iter().map(|c| json!({"id": c.id, "ok": c.ok, "detail": c.detail, "fix": c.fix})).collect::<Vec<_>>(),
    });
    Ok((v, if all { 0 } else { 1 }))
}
