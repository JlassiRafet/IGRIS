#![cfg(unix)] // these tests drive a fake Herdr shell script
#[path = "../../igris-core/tests/common/mod.rs"]
mod common;
use common::FakeHerdr;
use serde_json::{Value, json};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn tmp(name: &str) -> PathBuf {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let d = std::env::temp_dir().join(format!("igris-cli-{}-{n}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn git(dir: &Path, args: &[&str]) -> String {
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
    String::from_utf8_lossy(&o.stdout).into_owned()
}

struct Env {
    repo: PathBuf,
    home: PathBuf,
    bin: PathBuf,
    fake: FakeHerdr,
}

fn env() -> Env {
    let root = tmp("env");
    let repo = root.join("repo");
    fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    fs::write(repo.join("README.md"), "x").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "init"]);
    let bin = root.join("bin");
    fs::create_dir_all(&bin).unwrap();
    for exe in ["claude", "codex"] {
        let p = bin.join(exe);
        fs::write(&p, "#!/bin/sh\n").unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let fake = FakeHerdr::new();
    fs::write(
        repo.join("igris.toml"),
        format!(
            "[checks]\ncommand = \"true\"\n[herdr]\nbin = \"{}\"\n",
            fake.bin().display()
        ),
    )
    .unwrap();
    git(&repo, &["add", "igris.toml"]);
    git(&repo, &["commit", "-q", "-m", "config"]);
    Env {
        repo,
        home: root.join("home"),
        bin,
        fake,
    }
}

impl Env {
    fn igris(&self, args: &[&str]) -> Output {
        self.igris_with(args, &[])
    }
    fn igris_with(&self, args: &[&str], envs: &[(&str, &str)]) -> Output {
        let path = format!("{}:/usr/bin:/bin", self.bin.display());
        Command::new(env!("CARGO_BIN_EXE_igris"))
            .args(args)
            .env("IGRIS_HOME", &self.home)
            .env("PATH", path)
            .envs(envs.iter().copied())
            .output()
            .unwrap()
    }
    fn repo(&self) -> &str {
        self.repo.to_str().unwrap()
    }
}

fn json_of(o: &Output) -> Value {
    let out = String::from_utf8_lossy(&o.stdout);
    assert_eq!(
        out.trim_end().lines().count(),
        1,
        "stdout must be one JSON line: {out:?}"
    );
    serde_json::from_str(out.trim()).unwrap()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

#[test]
fn doctor_passes_when_everything_is_in_place() {
    let e = env();
    e.fake.on("agent-list", "agent-list.json");
    let o = e.igris(&["doctor", "--repo", e.repo()]);
    let v = json_of(&o);
    assert_eq!(o.status.code(), Some(0), "{v} {}", stderr(&o));
    assert_eq!(v["ok"], true);
    let ids: Vec<&str> = v["checks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        [
            "config", "herdr", "builder", "reviewer", "repo", "clean", "base"
        ]
    );
    assert!(
        v["checks"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c["ok"] == true)
    );
}

#[test]
fn doctor_json_adds_the_detection_snapshot_and_never_prints_the_key() {
    let e = env();
    e.fake.on("agent-list", "agent-list.json");
    let home = e.home.to_str().unwrap();
    let o = e.igris_with(
        &["doctor", "--json", "--repo", e.repo()],
        &[("TYPESAFE_API_KEY", "jev-secret-123"), ("HOME", home)],
    );
    let v = json_of(&o);
    assert_eq!(o.status.code(), Some(0), "{v} {}", stderr(&o));
    assert_eq!(v["ok"], true);
    assert_eq!(v["checks"].as_array().unwrap().len(), 7, "the checks stay");
    for key in [
        "machine",
        "herdr",
        "agents",
        "hermes",
        "decision",
        "local_llm",
    ] {
        assert!(v.get(key).is_some(), "missing {key}: {v}");
    }
    assert!(v["machine"]["os"].is_string());
    assert_eq!(v["agents"].as_array().unwrap().len(), 24);
    let claude = v["agents"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["kind"] == "claude")
        .unwrap();
    assert_eq!(
        (&claude["found"], &claude["login"]),
        (&json!(true), &json!("unknown"))
    );
    assert_eq!(v["local_llm"].as_array().unwrap().len(), 6);
    assert_eq!(v["decision"]["jev"]["key"], true);
    assert!(!String::from_utf8_lossy(&o.stdout).contains("jev-secret-123"));
}

#[test]
fn doctor_json_still_reports_the_machine_when_there_is_no_config() {
    let e = env();
    fs::remove_file(e.repo.join("igris.toml")).unwrap();
    let o = e.igris_with(
        &["doctor", "--json", "--repo", e.repo()],
        &[("HOME", e.home.to_str().unwrap())],
    );
    let v = json_of(&o);
    assert_eq!(o.status.code(), Some(1), "the config check fails");
    assert_eq!(v["ok"], false);
    assert!(v["machine"]["os"].is_string(), "{v}");
    assert_eq!(v["agents"].as_array().unwrap().len(), 24);
}

#[test]
fn doctor_without_json_has_no_snapshot() {
    let e = env();
    e.fake.on("agent-list", "agent-list.json");
    let v = json_of(&e.igris(&["doctor", "--repo", e.repo()]));
    assert!(v.get("machine").is_none() && v.get("agents").is_none());
}

#[test]
fn doctor_names_each_failing_check_and_its_fix() {
    let e = env();
    e.fake
        .fail_raw("agent-list", "failed to connect to herdr socket\n");
    fs::remove_file(e.bin.join("codex")).unwrap();
    fs::write(e.repo.join("scratch.txt"), "x").unwrap();
    let o = e.igris(&["doctor", "--repo", e.repo()]);
    assert_eq!(o.status.code(), Some(1));
    let v = json_of(&o);
    assert_eq!(v["ok"], false);
    let failed: Vec<(&str, &str)> = v["checks"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["ok"] == false)
        .map(|c| (c["id"].as_str().unwrap(), c["fix"].as_str().unwrap()))
        .collect();
    let ids: Vec<&str> = failed.iter().map(|f| f.0).collect();
    assert_eq!(ids, ["herdr", "reviewer", "clean"]);
    assert!(failed[0].1.contains("herdr status"), "{failed:?}");
    assert!(failed[1].1.contains("codex"), "{failed:?}");
    assert!(stderr(&o).contains("fail"), "{}", stderr(&o));
}

#[test]
fn doctor_without_a_config_says_where_to_put_one() {
    let e = env();
    fs::remove_file(e.repo.join("igris.toml")).unwrap();
    let o = e.igris(&["doctor", "--repo", e.repo()]);
    let v = json_of(&o);
    assert_eq!(v["checks"][0]["id"], "config");
    assert_eq!(v["checks"][0]["ok"], false);
    assert!(
        v["checks"][0]["fix"]
            .as_str()
            .unwrap()
            .contains("igris.example.toml")
    );
}

#[test]
fn run_refuses_a_repo_with_uncommitted_changes() {
    let e = env();
    fs::write(e.repo.join("scratch.txt"), "x").unwrap();
    let o = e.igris(&["run", "--repo", e.repo(), "--task", "add a login page"]);
    assert_eq!(o.status.code(), Some(1));
    assert_eq!(json_of(&o)["status"], "error");
    assert!(
        json_of(&o)["error"]
            .as_str()
            .unwrap()
            .contains("uncommitted"),
        "{}",
        json_of(&o)
    );
    assert!(e.fake.calls().is_empty());
}

#[test]
fn run_refuses_a_folder_that_is_not_a_git_repo() {
    let e = env();
    let plain = tmp("plain");
    let o = e.igris(&[
        "run",
        "--repo",
        plain.to_str().unwrap(),
        "--task",
        "x",
        "--config",
        e.repo.join("igris.toml").to_str().unwrap(),
    ]);
    assert_eq!(o.status.code(), Some(1));
    assert!(
        json_of(&o)["error"]
            .as_str()
            .unwrap()
            .contains("not a git repository")
    );
}

#[test]
fn a_risky_run_stops_for_approval_with_exit_3_and_calls_nothing() {
    let e = env();
    let o = e.igris(&["run", "--repo", e.repo(), "--task", "run the db migration"]);
    assert_eq!(o.status.code(), Some(3));
    let v = json_of(&o);
    assert_eq!(v["status"], "needs_approval");
    assert!(v["next"].as_str().unwrap().contains("--approved"));
    assert!(e.fake.calls().is_empty());
    assert!(stderr(&o).contains("needs_approval"), "{}", stderr(&o));
}

#[test]
fn status_lists_every_task_and_shows_one_in_full() {
    let e = env();
    let id =
        json_of(&e.igris(&["run", "--repo", e.repo(), "--task", "run the db migration"]))["id"]
            .as_str()
            .unwrap()
            .to_string();
    let all = json_of(&e.igris(&["status"]));
    assert_eq!(all[0]["id"], id.as_str());
    assert_eq!(all[0]["status"], "needs_approval");
    let one = json_of(&e.igris(&["status", &id]));
    assert_eq!(one["task"], "run the db migration");
    let missing = e.igris(&["status", "t0-dead"]);
    assert_eq!(missing.status.code(), Some(1));
    assert!(
        json_of(&missing)["error"]
            .as_str()
            .unwrap()
            .contains("no task")
    );
    let sneaky = e.igris(&["status", "../../etc/passwd"]);
    assert_eq!(sneaky.status.code(), Some(1));
}

/// A finished-looking task whose branch carries one commit, built the way the loop leaves it.
fn finished_task(e: &Env) -> String {
    let id =
        json_of(&e.igris(&["run", "--repo", e.repo(), "--task", "run the db migration"]))["id"]
            .as_str()
            .unwrap()
            .to_string();
    let wt = e.repo.parent().unwrap().join("wt");
    let branch = format!("igris/{id}");
    git(
        &e.repo,
        &["worktree", "add", "-q", "-b", &branch, wt.to_str().unwrap()],
    );
    fs::write(wt.join("a.txt"), "change").unwrap();
    git(&wt, &["add", "."]);
    git(&wt, &["commit", "-q", "-m", "work"]);
    edit_report(e, &id, |r| {
        r["status"] = json!("done");
        r["stage"] = json!("report");
        r["branch"] = json!(branch);
        r["workspace"] = json!("w2");
        r["worktree"] = json!(wt);
        r["verdict"] = json!("approve");
        r["checks"]["exit"] = json!(0);
    });
    id
}

fn edit_report(e: &Env, id: &str, f: impl FnOnce(&mut Value)) {
    let p = e.home.join("tasks").join(format!("{id}.json"));
    let mut r: Value = serde_json::from_str(&fs::read_to_string(&p).unwrap()).unwrap();
    f(&mut r);
    fs::write(p, serde_json::to_string(&r).unwrap()).unwrap();
}

#[test]
fn merge_refuses_a_task_that_is_not_approved_and_green() {
    let e = env();
    let id = finished_task(&e);
    for (verdict, exit) in [("changes", 0), ("approve", 1)] {
        edit_report(&e, &id, |r| {
            r["verdict"] = json!(verdict);
            r["checks"]["exit"] = json!(exit);
        });
        let o = e.igris(&["merge", &id]);
        assert_eq!(o.status.code(), Some(1), "{verdict}/{exit}");
        assert!(
            json_of(&o)["error"]
                .as_str()
                .unwrap()
                .contains("not mergeable"),
            "{}",
            json_of(&o)
        );
    }
    assert!(!e.repo.join("a.txt").exists(), "nothing merged");
}

#[test]
fn merge_refuses_when_the_repo_is_not_on_the_base_branch() {
    let e = env();
    let id = finished_task(&e);
    git(&e.repo, &["switch", "-q", "-c", "other"]);
    let o = e.igris(&["merge", &id]);
    assert_eq!(o.status.code(), Some(1));
    assert!(
        json_of(&o)["error"]
            .as_str()
            .unwrap()
            .contains("switch to main"),
        "{}",
        json_of(&o)
    );
}

#[test]
fn merge_fast_forwards_cleans_up_and_logs_the_outcome() {
    let e = env();
    e.fake.on("worktree-remove", "worktree-remove.json");
    let id = finished_task(&e);
    let branch = format!("igris/{id}");
    // The fake cannot delete the worktree directory, so do what `herdr worktree remove` does.
    let wt = e.repo.parent().unwrap().join("wt");
    let o = {
        git(
            &e.repo,
            &["worktree", "remove", "--force", wt.to_str().unwrap()],
        );
        e.igris(&["merge", &id])
    };
    assert_eq!(o.status.code(), Some(0), "{} {}", json_of(&o), stderr(&o));
    let v = json_of(&o);
    assert_eq!(
        (v["status"].as_str(), v["branch"].as_str()),
        (Some("merged"), Some(branch.as_str()))
    );
    assert!(e.repo.join("a.txt").exists(), "the work is on main");
    assert!(
        git(&e.repo, &["branch", "--list", "igris/*"])
            .trim()
            .is_empty(),
        "task branch deleted"
    );
    assert!(
        e.fake
            .calls()
            .iter()
            .any(|c| c == &["worktree", "remove", "--workspace", "w2"])
    );
    let log = fs::read_to_string(e.home.join("decisions.jsonl")).unwrap();
    let last: Value = serde_json::from_str(log.lines().last().unwrap()).unwrap();
    assert_eq!(
        (last["kind"].as_str(), last["outcome"]["merged"].as_bool()),
        (Some("outcome"), Some(true))
    );
    assert_eq!(json_of(&e.igris(&["status", &id]))["status"], "merged");
}

#[test]
fn merge_names_the_files_that_block_the_worktree_cleanup() {
    let e = env();
    e.fake.fail_raw(
        "worktree-remove",
        r#"{"error":{"code":"dirty_worktree_requires_force","message":"worktree contains modified or untracked files"}}"#,
    );
    let id = finished_task(&e);
    let wt = e.repo.parent().unwrap().join("wt");
    fs::write(wt.join("scratch.tmp"), "left behind by a test run").unwrap();
    let o = e.igris(&["merge", &id]);
    assert_eq!(o.status.code(), Some(0), "{} {}", json_of(&o), stderr(&o));
    let v = json_of(&o);
    assert_eq!(v["status"], "merged", "the work still lands");
    let cleanup = v["cleanup"].as_str().unwrap();
    assert!(cleanup.contains("scratch.tmp"), "names the file: {cleanup}");
    assert!(
        cleanup.contains("add them to .gitignore"),
        "says what to do: {cleanup}"
    );
}

#[test]
fn continue_on_an_unknown_task_fails_clearly() {
    let e = env();
    let o = e.igris(&["continue", "t0-dead"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(json_of(&o)["error"].as_str().unwrap().contains("no task"));
}

#[test]
fn the_smoke_script_is_valid_sh_and_names_the_trust_step() {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/smoke.sh");
    let text = fs::read_to_string(&script).expect("scripts/smoke.sh exists");
    let out = Command::new("sh").arg("-n").arg(&script).output().unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(
        text.contains("igris doctor") && text.contains("igris run"),
        "runs the real commands"
    );
    assert!(
        text.contains("trust"),
        "tells the user about Claude Code's folder-trust prompt"
    );
}

#[test]
fn setup_command_is_gone() {
    let o = Command::new(env!("CARGO_BIN_EXE_igris"))
        .arg("setup")
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(2), "{}", stderr(&o));
}

#[test]
fn doctor_does_not_promise_that_parent_folder_trust_works() {
    let e = env();
    fs::remove_file(e.bin.join("codex")).unwrap();
    e.fake.on("agent-list", "agent-list.json");
    // A PATH with the fake agents and git only, so a codex installed on this machine cannot count.
    let tools = tmp("tools");
    let git_bin = std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|d| d.join("git"))
        .find(|p| p.exists())
        .unwrap();
    std::os::unix::fs::symlink(git_bin, tools.join("git")).unwrap();
    let path = format!("{}:{}", e.bin.display(), tools.display());
    let v = json_of(&e.igris_with(&["doctor", "--repo", e.repo()], &[("PATH", &path)]));
    let c = v["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == "reviewer")
        .unwrap();
    assert_eq!(c["ok"], false, "{v}");
    let fix = c["fix"].as_str().unwrap();
    assert!(!fix.contains("parent folder"), "{fix}");
    assert!(fix.contains("install codex"), "{fix}");
}

#[test]
fn continue_yes_on_an_unknown_task_fails_clearly() {
    let e = env();
    let o = e.igris(&["continue", "t0-dead", "--yes"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(json_of(&o)["error"].as_str().unwrap().contains("no task"));
}

#[test]
fn continue_yes_refuses_with_exit_3_and_presses_nothing_when_herdr_cannot_say_who_waits() {
    let e = env();
    let id =
        json_of(&e.igris(&["run", "--repo", e.repo(), "--task", "run the db migration"]))["id"]
            .as_str()
            .unwrap()
            .to_string();
    edit_report(&e, &id, |r| {
        r["status"] = json!("blocked");
        r["stage"] = json!("build");
        r["waiting"] = json!("builder");
        r["panes"]["builder"] = json!("w2:p1");
    });
    let o = e.igris(&["continue", &id, "--yes"]);
    let v = json_of(&o);
    assert_eq!(o.status.code(), Some(3), "{v} {}", stderr(&o));
    assert_eq!(v["status"], "blocked");
    assert!(
        v["reason"].as_str().unwrap().contains("cannot ask Herdr"),
        "{v}"
    );
    assert!(!e.fake.calls().iter().any(|c| c[1] == "send-keys"));
}
