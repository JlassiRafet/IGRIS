mod common;
use common::FakeHerdr;
use igris_core::herdr::{AgentState, Herdr, sh_quote};

fn herdr(fake: &FakeHerdr) -> Herdr {
    Herdr {
        bin: fake.bin(),
        machine: None,
    }
}

#[test]
fn call_returns_result_object() {
    let fake = FakeHerdr::new();
    fake.on("worktree-create", "worktree-create.json");
    let v = herdr(&fake).call(&["worktree", "create"]).unwrap();
    assert_eq!(v["workspace"]["workspace_id"], "w2");
}

#[test]
fn worktree_create_returns_ids_and_path() {
    let fake = FakeHerdr::new();
    fake.on("worktree-create", "worktree-create.json");
    let wt = herdr(&fake)
        .worktree_create("/work/repo".as_ref(), "igris/t1", "main")
        .unwrap();
    assert_eq!(
        (wt.workspace_id.as_str(), wt.root_pane.as_str()),
        ("w2", "w2:p1")
    );
    assert!(wt.path.ends_with("igris-fx1"));
}

#[test]
fn machine_flag_prefixes_every_call() {
    let fake = FakeHerdr::new();
    fake.on("pane-split", "pane-split.json");
    let h = Herdr {
        bin: fake.bin(),
        machine: Some("box1".into()),
    };
    assert_eq!(
        h.split("w2:p1", "/work/repo".as_ref(), "right").unwrap(),
        "w2:p2"
    );
    assert_eq!(fake.calls()[0][..4], ["--machine", "box1", "pane", "split"]);
}

#[test]
fn error_json_on_stderr_maps_code_and_hint() {
    let fake = FakeHerdr::new();
    fake.fail("agent-prompt", "error-agent-blocked.txt");
    let e = herdr(&fake).prompt("builder", "hi", 1000).unwrap_err();
    assert_eq!(e.code, "agent_blocked");
    assert!(e.hint.contains("resolve it in Herdr"), "{}", e.hint);
}

#[test]
fn folder_trust_block_at_start_is_agent_not_ready() {
    let fake = FakeHerdr::new();
    fake.fail("agent-start", "error-agent-not-ready.txt");
    let e = herdr(&fake)
        .start_agent("builder", "claude", "w2:p1", &[])
        .unwrap_err();
    assert_eq!(e.code, "agent_not_ready");
    assert!(e.hint.contains("trust"), "{}", e.hint);
}

#[test]
fn server_down_gives_hint() {
    let fake = FakeHerdr::new();
    fake.fail_raw("pane-split", "failed to connect to herdr socket\n");
    let e = herdr(&fake)
        .split("w2:p1", "/work/repo".as_ref(), "right")
        .unwrap_err();
    assert_eq!(e.code, "server_unavailable");
    assert!(e.hint.contains("herdr status"));
}

#[test]
fn missing_binary_gives_install_hint() {
    let h = Herdr {
        bin: "/nonexistent/herdr".into(),
        machine: None,
    };
    let e = h.call(&["status"]).unwrap_err();
    assert_eq!(e.code, "not_installed");
}

#[test]
fn start_and_prompt_read_state_from_the_result() {
    let fake = FakeHerdr::new();
    fake.on("agent-start", "agent-start-ok.json");
    fake.on("agent-prompt", "agent-prompt-ok.json");
    let h = herdr(&fake);
    let args = vec!["--permission-mode".to_string(), "plan".to_string()];
    assert_eq!(
        h.start_agent("builder", "claude", "w2:p1", &args).unwrap(),
        AgentState::Idle
    );
    assert_eq!(
        h.prompt("builder", "do it", 5000).unwrap(),
        AgentState::Idle
    );
    let c = fake.calls();
    assert_eq!(c[0][c[0].len() - 3..], ["--", "--permission-mode", "plan"]);
    assert_eq!(c[1][..4], ["agent", "prompt", "builder", "do it"]);
}

#[test]
fn read_returns_plain_text_not_json() {
    let fake = FakeHerdr::new();
    fake.on("agent-read", "agent-read-blocked.txt");
    let text = herdr(&fake).read("builder", 40).unwrap();
    assert!(text.contains("Yes, I trust this folder"));
}

#[test]
fn run_check_reads_the_exit_code_and_tolerates_silent_pane_run() {
    let fake = FakeHerdr::new();
    fake.on("pane-run", "pane-run.json"); // real capture: empty stdout
    fake.on_raw(
        "pane-wait-output",
        r#"{"id":"x","result":{"matched_line":"IGRIS_EXIT_ab12=3","type":"output_matched"}}"#,
    );
    fake.on("pane-read", "pane-read.txt");
    let (code, tail) = herdr(&fake).run_check("w2:p2", "npm test", 5000).unwrap();
    assert_eq!(code, Some(3));
    assert!(tail.contains("hello"));
}

#[test]
fn run_check_marker_is_unique_per_run_so_a_rerun_cannot_match_a_stale_one() {
    let fake = FakeHerdr::new();
    fake.on("pane-run", "pane-run.json");
    fake.on("pane-wait-output", "pane-wait-output.json");
    fake.on("pane-read", "pane-read.txt");
    let h = herdr(&fake);
    h.run_check("w2:p2", "true", 1000).unwrap();
    h.run_check("w2:p2", "true", 1000).unwrap();
    let regexes: Vec<String> = fake
        .calls()
        .iter()
        .filter(|c| c[1] == "wait-output")
        .map(|c| c[c.iter().position(|a| a == "--regex").unwrap() + 1].clone())
        .collect();
    assert_eq!(regexes.len(), 2);
    assert_ne!(regexes[0], regexes[1]);
    let marker = regexes[0].trim_end_matches(r"\d+");
    assert!(
        fake.calls()[0][2..].iter().any(|a| a.contains(marker)),
        "command must echo the marker"
    );
}

#[test]
fn run_check_timeout_is_none_with_the_output_tail() {
    let fake = FakeHerdr::new();
    fake.on("pane-run", "pane-run.json");
    fake.fail("pane-wait-output", "error-wait-timeout.txt");
    fake.on("pane-read", "pane-read.txt");
    let (code, tail) = herdr(&fake).run_check("w2:p2", "sleep 999", 1000).unwrap();
    assert_eq!(code, None);
    assert!(tail.contains("hello"));
}

#[test]
fn check_command_runs_under_sh_so_the_users_shell_does_not_matter() {
    let fake = FakeHerdr::new();
    fake.on("pane-run", "pane-run.json");
    fake.on("pane-wait-output", "pane-wait-output.json");
    fake.on("pane-read", "pane-read.txt");
    herdr(&fake)
        .run_check("w2:p2", "echo 'it works'", 1000)
        .unwrap();
    let run = &fake.calls()[0];
    assert!(run[3].starts_with("sh -c '"), "{}", run[3]);
}

#[test]
fn sh_quote_survives_a_real_shell() {
    for s in ["plain", "it's", "a \"b\" $HOME `x` \\ ; &&", "multi\nline"] {
        let out = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("printf %s {}", sh_quote(s)))
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout), s);
    }
}

#[test]
fn remove_worktree_only_names_the_given_workspace() {
    let fake = FakeHerdr::new();
    fake.on("worktree-remove", "worktree-remove.json");
    herdr(&fake).remove_worktree("w2").unwrap();
    assert_eq!(fake.calls()[0], ["worktree", "remove", "--workspace", "w2"]);
}
