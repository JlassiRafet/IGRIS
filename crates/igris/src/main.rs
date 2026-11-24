//! `igris`: the IGRIS command line.
//!
//! Output contract: one minified JSON object on stdout (so Hermes and scripts can read it), a short
//! human line on stderr. Exit codes: 0 done, 3 the task is not done (needs approval, blocked or
//! failed), 1 error, 2 bad usage.

// Unused until the jobs and the connect step use them (issues #60, #63); the allows go then.
#[allow(dead_code)]
mod activity;
// Unused until the Tools screen reads it (issue #62); the allow goes then.
#[allow(dead_code)]
mod catalog;
mod check;
#[allow(dead_code)]
mod clean;
mod doctor;
mod merge;
#[allow(dead_code)]
mod secret;
mod tui;

use clap::{Parser, Subcommand};
use igris_core::config::Config;
use igris_core::decide::igris_home;
use igris_core::git;
use igris_core::herdr::Herdr;
use igris_core::orchestrate::{Orchestrator, Report, Status, list_reports, load_report};
use serde_json::{Value, json};
use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// Agent orchestration: Hermes takes your request, a decision layer picks, agents build and review
/// in Herdr, and the shell decides when it is done.
#[derive(Parser)]
#[command(name = "igris", version)]
#[command(args_conflicts_with_subcommands = true)]
struct Cli {
    /// With no command, `igris` opens the terminal app on this repository.
    #[arg(long, default_value = ".")]
    repo: PathBuf,
    /// Do not capture the mouse (keeps your terminal's own text selection).
    #[arg(long)]
    no_mouse: bool,
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Check that this machine is ready: config, Herdr, the agents and the repository.
    Doctor {
        #[arg(long, default_value = ".")]
        repo: PathBuf,
        #[arg(long)]
        config: Option<PathBuf>,
        /// Also report what this machine has: hardware, local models, agents and their logins.
        #[arg(long)]
        json: bool,
    },
    /// Run one coding task through build, review and checks.
    Run {
        #[arg(long, default_value = ".")]
        repo: PathBuf,
        #[arg(long)]
        task: String,
        /// Start even though the task looks risky.
        #[arg(long)]
        approved: bool,
        #[arg(long)]
        config: Option<PathBuf>,
    },
    /// Continue a blocked task after you resolved the prompt in Herdr.
    Continue {
        id: String,
        #[arg(long)]
        config: Option<PathBuf>,
    },
    /// List tasks, or show one in full.
    Status { id: Option<String> },
    /// Merge an approved, green task into its base branch (fast-forward only).
    Merge {
        id: String,
        #[arg(long)]
        config: Option<PathBuf>,
    },
}

type Out = Result<(Value, u8), Box<dyn Error>>;

pub fn herdr_of(cfg: &Config) -> Herdr {
    Herdr {
        bin: cfg.herdr.bin.clone().into(),
        machine: cfg.herdr.machine.clone(),
    }
}

/// Print the one-line summary for a task report and pick the exit code.
fn report(r: Report) -> Out {
    let said = if r.next.is_empty() {
        &r.reason
    } else {
        &r.next
    };
    let v = serde_json::to_value(&r)?;
    eprintln!("{}: {said}", v["status"].as_str().unwrap_or("?"));
    Ok((v, if r.status == Status::Done { 0 } else { 3 }))
}

fn run(repo: &Path, task: &str, approved: bool, config: Option<&Path>) -> Out {
    let top =
        git::toplevel(repo).map_err(|_| format!("{} is not a git repository", repo.display()))?;
    if git::is_dirty(&top)? {
        return Err("the repo has uncommitted changes: commit or stash them first (IGRIS branches from the committed base)".into());
    }
    let cfg = Config::load(&top, config)?;
    let o = Orchestrator {
        herdr: herdr_of(&cfg),
        cfg: &cfg,
        home: igris_home(),
    };
    report(o.run(&top, task, approved)?)
}

fn resume(id: &str, config: Option<&Path>) -> Out {
    let home = igris_home();
    let r = load_report(&home, id)?;
    let cfg = Config::load(&r.repo, config)?;
    report(
        Orchestrator {
            herdr: herdr_of(&cfg),
            cfg: &cfg,
            home,
        }
        .resume(id)?,
    )
}

fn status(id: Option<&str>) -> Out {
    let home = igris_home();
    let v = match id {
        Some(id) => serde_json::to_value(load_report(&home, id)?)?,
        None => Value::Array(
            list_reports(&home)?
                .iter()
                .map(|r| json!({"id": r.id, "status": r.status, "stage": r.stage, "task": r.task, "next": r.next}))
                .collect(),
        ),
    };
    Ok((v, 0))
}

fn dispatch(cmd: Cmd) -> Out {
    match cmd {
        Cmd::Doctor { repo, config, json } => doctor::run(&repo, config.as_deref(), json),
        Cmd::Run {
            repo,
            task,
            approved,
            config,
        } => run(&repo, &task, approved, config.as_deref()),
        Cmd::Continue { id, config } => resume(&id, config.as_deref()),
        Cmd::Status { id } => status(id.as_deref()),
        Cmd::Merge { id, config } => {
            merge::run(&igris_home(), &id, config.as_deref()).map(|v| (v, 0))
        }
    }
}

/// The app needs a terminal on both ends. Scripts get the usage and exit code 2, never a hang.
fn app(repo: PathBuf, no_mouse: bool) -> ExitCode {
    use std::io::IsTerminal;
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        eprintln!("igris: the terminal app needs a terminal; use a command from the list below.");
        let _ = <Cli as clap::CommandFactory>::command().write_help(&mut std::io::stderr());
        return ExitCode::from(2);
    }
    match tui::run(tui::Opts { repo, no_mouse }) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("igris: {e}");
            ExitCode::from(1)
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let Some(cmd) = cli.cmd else {
        return app(cli.repo, cli.no_mouse);
    };
    match dispatch(cmd) {
        Ok((v, code)) => {
            println!("{v}");
            ExitCode::from(code)
        }
        Err(e) => {
            eprintln!("igris: {e}");
            println!("{}", json!({"status": "error", "error": e.to_string()}));
            ExitCode::from(1)
        }
    }
}
