mod common;
use common::{FakeHost, fixture};
use igris_detect::fit::{Candidate, Fit, Source, recommend, recommend_from};

const INFO_LINE: &str = "llmfit info Qwen/Qwen3-4B-Instruct-2507 --json --no-dashboard";

fn with_llmfit() -> FakeHost {
    FakeHost::linux()
        .bin("llmfit")
        .cmd("llmfit --version", "llmfit 1.1.21\n")
}

#[test]
fn llmfit_is_asked_for_exactly_this_and_its_answer_is_read() {
    // The fake host answers only this exact command line, so any other argv would yield nothing.
    let h = with_llmfit().cmd(INFO_LINE, &fixture("llmfit/info-qwen3-4b.json"));
    let r = recommend(&h);
    assert_eq!(
        r.source,
        Source::Llmfit {
            version: "1.1.21".into()
        }
    );
    assert_eq!(r.suggestions.len(), 1);
    assert_eq!(r.suggestions[0].fit, Fit::Marginal);
    assert_eq!(r.suggestions[0].needs_gb, Some(4.36));
}

#[test]
fn llmfits_ollama_name_is_never_used() {
    // The fixture says `ollama_name: qwen3:4b`; the tag to pull stays IGRIS's own.
    let h = with_llmfit().cmd(INFO_LINE, &fixture("llmfit/info-qwen3-4b.json"));
    let r = recommend(&h);
    assert_eq!(
        r.suggestions[0].candidate.tag,
        "qwen3:4b-instruct-2507-q4_K_M"
    );
}

#[test]
fn no_command_is_built_from_llmfit_output() {
    // Hostile text in every field llmfit fills: the pull argv is still the table's, element for element.
    let hostile = r#"{"models":[{"name":"x; touch /tmp/pwned","ollama_name":"$(touch /tmp/pwned)","fit_level":"Good","runnable":true,"memory_required_gb":1.0}]}"#;
    let r = recommend(&with_llmfit().cmd(INFO_LINE, hostile));
    let c = r.suggestions[0].candidate;
    assert_eq!(c.pull, ["ollama", "pull", "qwen3:4b-instruct-2507-q4_K_M"]);
    assert!(c.pull.iter().all(|a| !a.contains("pwned")));
}

#[test]
fn no_llmfit_gives_every_candidate_with_unknown_fit_and_a_reason() {
    let r = recommend(&FakeHost::linux());
    match &r.source {
        Source::Fallback { reason } => assert!(reason.contains("llmfit"), "{reason}"),
        other => panic!("{other:?}"),
    }
    assert!(!r.suggestions.is_empty());
    assert!(r.suggestions.iter().all(|s| s.fit == Fit::Unknown));
}

#[test]
fn a_llmfit_that_does_not_answer_the_version_is_a_fallback() {
    let r = recommend(&FakeHost::linux().bin("llmfit"));
    assert!(matches!(r.source, Source::Fallback { .. }));
    assert!(r.suggestions.iter().all(|s| s.fit == Fit::Unknown));
}

#[test]
fn unparseable_output_leaves_that_candidate_unknown() {
    let h = with_llmfit().cmd(INFO_LINE, "<html>not json</html>");
    let r = recommend(&h);
    assert!(matches!(r.source, Source::Llmfit { .. }));
    assert_eq!(r.suggestions[0].fit, Fit::Unknown);
    assert_eq!(r.suggestions[0].needs_gb, None);
}

static TWO: &[Candidate] = &[
    Candidate {
        hf_name: "Example/Missing-Model",
        tag: "missing:1b",
        pull: &["ollama", "pull", "missing:1b"],
        why: "test",
    },
    Candidate {
        hf_name: "Qwen/Qwen3-4B-Instruct-2507",
        tag: "qwen3:4b-instruct-2507-q4_K_M",
        pull: &["ollama", "pull", "qwen3:4b-instruct-2507-q4_K_M"],
        why: "test",
    },
];

#[test]
fn one_failing_candidate_does_not_hide_the_others() {
    let h = with_llmfit().cmd(INFO_LINE, &fixture("llmfit/info-qwen3-4b.json"));
    let r = recommend_from(&h, TWO);
    assert_eq!(r.suggestions.len(), 2);
    assert_eq!(r.suggestions[0].fit, Fit::Unknown);
    assert_eq!(r.suggestions[1].fit, Fit::Marginal);
}
