//! `igris.toml`: which agents play which role, the check command, and the policy knobs.
//! Unknown keys are an error, so a typo never silently falls back to a default.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Every agent kind Herdr 0.9.1 can start (`herdr agent start --kind`).
pub const KINDS: [&str; 24] = [
    "pi",
    "claude",
    "codex",
    "gemini",
    "cursor",
    "devin",
    "agy",
    "cline",
    "omp",
    "mastracode",
    "opencode",
    "copilot",
    "kimi",
    "kiro",
    "droid",
    "amp",
    "grok",
    "hermes",
    "kilo",
    "qodercli",
    "qwen",
    "letta",
    "maki",
    "muse",
];

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error(
        "no igris.toml found (looked in: {0}). Copy examples/igris.example.toml and set checks.command"
    )]
    NotFound(String),
    #[error("cannot read {path}: {reason}")]
    Read { path: String, reason: String },
    #[error("{path}: {reason}")]
    Parse { path: String, reason: String },
    #[error("{0}")]
    Invalid(String),
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Agents {
    pub builder: String,
    pub reviewer: String,
    pub builder_args: Vec<String>,
    /// `None` means the default for the chosen reviewer (see `reviewer_args()`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reviewer_args: Option<Vec<String>>,
}

impl Agents {
    /// Arguments that keep the reviewer read-only. Explicit `reviewer_args` win; otherwise the
    /// default follows the chosen agent, and agents with no known read-only flag get none.
    pub fn reviewer_args(&self) -> Vec<String> {
        if let Some(a) = &self.reviewer_args {
            return a.clone();
        }
        let flags: &[&str] = match self.reviewer.as_str() {
            "codex" => &["--sandbox", "read-only"],
            "claude" => &["--permission-mode", "plan"],
            _ => &[],
        };
        flags.iter().map(|s| (*s).to_string()).collect()
    }
}

