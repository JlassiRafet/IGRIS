use igris_core::config::{Config, ConfigError, KINDS};
use std::fs;
use std::path::{Path, PathBuf};

fn tmp(name: &str) -> PathBuf {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let d = std::env::temp_dir().join(format!("igris-cfg-{}-{n}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn write(dir: &Path, file: &str, text: &str) -> PathBuf {
    let p = dir.join(file);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(&p, text).unwrap();
    p
}

fn parse(text: &str) -> Result<Config, ConfigError> {
    let d = tmp("parse");
    let p = write(&d, "igris.toml", text);
    Config::load_from(&d, Some(&p), None)
}

#[test]
fn defaults_fill_missing_keys() {
    let c = parse("[checks]\ncommand = \"npm test\"\n").unwrap();
    assert_eq!(
        (c.agents.builder.as_str(), c.agents.reviewer.as_str()),
        ("claude", "codex")
    );
    assert_eq!(c.checks.timeout_s, 900);
    assert_eq!(c.policy.base, "main");
    assert_eq!(
        (c.policy.max_review_rounds, c.policy.max_fix_rounds),
        (2, 2)
    );
    assert_eq!(c.decision.backend, "rules");
    assert!((c.decision.threshold - 0.85).abs() < f32::EPSILON);
    assert_eq!(c.herdr.bin, "herdr");
    assert_eq!(c.herdr.machine, None);
    assert!(
        c.policy
            .risky_paths
            .contains(&"**/migrations/**".to_string())
    );
}

#[test]
fn reviewer_args_default_is_read_only_for_the_chosen_agent() {
    let codex = parse("[checks]\ncommand = \"t\"\n").unwrap();
    assert_eq!(codex.agents.reviewer_args(), ["--sandbox", "read-only"]);
    let claude = parse("[agents]\nreviewer = \"claude\"\n[checks]\ncommand = \"t\"\n").unwrap();
    assert_eq!(claude.agents.reviewer_args(), ["--permission-mode", "plan"]);
    let other = parse("[agents]\nreviewer = \"gemini\"\n[checks]\ncommand = \"t\"\n").unwrap();
    assert!(other.agents.reviewer_args().is_empty());
    let set = parse("[agents]\nreviewer_args = [\"--x\"]\n[checks]\ncommand = \"t\"\n").unwrap();
    assert_eq!(set.agents.reviewer_args(), ["--x"]);
}

#[test]
fn unknown_kind_lists_the_24() {
    let e = parse("[agents]\nbuilder = \"gpt\"\n[checks]\ncommand = \"t\"\n")
        .unwrap_err()
        .to_string();
    assert!(
        e.contains("'gpt'") && e.contains("claude") && e.contains("muse"),
        "{e}"
    );
    assert_eq!(KINDS.len(), 24);
}

#[test]
fn empty_check_command_is_rejected_with_a_fix() {
    let e = parse("[checks]\ncommand = \"  \"\n")
        .unwrap_err()
        .to_string();
    assert!(e.contains("checks.command"), "{e}");
    let e = parse("[agents]\nbuilder = \"claude\"\n")
        .unwrap_err()
        .to_string();
    assert!(e.contains("checks.command"), "{e}");
}

#[test]
fn jev_and_laya_are_a_later_milestone_and_typos_are_rejected() {
    for b in ["jev", "laya"] {
        let e = parse(&format!(
            "[decision]\nbackend = \"{b}\"\n[checks]\ncommand = \"t\"\n"
        ))
        .unwrap_err()
        .to_string();
        assert!(e.contains("later milestone"), "{e}");
    }
    let e = parse("[decision]\nbackend = \"magic\"\n[checks]\ncommand = \"t\"\n")
        .unwrap_err()
        .to_string();
    assert!(e.contains("'magic'"), "{e}");
}

#[test]
fn unknown_keys_are_rejected_so_typos_do_not_pass_silently() {
    let e = parse("[agents]\nbuidler = \"claude\"\n[checks]\ncommand = \"t\"\n")
        .unwrap_err()
        .to_string();
    assert!(e.contains("buidler"), "{e}");
}

#[test]
fn threshold_must_be_between_0_and_1() {
    let e = parse("[decision]\nthreshold = 1.5\n[checks]\ncommand = \"t\"\n")
        .unwrap_err()
        .to_string();
    assert!(e.contains("threshold"), "{e}");
}

#[test]
fn lookup_order_is_explicit_then_repo_then_home() {
    let repo = tmp("repo");
    let home = tmp("home");
    write(
        &home,
        ".config/igris/igris.toml",
        "[checks]\ncommand = \"from-home\"\n",
    );
    assert_eq!(
        Config::load_from(&repo, None, Some(&home))
            .unwrap()
            .checks
            .command,
        "from-home"
    );
    write(&repo, "igris.toml", "[checks]\ncommand = \"from-repo\"\n");
    assert_eq!(
        Config::load_from(&repo, None, Some(&home))
            .unwrap()
            .checks
            .command,
        "from-repo"
    );
    let explicit = write(&repo, "other.toml", "[checks]\ncommand = \"from-flag\"\n");
    assert_eq!(
        Config::load_from(&repo, Some(&explicit), Some(&home))
            .unwrap()
            .checks
            .command,
        "from-flag"
    );
}

#[test]
fn missing_config_names_every_place_it_looked() {
    let repo = tmp("empty");
    let home = tmp("emptyhome");
    let e = Config::load_from(&repo, None, Some(&home)).unwrap_err();
    assert!(matches!(e, ConfigError::NotFound(_)));
    let msg = e.to_string();
    assert!(
        msg.contains("igris.toml") && msg.contains("igris.example.toml"),
        "{msg}"
    );
}

#[test]
fn the_shipped_example_stays_valid() {
    let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/igris.example.toml");
    let d = tmp("example");
    let c = Config::load_from(&d, Some(&example), None).unwrap();
    assert_eq!(c.checks.command, "npm test");
}

#[test]
fn a_bad_risky_path_pattern_is_rejected_at_load() {
    let e = parse("[policy]\nrisky_paths = [\"[\"]\n[checks]\ncommand = \"t\"\n")
        .unwrap_err()
        .to_string();
    assert!(e.contains("policy.risky_paths") && e.contains("'['"), "{e}");
}
