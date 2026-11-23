#![cfg(unix)] // the fake Herdr is a shell script
mod common;
use common::{FIXTURES, FakeHerdr};
use igris_core::config::Config;
use igris_core::herdr::Herdr;
use igris_core::orchestrate::{Orchestrator, Stage, Status, parse_verdict};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn tmp(name: &str) -> PathBuf {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let d = std::env::temp_dir().join(format!("igris-orc-{}-{n}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn git(dir: &Path, args: &[&str]) {
    let o = Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        o.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&o.stderr)
    );
}

/// A repo on `main` plus a worktree with one committed change (a.txt), like a builder would leave.
struct World {
    repo: PathBuf,
    wt: PathBuf,
    home: PathBuf,
    fake: FakeHerdr,
}

fn world(commit_change: bool) -> World {
    let root = tmp("w");
    let repo = root.join("repo");
    fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    fs::write(repo.join("README.md"), "x").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "init"]);
    let wt = root.join("wt");
    git(
        &repo,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "wt-branch",
            wt.to_str().unwrap(),
        ],
    );
    if commit_change {
        fs::write(wt.join("a.txt"), "change").unwrap();
        git(&wt, &["add", "."]);
        git(&wt, &["commit", "-q", "-m", "work"]);
    }
    let fake = FakeHerdr::new();
    let created = fs::read_to_string(format!("{FIXTURES}/worktree-create.json"))
        .unwrap()
        .replace(
            "/home/user/.herdr/worktrees/repo/igris-fx1",
            wt.to_str().unwrap(),
        );
    fake.on_raw("worktree-create", &created);
    fake.on("pane-split", "pane-split.json");
    fake.on("agent-start", "agent-start-ok.json");
    fake.on("agent-prompt", "agent-prompt-ok.json");
    fake.on("agent-wait", "agent-wait-ok.json");
    fake.on("pane-run", "pane-run.json");
    fake.on("pane-wait-output", "pane-wait-output.json"); // exit 0
    fake.on("pane-read", "pane-read.txt");
    World {
        repo,
        wt,
        home: root.join("home"),
        fake,
    }
}

fn cfg() -> Config {
    let mut c = Config::default();
    c.checks.command = "true".into();
    c
}

impl World {
    fn orch<'a>(&self, cfg: &'a Config) -> Orchestrator<'a> {
        Orchestrator {
            herdr: Herdr {
                bin: self.fake.bin(),
                machine: None,
            },
            cfg,
            home: self.home.clone(),
        }
    }
    fn count(&self, a: &str, b: &str) -> usize {
        self.fake
            .calls()
            .iter()
            .filter(|c| c[0] == a && c[1] == b)
            .count()
    }
    fn decision_lines(&self) -> usize {
        fs::read_to_string(self.home.join("decisions.jsonl")).map_or(0, |t| t.lines().count())
    }
}

const APPROVE: &str = "The change looks right.\n@TAG@: APPROVE\n";

#[test]
fn happy_path_is_done_and_approved() {
    let w = world(true);
    w.fake.queue_raw_ok("agent-read", APPROVE);
    let c = cfg();
    let r = w.orch(&c).run(&w.repo, "add a login page", false).unwrap();
    assert_eq!((r.status, r.stage), (Status::Done, Stage::Report));
    assert_eq!(r.verdict.as_deref(), Some("approve"));
    assert_eq!(r.checks.exit, Some(0));
    assert_eq!(r.files_changed, ["a.txt"]);
    assert!(r.next.contains("igris merge"), "{}", r.next);
    assert_eq!(w.count("agent", "start"), 2);
    let starts: Vec<_> = w
        .fake
        .calls()
        .into_iter()
        .filter(|c| c[1] == "start")
        .collect();
    assert!(starts[0].contains(&"claude".to_string()));
    assert!(
        starts[1]
            .windows(2)
            .any(|p| p == ["--sandbox", "read-only"]),
        "reviewer must be read-only: {:?}",
        starts[1]
    );
    assert!(
        w.fake
            .calls()
            .iter()
            .any(|c| c[1] == "run" && c[3].starts_with("sh -c "))
    );
    assert!(w.home.join("tasks").join(format!("{}.json", r.id)).exists());
    assert_eq!(
        w.decision_lines(),
        4 + 4 + 1,
        "initial and final decisions, plus the outcome"
    );
}

