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
        self.orch_with(cfg, None)
    }
    fn orch_with<'a>(&self, cfg: &'a Config, config_path: Option<PathBuf>) -> Orchestrator<'a> {
        Orchestrator {
            herdr: Herdr {
                bin: self.fake.bin(),
                machine: None,
            },
            cfg,
            home: self.home.clone(),
            config_path,
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

fn blocked_run(w: &World, c: &Config, config: Option<&str>) -> igris_core::orchestrate::Report {
    w.fake.fail("agent-start", "error-agent-not-ready.txt");
    w.fake.on_raw("agent-read", UNKNOWN_DIALOG); // not a recorded dialog: the plain next line
    w.orch_with(c, config.map(PathBuf::from))
        .run(&w.repo, "add a login page", false)
        .unwrap()
}

#[test]
fn blocked_next_line_repeats_the_config_path() {
    let w = world(true);
    let r = blocked_run(&w, &cfg(), Some("/cfg/solo.toml"));
    assert_eq!(r.status, Status::Blocked);
    assert!(
        r.next.ends_with(&format!(
            "`igris continue {} --config '/cfg/solo.toml'`",
            r.id
        )),
        "{}",
        r.next
    );
}

#[test]
fn the_yes_sentence_repeats_the_config_path_before_the_flag() {
    let w = world(true);
    w.fake.fail("agent-start", "error-agent-not-ready.txt");
    w.fake.on("agent-read", "agent-read-blocked.txt");
    let r = w
        .orch_with(&cfg(), Some(PathBuf::from("/cfg/solo.toml")))
        .run(&w.repo, "add a login page", false)
        .unwrap();
    assert!(
        r.next.ends_with(&format!(
            "igris continue {} --config '/cfg/solo.toml' --yes`",
            r.id
        )),
        "{}",
        r.next
    );
}

#[test]
fn a_run_without_a_config_prints_the_plain_continue_command() {
    let w = world(true);
    let r = blocked_run(&w, &cfg(), None);
    assert!(
        r.next.ends_with(&format!("`igris continue {}`", r.id)),
        "{}",
        r.next
    );
}

#[test]
fn needs_approval_next_line_repeats_the_config_path() {
    let w = world(true);
    let c = cfg();
    let r = w
        .orch_with(&c, Some(PathBuf::from("/cfg/solo.toml")))
        .run(&w.repo, "run the db migration", false)
        .unwrap();
    assert_eq!(r.status, Status::NeedsApproval);
    assert!(r.next.contains("--approved"), "{}", r.next);
    assert!(r.next.contains("--config '/cfg/solo.toml'"), "{}", r.next);
}

#[test]
fn a_config_path_with_a_space_or_a_quote_is_quoted_for_the_shell() {
    let w = world(true);
    let r = blocked_run(&w, &cfg(), Some("/my dir/it's.toml"));
    assert!(
        r.next.contains(r"--config '/my dir/it'\''s.toml'"),
        "{}",
        r.next
    );
}

#[test]
fn a_report_saved_by_rc2_still_loads() {
    let w = world(true);
    let r = blocked_run(&w, &cfg(), Some("/cfg/solo.toml"));
    let mut v = serde_json::to_value(&r).unwrap();
    for k in ["config", "waiting", "reviewed_by"] {
        v.as_object_mut().unwrap().remove(k);
    }
    let old: igris_core::orchestrate::Report = serde_json::from_value(v).unwrap();
    assert_eq!(old.id, r.id);
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
    assert!(r.reason.contains("?? b.txt"), "{}", r.reason);
    let nudge = w
        .fake
        .calls()
        .into_iter()
        .filter(|c| c[1] == "prompt" && c[2].starts_with("ib-"))
        .nth(1)
        .unwrap();
    assert!(nudge[3].contains("?? b.txt"), "{}", nudge[3]);
    assert!(nudge[3].contains(".gitignore"), "{}", nudge[3]);
}

#[test]
fn dirty_files_names_a_file_with_a_space_and_non_ascii_characters() {
    let w = world(true);
    fs::write(w.wt.join("my file é.txt"), "x").unwrap();
    fs::write(w.wt.join("a.txt"), "changed").unwrap();
    let files = igris_core::git::dirty_files(&w.wt).unwrap();
    assert_eq!(files, [" M a.txt", "?? my file é.txt"]);
}

#[test]
fn more_than_ten_leftover_files_are_summarised() {
    let w = world(true);
    for i in 1..=12 {
        fs::write(w.wt.join(format!("f{i:02}.txt")), "x").unwrap();
    }
    let r = w
        .orch(&cfg())
        .run(&w.repo, "add a login page", false)
        .unwrap();
    assert_eq!(r.status, Status::Failed);
    assert!(r.reason.contains("?? f01.txt"), "{}", r.reason);
    assert!(r.reason.contains("?? f10.txt"), "{}", r.reason);
    assert!(!r.reason.contains("f11.txt"), "{}", r.reason);
    assert!(r.reason.ends_with("and 2 more"), "{}", r.reason);
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

#[test]
fn a_codex_update_prompt_is_reported_as_codex_with_the_screen() {
    let w = world(true);
    w.fake.queue_ok("agent-start", "agent-start-ok.json"); // the builder starts
    w.fake
        .queue_fail("agent-start", "error-agent-not-ready.txt"); // the reviewer is blocked
    w.fake.on("agent-get", "agent-get-codex-blocked.json");
    w.fake.on("agent-read", "codex-update.txt");
    let r = w
        .orch(&cfg())
        .run(&w.repo, "add a login page", false)
        .unwrap();
    assert_eq!((r.status, r.stage), (Status::Blocked, Stage::Review));
    assert!(r.reason.contains("reviewer (codex)"), "{}", r.reason);
    assert!(r.reason.contains("Update now"), "{}", r.reason);
    assert!(!r.reason.contains("Claude Code"), "{}", r.reason);
    assert!(
        r.next.contains("key presses in the Herdr pane"),
        "{}",
        r.next
    );
    assert!(
        r.next.contains(&format!("igris continue {}", r.id)),
        "{}",
        r.next
    );
}

#[test]
fn a_claude_trust_prompt_names_claude_and_quotes_the_dialog() {
    let w = world(true);
    w.fake.fail("agent-start", "error-agent-not-ready.txt");
    w.fake.on("agent-get", "agent-get-blocked.json");
    w.fake.on("agent-read", "agent-read-blocked.txt");
    let r = w
        .orch(&cfg())
        .run(&w.repo, "add a login page", false)
        .unwrap();
    assert_eq!(r.status, Status::Blocked);
    assert!(r.reason.contains("builder (claude)"), "{}", r.reason);
    assert!(r.reason.contains("trust"), "{}", r.reason);
}

#[test]
fn the_configured_agent_is_named_when_herdr_cannot_say() {
    let w = world(true);
    w.fake.fail("agent-start", "error-agent-not-ready.txt");
    w.fake.on("agent-read", "agent-read-blocked.txt"); // no agent-get fixture: the call fails
    let r = w
        .orch(&cfg())
        .run(&w.repo, "add a login page", false)
        .unwrap();
    assert_eq!(r.status, Status::Blocked);
    assert!(r.reason.contains("builder (claude)"), "{}", r.reason);
}

#[test]
fn the_reason_never_holds_escape_sequences_or_long_lines() {
    let screen = format!(
        "\x1b[31mred\x1b[0m before\n{}\nline a\nline b\nline c\nline d\nline e\nline f\n",
        "x".repeat(400)
    );
    let out = igris_core::orchestrate::sanitize(&screen);
    assert!(!out.contains('\x1b') && !out.contains("[31m"), "{out:?}");
    assert!(out.split(" / ").count() <= 5, "{out:?}");
    assert!(
        out.split(" / ").all(|l| l.chars().count() <= 160),
        "{out:?}"
    );
    assert!(out.ends_with("line f"), "{out:?}");
    assert_eq!(igris_core::orchestrate::sanitize("\n  \n"), "");
}

#[test]
fn an_escape_sequence_left_open_at_a_line_end_hides_the_next_line_too() {
    let out = igris_core::orchestrate::sanitize("\x1b]0;hidden\nstill hidden\x07\nUpdate now");
    assert_eq!(out, "Update now");
}

#[test]
fn a_solo_run_says_the_same_agent_reviewed() {
    let w = world(true);
    w.fake.queue_raw_ok("agent-read", APPROVE);
    let mut c = cfg();
    c.agents.builder = "claude".into();
    c.agents.reviewer = "claude".into();
    let r = w.orch(&c).run(&w.repo, "add a login page", false).unwrap();
    assert_eq!(r.status, Status::Done);
    assert_eq!(
        r.reviewed_by.as_deref(),
        Some("claude (same agent, new session)")
    );
}

#[test]
fn two_agents_name_the_reviewer_only_and_the_report_json_carries_it() {
    let w = world(true);
    w.fake.queue_raw_ok("agent-read", APPROVE);
    let r = w
        .orch(&cfg())
        .run(&w.repo, "add a login page", false)
        .unwrap();
    assert_eq!(r.reviewed_by.as_deref(), Some("codex"));
    assert_eq!(serde_json::to_value(&r).unwrap()["reviewed_by"], "codex");
}

#[test]
fn a_resume_with_a_changed_config_keeps_the_reviewer_that_started() {
    let w = world(true);
    w.fake.queue_ok("agent-start", "agent-start-ok.json"); // the builder
    w.fake
        .queue_fail("agent-start", "error-agent-not-ready.txt"); // the codex reviewer is blocked
    w.fake.on("agent-read", "agent-read-blocked.txt");
    let started = cfg(); // builder claude, reviewer codex
    let r = w
        .orch(&started)
        .run(&w.repo, "add a login page", false)
        .unwrap();
    assert_eq!((r.status, r.stage), (Status::Blocked, Stage::Review));
    // The config now names another reviewer; the pane that was started is still codex's.
    let mut changed = cfg();
    changed.agents.builder = "claude".into();
    changed.agents.reviewer = "claude".into();
    w.fake.on("agent-start", "agent-start-ok.json");
    w.fake.on_raw("agent-read", APPROVE);
    let r = w.orch(&changed).resume(&r.id).unwrap();
    assert_eq!(r.status, Status::Done);
    assert_eq!(r.reviewed_by.as_deref(), Some("codex"));
}

// ---- `continue --yes`: answering a recorded folder-trust dialog ----

const KEYS_OK: &str = r#"{"id":"cli:agent:send-keys","result":{"type":"ok"}}"#;
const UNKNOWN_DIALOG: &str = "Do you want to run `rm -rf build`?\n❯ 1. Yes\n  2. No\n";

fn fixture(name: &str) -> String {
    fs::read_to_string(format!("{FIXTURES}/{name}")).unwrap()
}

/// Herdr's answer for `agent get`; `same_folder` puts the pane in the task's own worktree.
fn agent_get(w: &World, name: &str, same_folder: bool) {
    let text = fixture(name);
    let text = if same_folder {
        text.replace(
            "/home/user/.herdr/worktrees/repo/igris-fx1",
            w.wt.to_str().unwrap(),
        )
    } else {
        text
    };
    w.fake.on_raw("agent-get", &text);
}

fn keys_sent(w: &World) -> Vec<String> {
    w.fake
        .calls()
        .into_iter()
        .filter(|c| c[1] == "send-keys")
        .map(|c| c[3].clone())
        .collect()
}

/// The builder (Claude) stops at startup on the given screen.
fn builder_blocked(w: &World, screen: &str, same_folder: bool) -> igris_core::orchestrate::Report {
    w.fake.fail("agent-start", "error-agent-not-ready.txt");
    w.fake.on_raw("agent-read", screen);
    agent_get(w, "agent-get-blocked.json", same_folder);
    let r = w
        .orch(&cfg())
        .run(&w.repo, "add a login page", false)
        .unwrap();
    assert_eq!(
        (r.status, r.waiting.as_deref()),
        (Status::Blocked, Some("builder"))
    );
    r
}

/// The reviewer (Codex) stops at startup on the given screen.
fn reviewer_blocked(w: &World, screen: &str) -> igris_core::orchestrate::Report {
    w.fake.queue_ok("agent-start", "agent-start-ok.json");
    w.fake
        .queue_fail("agent-start", "error-agent-not-ready.txt");
    w.fake.on_raw("agent-read", screen);
    agent_get(w, "agent-get-codex-blocked.json", true);
    let r = w
        .orch(&cfg())
        .run(&w.repo, "add a login page", false)
        .unwrap();
    assert_eq!(
        (r.status, r.waiting.as_deref()),
        (Status::Blocked, Some("reviewer"))
    );
    r
}

fn dialog_then_ready(w: &World, dialog: &str) {
    w.fake.on("agent-start", "agent-start-ok.json");
    w.fake.on_raw("agent-send-keys", KEYS_OK);
    w.fake.queue_raw_ok("agent-read", dialog); // the screen before the keys
    w.fake.on_raw("agent-read", APPROVE); // after the keys, and the reviewer's answer
}

#[test]
fn a_matched_dialog_gets_a_sentence_built_from_fixed_fields() {
    let w = world(true);
    let r = builder_blocked(&w, &fixture("agent-read-blocked.txt"), true);
    assert!(r.next.contains("Claude Code asks to trust"), "{}", r.next);
    assert!(r.next.contains(w.wt.to_str().unwrap()), "{}", r.next);
    assert!(
        r.next.contains(&format!("igris continue {} --yes", r.id)),
        "{}",
        r.next
    );
}

#[test]
fn yes_presses_down_then_enter_for_a_claude_trust_dialog() {
    let w = world(true);
    let claude = fixture("agent-read-blocked.txt");
    let r = builder_blocked(&w, &claude, true);
    dialog_then_ready(&w, &claude);
    let r = w.orch(&cfg()).resume_with(&r.id, true).unwrap();
    assert_eq!(r.status, Status::Done, "{}", r.reason);
    assert_eq!(keys_sent(&w), ["down", "enter"]);
}

#[test]
fn yes_presses_enter_for_codex_and_the_sentence_names_the_repository_root() {
    let w = world(true);
    let codex = fixture("codex-trust-root.txt");
    let r = reviewer_blocked(&w, &codex);
    assert!(
        r.next.contains("Codex asks to trust the repository root"),
        "{}",
        r.next
    );
    assert!(r.next.contains(w.repo.to_str().unwrap()), "{}", r.next);
    dialog_then_ready(&w, &codex);
    let r = w.orch(&cfg()).resume_with(&r.id, true).unwrap();
    assert_eq!(r.status, Status::Done, "{}", r.reason);
    assert_eq!(keys_sent(&w), ["enter"]);
}

#[test]
fn yes_refuses_on_an_update_prompt_and_sends_nothing() {
    let w = world(true);
    let r = reviewer_blocked(&w, &fixture("codex-update.txt"));
    w.fake.on_raw("agent-send-keys", KEYS_OK);
    let r = w.orch(&cfg()).resume_with(&r.id, true).unwrap();
    assert_eq!(r.status, Status::Blocked);
    assert!(
        r.reason.contains("not a recorded folder-trust dialog"),
        "{}",
        r.reason
    );
    assert!(r.reason.contains("Update now"), "{}", r.reason);
    assert!(keys_sent(&w).is_empty());
}

#[test]
fn yes_refuses_on_an_unknown_dialog() {
    let w = world(true);
    let r = builder_blocked(&w, UNKNOWN_DIALOG, true);
    w.fake.on_raw("agent-send-keys", KEYS_OK);
    let r = w.orch(&cfg()).resume_with(&r.id, true).unwrap();
    assert_eq!(r.status, Status::Blocked);
    assert!(
        r.reason.contains("not a recorded folder-trust dialog"),
        "{}",
        r.reason
    );
    assert!(keys_sent(&w).is_empty());
}

#[test]
fn yes_refuses_when_the_pane_is_not_the_tasks_worktree() {
    let w = world(true);
    let claude = fixture("agent-read-blocked.txt");
    let r = builder_blocked(&w, &claude, false); // Herdr says another folder
    w.fake.on_raw("agent-send-keys", KEYS_OK);
    let r = w.orch(&cfg()).resume_with(&r.id, true).unwrap();
    assert_eq!(r.status, Status::Blocked);
    assert!(r.reason.contains("not the task's worktree"), "{}", r.reason);
    assert!(keys_sent(&w).is_empty());
}

#[test]
fn yes_refuses_when_herdr_cannot_say_which_agent_is_in_the_pane() {
    let w = world(true);
    let claude = fixture("agent-read-blocked.txt");
    let r = builder_blocked(&w, &claude, true);
    w.fake.fail_raw(
        "agent-get",
        "{\"error\":{\"code\":\"agent_not_found\",\"message\":\"gone\"}}",
    );
    w.fake.on_raw("agent-send-keys", KEYS_OK);
    let r = w.orch(&cfg()).resume_with(&r.id, true).unwrap();
    assert_eq!(r.status, Status::Blocked);
    assert!(r.reason.contains("cannot ask Herdr"), "{}", r.reason);
    assert!(keys_sent(&w).is_empty());
}

#[test]
fn yes_refuses_when_the_task_is_not_blocked() {
    let w = world(true);
    w.fake.queue_raw_ok("agent-read", APPROVE);
    let r = w
        .orch(&cfg())
        .run(&w.repo, "add a login page", false)
        .unwrap();
    assert_eq!(r.status, Status::Done);
    w.fake.on_raw("agent-send-keys", KEYS_OK);
    let e = w.orch(&cfg()).resume_with(&r.id, true).unwrap_err();
    assert!(e.to_string().contains("is not blocked"), "{e}");
    assert!(keys_sent(&w).is_empty());
}

#[test]
fn yes_reports_a_dialog_that_is_still_on_screen_after_the_keys_and_does_not_retry() {
    let w = world(true);
    let claude = fixture("agent-read-blocked.txt");
    let r = builder_blocked(&w, &claude, true);
    w.fake.on("agent-start", "agent-start-ok.json");
    w.fake.on_raw("agent-send-keys", KEYS_OK);
    w.fake.on_raw("agent-read", &claude); // the dialog never goes away
    let r = w.orch(&cfg()).resume_with(&r.id, true).unwrap();
    assert_eq!(r.status, Status::Blocked);
    assert!(r.reason.contains("still on screen"), "{}", r.reason);
    assert_eq!(keys_sent(&w), ["down", "enter"], "one attempt, no retry");
}

#[test]
fn yes_gives_the_agent_time_to_settle_before_the_task_prompt_is_sent() {
    // Live probe, 2026-11-27: Claude Code typed the task into its input but lost the Enter when the
    // prompt came right after the trust keys.
    let w = world(true);
    let claude = fixture("agent-read-blocked.txt");
    let r = builder_blocked(&w, &claude, true);
    dialog_then_ready(&w, &claude);
    let t = std::time::Instant::now();
    let r = w.orch(&cfg()).resume_with(&r.id, true).unwrap();
    assert_eq!(r.status, Status::Done, "{}", r.reason);
    assert!(
        t.elapsed() >= std::time::Duration::from_millis(2500),
        "only {:?} between the keys and the next prompt",
        t.elapsed()
    );
}
