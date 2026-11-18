//! Thin wrapper over the `herdr` CLI. Every Herdr call in IGRIS goes through here.
//!
//! Shapes below come from recordings of Herdr 0.9.1 (`tests/fixtures/herdr`):
//! success is `{"id":..,"result":{..}}` on stdout, errors are `{"error":{"code","message"}}` on
//! stderr with exit 1, `pane run` prints nothing, and the `read` commands print plain text.
//! Nothing here retries: a failed call may still have changed Herdr's state.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("herdr {code}: {message} ({hint})")]
pub struct HerdrError {
    pub code: String,
    pub message: String,
    pub hint: String,
}

impl HerdrError {
    fn new(code: &str, message: impl Into<String>) -> Self {
        let hint = match code {
            "agent_not_ready" | "agent_blocked" => {
                "the agent is waiting for input, often a folder-trust prompt: resolve it in Herdr \
                 (Claude Code asks once per new folder, so trust the parent folder of your \
                 worktrees), then run `igris continue <id>`"
            }
            "timeout" => {
                "the command timed out but may still be running: check the pane before retrying"
            }
            "pane_not_found" | "agent_not_found" => {
                "the pane or agent was closed: check `herdr pane list`"
            }
            "server_unavailable" => "start herdr, or check `herdr status`",
            "not_installed" => "install herdr (https://herdr.dev) and make sure it is on PATH",
            _ => "see `herdr status`; calls that change state are never retried automatically",
        };
        Self {
            code: code.into(),
            message: message.into(),
            hint: hint.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentState {
    Idle,
    Working,
    Blocked,
    Done,
    Unknown,
}

impl AgentState {
    fn parse(s: &str) -> Self {
        match s {
            "idle" => Self::Idle,
            "working" => Self::Working,
            "blocked" => Self::Blocked,
            "done" => Self::Done,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Worktree {
    pub workspace_id: String,
    pub root_pane: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct Herdr {
    pub bin: PathBuf,
    /// A saved SSH machine label; every call is prefixed with `--machine <label>`.
    pub machine: Option<String>,
}

/// Quote `s` as one POSIX shell word (`'` becomes `'\''`).
pub fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

fn unique() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    format!(
        "{:x}{:x}{:x}",
        std::process::id(),
        nanos,
        COUNTER.fetch_add(1, Ordering::SeqCst)
    )
}

fn text_at(v: &Value, path: &[&str]) -> Result<String, HerdrError> {
    let mut cur = v;
    for key in path {
        cur = &cur[*key];
    }
    cur.as_str().map(str::to_owned).ok_or_else(|| {
        HerdrError::new(
            "bad_output",
            format!("herdr output has no {}", path.join(".")),
        )
    })
}

fn state_of(v: &Value) -> AgentState {
    AgentState::parse(v["agent"]["agent_status"].as_str().unwrap_or(""))
}

impl Herdr {
    pub fn local() -> Self {
        Self {
            bin: "herdr".into(),
            machine: None,
        }
    }

    fn exec(&self, args: &[&str]) -> Result<String, HerdrError> {
        let mut cmd = Command::new(&self.bin);
        if let Some(m) = &self.machine {
            cmd.arg("--machine").arg(m);
        }
        let out = cmd.args(args).output().map_err(|e| {
            HerdrError::new(
                "not_installed",
                format!("cannot run {}: {e}", self.bin.display()),
            )
        })?;
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        if out.status.success() {
            return Ok(stdout);
        }
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        for text in [&stderr, &stdout] {
            if let Ok(v) = serde_json::from_str::<Value>(text.trim())
                && let Some(code) = v["error"]["code"].as_str()
            {
                return Err(HerdrError::new(
                    code,
                    v["error"]["message"].as_str().unwrap_or(""),
                ));
            }
        }
        let msg = stderr.trim();
        let msg = if msg.is_empty() {
            format!("herdr exited with {}", out.status)
        } else {
            msg.to_owned()
        };
        Err(HerdrError::new("server_unavailable", msg))
    }

    /// Run `herdr <args>` and return the `result` object (`Null` when Herdr prints nothing).
    pub fn call(&self, args: &[&str]) -> Result<Value, HerdrError> {
        let out = self.exec(args)?;
        if out.trim().is_empty() {
            return Ok(Value::Null);
        }
        let v: Value = serde_json::from_str(&out).map_err(|e| {
            HerdrError::new("bad_output", format!("herdr printed invalid JSON: {e}"))
        })?;
        Ok(v.get("result").cloned().unwrap_or(v))
    }

    /// Like `call`, for the `read` commands, which print plain text.
    pub fn call_text(&self, args: &[&str]) -> Result<String, HerdrError> {
        self.exec(args)
    }

    pub fn worktree_create(
        &self,
        repo: &Path,
        branch: &str,
        base: &str,
    ) -> Result<Worktree, HerdrError> {
        let repo = repo.to_string_lossy();
        let v = self.call(&[
            "worktree",
            "create",
            "--cwd",
            &repo,
            "--branch",
            branch,
            "--base",
            base,
            "--no-focus",
        ])?;
        Ok(Worktree {
            workspace_id: text_at(&v, &["workspace", "workspace_id"])?,
            root_pane: text_at(&v, &["root_pane", "pane_id"])?,
            path: text_at(&v, &["worktree", "path"])?.into(),
        })
    }

    pub fn split(&self, pane: &str, cwd: &Path, direction: &str) -> Result<String, HerdrError> {
        let cwd = cwd.to_string_lossy();
        let v = self.call(&[
            "pane",
            "split",
            pane,
            "--direction",
            direction,
            "--cwd",
            &cwd,
            "--no-focus",
        ])?;
        text_at(&v, &["pane", "pane_id"])
    }

    /// Start `kind` in `pane`. A blocked start (for example Claude Code's folder-trust prompt)
    /// is an `agent_not_ready` error; the agent stays registered so it can be read.
    pub fn start_agent(
        &self,
        name: &str,
        kind: &str,
        pane: &str,
        args: &[String],
    ) -> Result<AgentState, HerdrError> {
        let mut a = vec!["agent", "start", name, "--kind", kind, "--pane", pane];
        if !args.is_empty() {
            a.push("--");
            a.extend(args.iter().map(String::as_str));
        }
        Ok(state_of(&self.call(&a)?))
    }

    /// Send `text` and wait until the agent settles; returns the state it settled in.
    pub fn prompt(
        &self,
        name: &str,
        text: &str,
        timeout_ms: u64,
    ) -> Result<AgentState, HerdrError> {
        let t = timeout_ms.to_string();
        Ok(state_of(&self.call(&[
            "agent",
            "prompt",
            name,
            text,
            "--wait",
            "--timeout",
            &t,
        ])?))
    }

    /// Wait until the agent settles (idle, done or blocked), for example after the user has answered
    /// a prompt in Herdr. Used to resume without sending the same prompt twice.
    pub fn wait(&self, name: &str, timeout_ms: u64) -> Result<AgentState, HerdrError> {
        let t = timeout_ms.to_string();
        Ok(state_of(&self.call(&[
            "agent",
            "wait",
            name,
            "--timeout",
            &t,
        ])?))
    }

    pub fn read(&self, name: &str, lines: u32) -> Result<String, HerdrError> {
        let n = lines.to_string();
        self.call_text(&[
            "agent",
            "read",
            name,
            "--source",
            "recent-unwrapped",
            "--lines",
            &n,
        ])
    }

    /// Run `cmd` in `pane` under `sh -c` (so the user's login shell does not matter) and wait for
    /// its exit code. The marker carries a per-run nonce because `wait-output` also matches output
    /// already on screen, and a re-run in the same pane must not match the previous run's marker.
    /// Returns `(None, tail)` on timeout; the pane is left open.
    pub fn run_check(
        &self,
        pane: &str,
        cmd: &str,
        timeout_ms: u64,
    ) -> Result<(Option<i32>, String), HerdrError> {
        let marker = format!("IGRIS_EXIT_{}=", unique());
        let line = format!("sh -c {}", sh_quote(&format!("{cmd}; echo {marker}$?")));
        self.call(&["pane", "run", pane, &line])?;
        let tail = || {
            self.call_text(&[
                "pane",
                "read",
                pane,
                "--source",
                "recent-unwrapped",
                "--lines",
                "200",
            ])
        };
        let t = timeout_ms.to_string();
        match self.call(&[
            "pane",
            "wait-output",
            pane,
            "--regex",
            &format!(r"{marker}\d+"),
            "--timeout",
            &t,
        ]) {
            Ok(v) => {
                let code = text_at(&v, &["matched_line"])?
                    .rsplit('=')
                    .next()
                    .and_then(|d| d.trim().parse().ok())
                    .ok_or_else(|| HerdrError::new("bad_output", "could not read the exit code"))?;
                Ok((Some(code), tail()?))
            }
            Err(e) if e.code == "timeout" => Ok((None, tail()?)),
            Err(e) => Err(e),
        }
    }

    pub fn remove_worktree(&self, workspace_id: &str) -> Result<(), HerdrError> {
        self.call(&["worktree", "remove", "--workspace", workspace_id])
            .map(|_| ())
    }
}