#[test]
fn changes_then_approve_costs_one_review_round() {
    let w = world(true);
    w.fake.queue_raw_ok(
        "agent-read",
        "@TAG@: CHANGES\nThe empty password is accepted.\n",
    );
    w.fake.queue_raw_ok("agent-read", APPROVE);
    let c = cfg();
    let r = w.orch(&c).run(&w.repo, "add a login page", false).unwrap();
    assert_eq!((r.status, r.rounds.review), (Status::Done, 1));
    let prompts: Vec<_> = w
        .fake
        .calls()
        .into_iter()
        .filter(|c| c[1] == "prompt")
        .collect();
    assert!(
        prompts
            .iter()
            .any(|p| p[2].starts_with("ib-") && p[3].contains("empty password is accepted")),
        "builder gets the findings"
    );
}

#[test]
fn a_risky_task_starts_nothing_until_approved() {
    let w = world(true);
    let c = cfg();
    let r = w
        .orch(&c)
        .run(&w.repo, "run the db migration", false)
        .unwrap();
    assert_eq!(r.status, Status::NeedsApproval);
    assert!(w.fake.calls().is_empty(), "no herdr call before approval");
    assert!(r.next.contains("--approved"));
    w.fake.queue_raw_ok("agent-read", APPROVE);
    let r = w
        .orch(&c)
        .run(&w.repo, "run the db migration", true)
        .unwrap();
    assert_eq!(r.status, Status::Done);
}

#[test]
fn a_folder_trust_prompt_at_startup_is_reported_not_crashed() {
    let w = world(true);
    w.fake.fail("agent-start", "error-agent-not-ready.txt");
    w.fake.on("agent-read", "agent-read-blocked.txt");
    let c = cfg();
    let r = w.orch(&c).run(&w.repo, "add a login page", false).unwrap();
    assert_eq!((r.status, r.stage), (Status::Blocked, Stage::Build));
    assert!(
        r.checks.tail.contains("trust this folder"),
        "{}",
        r.checks.tail
    );
    assert!(r.next.contains("igris continue"), "{}", r.next);
}

#[test]
fn resume_picks_up_a_blocked_run_without_creating_a_second_worktree() {
    let w = world(true);
    w.fake.fail("agent-start", "error-agent-not-ready.txt");
    w.fake.on("agent-read", "agent-read-blocked.txt");
    let c = cfg();
    let o = w.orch(&c);
    let r = o.run(&w.repo, "add a login page", false).unwrap();
    assert_eq!(r.status, Status::Blocked);
    // The user trusts the folder in Herdr; the builder is now ready and the reviewer approves.
    w.fake.on("agent-start", "agent-start-ok.json");
    w.fake.on_raw("agent-read", APPROVE);
    let r = o.resume(&r.id).unwrap();
    assert_eq!(r.status, Status::Done);
    assert_eq!(w.count("worktree", "create"), 1);
}

#[test]
fn a_missing_verdict_is_asked_for_once_then_fails() {
    let w = world(true);
    w.fake
        .on_raw("agent-read", "I looked at it and it seems fine.\n");
    let c = cfg();
    let r = w.orch(&c).run(&w.repo, "add a login page", false).unwrap();
    assert_eq!((r.status, r.stage), (Status::Failed, Stage::Review));
    assert!(r.reason.contains("no verdict"), "{}", r.reason);
    let reviewer_prompts = w
        .fake
        .calls()
        .iter()
        .filter(|c| c[1] == "prompt" && c[2].starts_with("ir-"))
        .count();
    assert_eq!(reviewer_prompts, 2, "one review, one re-ask");
}

