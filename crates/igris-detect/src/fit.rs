//! Which local models fit this machine, asked of the external `llmfit` tool (MIT, a separate
//! program; nothing of it is linked or copied). IGRIS keeps its own short list of models proven to
//! call tools, and asks llmfit only for the fit of each. No command is ever built from llmfit's
//! output: the model names and the Ollama tags below are the only ones used.
use crate::Host;

/// A model IGRIS can recommend as an assistant's brain: proven to call tools, with the fixed
/// names llmfit and Ollama know it by.
#[derive(Debug, PartialEq, Eq)]
pub struct Candidate {
    /// The name `llmfit info` looks up.
    pub hf_name: &'static str,
    /// The Ollama tag to pull. Never taken from llmfit's `ollama_name`, which may differ (it
    /// names another quantisation).
    pub tag: &'static str,
    /// The whole pull command, fixed here: the only argv the app ever runs for a model.
    pub pull: &'static [&'static str],
    pub why: &'static str,
}

/// At most five, each added only after it called tools through Hermes in a real `igris run`.
pub static CANDIDATES: &[Candidate] = &[Candidate {
    hf_name: "Qwen/Qwen3-4B-Instruct-2507",
    tag: "qwen3:4b-instruct-2507-q4_K_M",
    pull: &["ollama", "pull", "qwen3:4b-instruct-2507-q4_K_M"],
    why: "calls tools through Hermes (igris run), checked 2026-11-23",
}];

/// llmfit's `fit_level`. A label this code does not know is `Unknown`, never a guess.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fit {
    Perfect,
    Good,
    Marginal,
    TooBig,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Parsed {
    pub fit: Fit,
    pub needs_gb: Option<f32>,
    pub available_gb: Option<f32>,
    pub tps: Option<f32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Suggestion {
    pub candidate: &'static Candidate,
    pub fit: Fit,
    pub needs_gb: Option<f32>,
    pub available_gb: Option<f32>,
    pub tps: Option<f32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    Llmfit { version: String },
    Fallback { reason: String },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Report {
    pub source: Source,
    pub suggestions: Vec<Suggestion>,
}

/// The fit of one model from `llmfit info <model> --json`.
pub fn parse_info(json: &str) -> Result<Parsed, String> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| format!("not JSON: {e}"))?;
    let model = v["models"]
        .get(0)
        .ok_or_else(|| "no model in the answer".to_string())?;
    let num = |key: &str| model[key].as_f64().map(|n| n as f32);
    // `Good` and `Marginal` were seen; `Perfect` is documented (`--min-fit perfect|good|marginal`).
    let fit = if model["runnable"].as_bool() == Some(false) {
        Fit::TooBig
    } else {
        match model["fit_level"].as_str() {
            Some("Perfect") => Fit::Perfect,
            Some("Good") => Fit::Good,
            Some("Marginal") => Fit::Marginal,
            _ => Fit::Unknown,
        }
    };
    Ok(Parsed {
        fit,
        needs_gb: num("memory_required_gb"),
        available_gb: num("memory_available_gb"),
        tps: num("estimated_tps"),
    })
}

/// The fit of each of IGRIS's own models on this machine.
pub fn recommend(host: &dyn Host) -> Report {
    recommend_from(host, CANDIDATES)
}

pub fn recommend_from(host: &dyn Host, candidates: &'static [Candidate]) -> Report {
    let unknown = |candidate| Suggestion {
        candidate,
        fit: Fit::Unknown,
        needs_gb: None,
        available_gb: None,
        tps: None,
    };
    let installed = host.which("llmfit").is_some();
    let version = host
        .run("llmfit", &["--version"])
        .filter(|_| installed)
        .and_then(|out| out.split_whitespace().nth(1).map(str::to_string));
    let Some(version) = version else {
        let reason = if installed {
            "llmfit did not answer `--version`"
        } else {
            "llmfit is not installed"
        };
        return Report {
            source: Source::Fallback {
                reason: reason.into(),
            },
            suggestions: candidates.iter().map(unknown).collect(),
        };
    };
    let suggestions = candidates
        .iter()
        .map(|c| {
            let parsed = host
                .run("llmfit", &["info", c.hf_name, "--json", "--no-dashboard"])
                .and_then(|out| parse_info(&out).ok());
            match parsed {
                Some(p) => Suggestion {
                    candidate: c,
                    fit: p.fit,
                    needs_gb: p.needs_gb,
                    available_gb: p.available_gb,
                    tps: p.tps,
                },
                None => unknown(c),
            }
        })
        .collect();
    Report {
        source: Source::Llmfit { version },
        suggestions,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INFO: &str = include_str!("../tests/fixtures/llmfit/info-qwen3-4b.json");

    #[test]
    fn parses_the_recorded_info() {
        let p = parse_info(INFO).unwrap();
        assert_eq!(p.fit, Fit::Marginal);
        assert_eq!(p.needs_gb, Some(4.36));
        assert_eq!(p.available_gb, Some(4.5));
        assert_eq!(p.tps, Some(60.2));
    }

    #[test]
    fn the_documented_levels_map_and_an_unrecognised_one_is_unknown() {
        let level = |l: &str| {
            parse_info(&format!(
                r#"{{"models":[{{"fit_level":"{l}","runnable":true}}]}}"#
            ))
            .unwrap()
            .fit
        };
        assert_eq!(level("Perfect"), Fit::Perfect);
        assert_eq!(level("Good"), Fit::Good);
        assert_eq!(level("Marginal"), Fit::Marginal);
        assert_eq!(level("Something New"), Fit::Unknown);
    }

    #[test]
    fn a_model_that_cannot_run_is_too_big_whatever_its_level() {
        let p = parse_info(r#"{"models":[{"fit_level":"Good","runnable":false}]}"#).unwrap();
        assert_eq!(p.fit, Fit::TooBig);
    }

    #[test]
    fn numbers_that_are_missing_stay_none() {
        let p = parse_info(r#"{"models":[{"fit_level":"Good","runnable":true}]}"#).unwrap();
        assert_eq!((p.needs_gb, p.available_gb, p.tps), (None, None, None));
    }

    #[test]
    fn output_that_is_not_an_answer_is_an_error() {
        assert!(parse_info("<html>").is_err());
        assert!(parse_info(r#"{"models":[]}"#).is_err());
        assert!(parse_info("{}").is_err());
    }

    #[test]
    fn the_candidates_are_few_and_complete() {
        assert!(!CANDIDATES.is_empty() && CANDIDATES.len() <= 5);
        for c in CANDIDATES {
            assert!(!c.hf_name.is_empty() && !c.tag.is_empty() && !c.why.is_empty());
            assert_eq!(
                c.pull,
                ["ollama", "pull", c.tag],
                "the pull command is the table's tag"
            );
        }
    }
}