impl Default for Agents {
    fn default() -> Self {
        Self {
            builder: "claude".into(),
            reviewer: "codex".into(),
            builder_args: vec![],
            reviewer_args: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Checks {
    pub command: String,
    pub timeout_s: u64,
}

impl Default for Checks {
    fn default() -> Self {
        Self {
            command: String::new(),
            timeout_s: 900,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Policy {
    pub base: String,
    pub risky_paths: Vec<String>,
    pub risky_words: Vec<String>,
    pub max_review_rounds: u32,
    pub max_fix_rounds: u32,
    pub build_timeout_s: u64,
}

impl Default for Policy {
    fn default() -> Self {
        let own = |v: &[&str]| v.iter().map(|s| (*s).to_string()).collect();
        Self {
            base: "main".into(),
            risky_paths: own(&[".env*", "**/*.sql", "**/migrations/**", "**/auth/**"]),
            risky_words: own(&["migration", "deploy", "secret", "auth", "delete", "drop"]),
            max_review_rounds: 2,
            max_fix_rounds: 2,
            build_timeout_s: 1800,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct DecisionCfg {
    pub backend: String,
    /// Written through `f64` text so 0.85 stays 0.85 in the file.
    #[serde(serialize_with = "short_f32")]
    pub threshold: f32,
}

impl Default for DecisionCfg {
    fn default() -> Self {
        Self {
            backend: "rules".into(),
            threshold: 0.85,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct HerdrCfg {
    pub bin: String,
    /// A saved SSH machine label. Empty (the default) means this computer.
    #[serde(deserialize_with = "empty_is_none", serialize_with = "none_is_empty")]
    pub machine: Option<String>,
}

fn short_f32<S: serde::Serializer>(v: &f32, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_f64(v.to_string().parse().unwrap_or(f64::from(*v)))
}

fn none_is_empty<S: serde::Serializer>(v: &Option<String>, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(v.as_deref().unwrap_or(""))
}

fn empty_is_none<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    let s = String::deserialize(d)?;
    Ok((!s.trim().is_empty()).then_some(s))
}

impl Default for HerdrCfg {
    fn default() -> Self {
        Self {
            bin: "herdr".into(),
            machine: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub agents: Agents,
    pub checks: Checks,
    pub policy: Policy,
    pub decision: DecisionCfg,
    pub herdr: HerdrCfg,
}

impl Config {
    /// Look in `path`, then `<repo>/igris.toml`, then `~/.config/igris/igris.toml`.
    pub fn load(repo: &Path, path: Option<&Path>) -> Result<Self, ConfigError> {
        let home = crate::platform::home();
        Self::load_from(repo, path, home.as_deref())
    }

    pub fn load_from(
        repo: &Path,
        path: Option<&Path>,
        home: Option<&Path>,
    ) -> Result<Self, ConfigError> {
        let found = Self::find(repo, path, home).map_err(ConfigError::NotFound)?;
        let shown = found.display().to_string();
        let text = std::fs::read_to_string(&found).map_err(|e| ConfigError::Read {
            path: shown.clone(),
            reason: e.to_string(),
        })?;
        let cfg = Self::parse(&text, &shown)?;
        cfg.validate()?;
        Ok(cfg)
    }

    /// The file `load_from` would read, or the list of places it looked.
    pub fn find(repo: &Path, path: Option<&Path>, home: Option<&Path>) -> Result<PathBuf, String> {
        let user = home.map(crate::platform::user_config_file_here);
        Self::find_in(repo, path, user.as_deref())
    }

    /// `find` with the user-level file given directly: the lookup order is the same on every platform.
    pub fn find_in(
        repo: &Path,
        path: Option<&Path>,
        user_file: Option<&Path>,
    ) -> Result<PathBuf, String> {
        let mut tried: Vec<PathBuf> = vec![];
        if let Some(p) = path {
            tried.push(p.to_path_buf());
        } else {
            tried.push(repo.join("igris.toml"));
            if let Some(u) = user_file {
                tried.push(u.to_path_buf());
            }
        }
        tried.iter().find(|p| p.is_file()).cloned().ok_or_else(|| {
            tried
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        })
    }

    fn parse(text: &str, shown: &str) -> Result<Self, ConfigError> {
        toml::from_str(text).map_err(|e| ConfigError::Parse {
            path: shown.to_string(),
            reason: e.message().to_string(),
        })
    }

    /// Parse text without checking the values, so a form can show a half-filled file and its problems.
    pub fn from_toml_str(text: &str) -> Result<Self, ConfigError> {
        Self::parse(text, "igris.toml")
    }

    /// The file text for this config. Comments in an old file are not kept.
    pub fn to_toml(&self) -> String {
        let body = toml::to_string(self).unwrap_or_default();
        format!("# Written by igris. Keys are documented in examples/igris.example.toml.\n{body}")
    }

    /// Every problem with the values, in words. Empty means the config can run.
    pub fn problems(&self) -> Vec<String> {
        let mut out = vec![];
        for (key, kind) in [
            ("agents.builder", &self.agents.builder),
            ("agents.reviewer", &self.agents.reviewer),
        ] {
            if !KINDS.contains(&kind.as_str()) {
                out.push(format!(
                    "{key}: unknown agent kind '{kind}'. Use one of: {}",
                    KINDS.join(", ")
                ));
            }
        }
        if self.checks.command.trim().is_empty() {
            out.push("checks.command is required: the command that proves the work is done, for example \"npm test\"".into());
        }
        match self.decision.backend.as_str() {
            "rules" => {}
            b @ ("jev" | "laya") => out.push(format!(
                "decision.backend '{b}' arrives in a later milestone; use \"rules\" for now"
            )),
            b => out.push(format!(
                "decision.backend: unknown backend '{b}'; use \"rules\""
            )),
        }
        if !(0.0..=1.0).contains(&self.decision.threshold) {
            out.push("decision.threshold must be between 0 and 1".into());
        }
        if let Err(e) = crate::decide::risky_globs(&self.policy.risky_paths) {
            out.push(format!("policy.risky_paths: {e}"));
        }
        if self.checks.timeout_s == 0 || self.policy.build_timeout_s == 0 {
            out.push("checks.timeout_s and policy.build_timeout_s must be above 0".into());
        }
        out
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        match self.problems().into_iter().next() {
            Some(first) => Err(ConfigError::Invalid(first)),
            None => Ok(()),
        }
    }
}
