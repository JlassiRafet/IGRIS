use igris_core::config::Config;
use igris_core::decide::{Decision, Kind, decide, igris_home_from, log_decision, log_outcome};
use serde_json::{Value, json};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

fn cfg() -> Config {
    let mut c = Config::default();
    c.checks.command = "t".into();
    c
}

fn answer<'a>(d: &'a [Decision], q: &str) -> &'a str {
    &d.iter().find(|x| x.question == q).unwrap().answer
}

fn tmp(name: &str) -> PathBuf {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let d = std::env::temp_dir().join(format!("igris-dec-{}-{n}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    d
}

fn lines(p: &Path) -> Vec<Value> {
    fs::read_to_string(p)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

#[test]
fn answers_come_in_the_jev_laya_shape_and_order() {
    let d = decide("add a login page", &cfg(), &[]);
    let names: Vec<_> = d.iter().map(|x| x.question.as_str()).collect();
    assert_eq!(names, ["agent", "tier", "risk", "needs_approval"]);
    let kinds: Vec<_> = d.iter().map(|x| x.kind).collect();
    assert_eq!(kinds, [Kind::Choice, Kind::Choice, Kind::Score, Kind::Noul]);
    assert!(d.iter().all(|x| x.confidence == 1.0 && x.source == "rules"));
    assert_eq!(answer(&d, "agent"), "claude");
}

#[test]
fn a_short_fix_is_the_small_tier_and_anything_else_is_mid() {
    let c = cfg();
    assert_eq!(
        answer(&decide("fix the failing cart test", &c, &[]), "tier"),
        "small"
    );
    assert_eq!(
        answer(&decide("Rename props in settings", &c, &[]), "tier"),
        "small"
    );
    assert_eq!(answer(&decide("add a login page", &c, &[]), "tier"), "mid");
    let long = format!("fix {}", "x".repeat(80));
    assert_eq!(answer(&decide(&long, &c, &[]), "tier"), "mid");
}

#[test]
fn a_risky_word_needs_approval() {
    let c = cfg();
    for t in [
        "run the db migration",
        "Deploy to staging",
        "rotate the secret",
        "add authentication",
    ] {
        let d = decide(t, &c, &[]);
        assert_eq!(
            (answer(&d, "risk"), answer(&d, "needs_approval")),
            ("high", "yes"),
            "{t}"
        );
    }
    let d = decide("update the readme", &c, &[]);
    assert_eq!(
        (answer(&d, "risk"), answer(&d, "needs_approval")),
        ("low", "no")
    );
}

#[test]
fn a_risky_path_glob_marks_high_even_when_the_words_are_harmless() {
    let c = cfg();
    for p in [
        "db/migrations/014.sql",
        "schema.sql",
        ".env.local",
        "src/app/.env",
        "src/auth/session.ts",
    ] {
        let d = decide("tidy up", &c, &[p.to_string()]);
        assert_eq!(answer(&d, "risk"), "high", "{p}");
    }
    assert_eq!(
        answer(&decide("tidy up", &c, &["src/cart.ts".into()]), "risk"),
        "low"
    );
}

#[test]
fn an_unreadable_glob_fails_closed() {
    let mut c = cfg();
    c.policy.risky_paths = vec!["[".into()];
    assert_eq!(
        answer(&decide("tidy up", &c, &["a.txt".into()]), "risk"),
        "high"
    );
}

#[test]
fn the_log_appends_and_never_rewrites() {
    let dir = tmp("log");
    let p = dir.join("nested/decisions.jsonl");
    let d = decide("add a login page", &cfg(), &[]);
    log_decision(&p, "t1", &d[0]).unwrap();
    let first = lines(&p);
    log_decision(&p, "t1", &d[1]).unwrap();
    let all = lines(&p);
    assert_eq!(all.len(), 2);
    assert_eq!(all[0], first[0]);
    assert_eq!(
        (all[0]["kind"].as_str(), all[0]["task_id"].as_str()),
        (Some("decision"), Some("t1"))
    );
    assert_eq!(all[1]["decision"]["question"], "tier");
    assert!(all[0]["ts"].as_u64().unwrap() > 1_700_000_000);
}

#[test]
fn outcomes_are_logged_next_to_the_decisions_they_judge() {
    let p = tmp("out").join("decisions.jsonl");
    log_outcome(&p, "t1", &json!({"merged": true, "checks_exit": 0})).unwrap();
    let l = lines(&p);
    assert_eq!(l[0]["kind"], "outcome");
    assert_eq!(l[0]["outcome"]["merged"], true);
}

#[test]
fn igris_home_prefers_the_env_var_then_dot_igris_in_home() {
    let some = |s: &str| Some(OsString::from(s));
    assert_eq!(
        igris_home_from(some("/x/igris"), some("/home/u")),
        PathBuf::from("/x/igris")
    );
    assert_eq!(
        igris_home_from(None, some("/home/u")),
        PathBuf::from("/home/u/.igris")
    );
    assert_eq!(
        igris_home_from(some(""), some("/home/u")),
        PathBuf::from("/home/u/.igris")
    );
}