#[test]
fn a_check_that_hangs_fails_with_the_output_tail_and_leaves_the_pane() {
    let w = world(true);
    w.fake.queue_raw_ok("agent-read", APPROVE);
    w.fake.fail("pane-wait-output", "error-wait-timeout.txt");
    let c = cfg();
    let r = w.orch(&c).run(&w.repo, "add a login page", false).unwrap();
    assert_eq!((r.status, r.stage), (Status::Failed, Stage::Checks));
    assert_eq!(r.checks.exit, None);
    assert!(
        r.reason.contains("timed out") && r.checks.tail.contains("hello"),
        "{} / {}",
        r.reason,
        r.checks.tail
    );
    assert_eq!(w.count("pane", "close"), 0);
}

#[test]
fn failing_checks_are_fixed_by_the_builder_twice_then_give_up() {
    let w = world(true);
    w.fake.queue_raw_ok("agent-read", APPROVE);
    let red = r#"{"id":"x","result":{"matched_line":"IGRIS_EXIT_ab=1","type":"output_matched"}}"#;
    for _ in 0..3 {
        w.fake.queue_raw_ok("pane-wait-output", red);
    }
    let c = cfg();
    let r = w.orch(&c).run(&w.repo, "add a login page", false).unwrap();
    assert_eq!(
        (r.status, r.stage, r.rounds.fix),
        (Status::Failed, Stage::Checks, 2)
    );
    assert!(r.reason.contains("exit 1"), "{}", r.reason);
    assert_eq!(
        w.count("pane", "wait-output"),
        3,
        "first run plus two re-runs"
    );
}

#[test]
fn a_builder_that_changed_nothing_fails() {
    let w = world(false);
    let c = cfg();
    let r = w.orch(&c).run(&w.repo, "add a login page", false).unwrap();
    assert_eq!((r.status, r.stage), (Status::Failed, Stage::Build));
    assert!(r.reason.contains("no changes"), "{}", r.reason);
}

#[test]
fn a_builder_that_leaves_uncommitted_work_is_asked_once_then_fails() {
    let w = world(true);
    fs::write(w.wt.join("b.txt"), "left behind").unwrap();
    let c = cfg();
    let r = w.orch(&c).run(&w.repo, "add a login page", false).unwrap();
    assert_eq!((r.status, r.stage), (Status::Failed, Stage::Build));
    assert!(r.reason.contains("uncommitted"), "{}", r.reason);
    let builder_prompts = w
        .fake
        .calls()
        .iter()
        .filter(|c| c[1] == "prompt" && c[2].starts_with("ib-"))
        .count();
    assert_eq!(builder_prompts, 2, "the task, then one ask to commit");
}

#[test]
fn parse_verdict_takes_the_last_matching_line_and_ignores_the_echoed_instruction() {
    let tag = "VERDICT-ab12";
    let text = "❯ Finish your reply with one line that starts with VERDICT-ab12: and then APPROVE or CHANGES.\n\
                ⏺ Two problems.\nVERDICT-ab12: CHANGES\n- no input check\n- no test\n";
    let (v, findings) = parse_verdict(text, tag);
    assert_eq!(v.map(|v| v.as_str()), Some("changes"));
    assert_eq!(findings, "- no input check\n- no test");
    assert!(parse_verdict("nothing here", tag).0.is_none());
    // a verdict from an earlier round (different tag) must not count
    assert!(parse_verdict("VERDICT-0000: APPROVE\n", tag).0.is_none());
    let (v, _) = parse_verdict("VERDICT-ab12: CHANGES\nVERDICT-ab12: APPROVE\n", tag);
    assert_eq!(v.map(|v| v.as_str()), Some("approve"));
}
