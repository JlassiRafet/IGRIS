mod common;
use common::{FakeHost, fixture};
use igris_detect::{Options, detect, detect_with};

const PY: &str = "import importlib.metadata as m;print(m.version('laya'))";

#[test]
fn local_servers_are_found_by_port_with_their_model_names() {
    let h = FakeHost::linux()
        .http(
            11434,
            "/api/tags",
            &fixture("llm/ollama-tags.handmade.json"),
        )
        .http(
            1234,
            "/v1/models",
            &fixture("llm/openai-models.handmade.json"),
        );
    let list = detect(&h).local_llm;
    let by = |n: &str| list.iter().find(|s| s.name == n).unwrap().clone();
    let ollama = by("ollama");
    assert_eq!((ollama.up, ollama.port), (true, 11434));
    assert_eq!(ollama.models, ["llama3.2:3b", "qwen2.5-coder:7b"]);
    assert_eq!(by("lmstudio").models, ["gemma-3-4b-it"]);
    for closed in ["llamacpp", "vllm", "jan", "koboldcpp"] {
        let s = by(closed);
        assert_eq!((s.up, s.models.is_empty()), (false, true), "{closed}");
    }
    assert_eq!(list.len(), 6);
    assert_eq!(by("llamacpp").port, 8080);
}

#[test]
fn a_server_that_answers_with_unreadable_json_is_up_with_no_models() {
    let h = FakeHost::linux().http(8000, "/v1/models", "<html>");
    let s = detect(&h)
        .local_llm
        .into_iter()
        .find(|s| s.name == "vllm")
        .unwrap();
    assert_eq!((s.up, s.models.is_empty()), (true, true));
}

#[test]
fn a_custom_port_is_checked_too() {
    let h = FakeHost::linux().http(
        9999,
        "/v1/models",
        &fixture("llm/openai-models.handmade.json"),
    );
    let o = Options {
        extra_llm_ports: vec![("mine".into(), 9999)],
    };
    let list = detect_with(&h, &o).local_llm;
    let mine = list.iter().find(|s| s.name == "mine").unwrap();
    assert_eq!(
        (mine.up, mine.port, mine.models.clone()),
        (true, 9999, vec!["gemma-3-4b-it".to_string()])
    );
    assert_eq!(list.len(), 7);
}

#[test]
fn laya_in_the_igris_folder_is_found_with_its_version() {
    let py = "/home/user/.local/share/igris/laya/venv/bin/python";
    let h = FakeHost::linux().cmd(&format!("{py} -c {PY}"), "0.4.35\n");
    let l = detect(&h).decision.laya;
    assert_eq!(
        (l.found, l.version.as_deref(), l.location.as_deref()),
        (true, Some("0.4.35"), Some("igris"))
    );
}

#[test]
fn laya_in_system_python_is_found_and_none_is_none() {
    let sys = FakeHost::linux().cmd(&format!("python3 -c {PY}"), "0.4.35\n");
    assert_eq!(
        detect(&sys).decision.laya.location.as_deref(),
        Some("system")
    );
    let none = detect(&FakeHost::linux()).decision.laya;
    assert_eq!(
        (none.found, none.version, none.location),
        (false, None, None)
    );
}

#[test]
fn jev_is_a_yes_or_no_and_the_key_never_appears() {
    let on = FakeHost::linux().env("TYPESAFE_API_KEY");
    assert!(detect(&on).decision.jev.key);
    assert!(!detect(&FakeHost::linux()).decision.jev.key);
    let json = serde_json::to_string(&detect(&on)).unwrap();
    assert!(
        !json.contains("TYPESAFE_API_KEY"),
        "even the variable name stays out"
    );
}

#[test]
fn hermes_provider_and_chat_apps_are_read_but_secrets_never_are() {
    let h = FakeHost::linux()
        .bin("hermes")
        .cmd("hermes --version", "Hermes Agent v0.12.3\n")
        .file(
            "/home/user/.hermes/config.yaml",
            &fixture("hermes/config.handmade.yaml"),
        )
        .file("/home/user/.hermes/.env", &fixture("hermes/env.handmade"));
    let hm = detect(&h).hermes;
    assert_eq!(
        (hm.found, hm.version.as_deref(), hm.provider.as_deref()),
        (true, Some("0.12.3"), Some("openrouter"))
    );
    assert_eq!(
        hm.platforms,
        ["telegram"],
        "an empty token is not a configured app"
    );
    let json = serde_json::to_string(&detect(&h)).unwrap();
    for secret in [
        "SECRET-TOKEN-VALUE",
        "sk-or-SECRET",
        "123456:",
        "OPENROUTER_API_KEY",
    ] {
        assert!(!json.contains(secret), "leaked {secret}");
    }
}

#[test]
fn hermes_missing_or_without_files_is_not_a_crash() {
    assert!(!detect(&FakeHost::linux()).hermes.found);
    let bare = detect(&FakeHost::linux().bin("hermes")).hermes;
    assert_eq!(
        (bare.found, bare.provider, bare.platforms.len()),
        (true, None, 0)
    );
}
