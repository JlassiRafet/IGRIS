//! `igris.toml`: which agents play which role, the check command, and the policy knobs.
//! Unknown keys are an error, so a typo never silently falls back to a default.

use serde::Deserialize;
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

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Agents {
    pub builder: String,
    pub reviewer: String,
    pub builder_args: Vec<String>,
    reviewer_args: Option<Vec<String>>,
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

#[derive(Debug, Clone, PartialEq, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DecisionCfg {
    pub backend: String,
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

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct HerdrCfg {
    pub bin: String,
    /// A saved SSH machine label. Empty (the default) means this computer.
    #[serde(deserialize_with = "empty_is_none")]
    pub machine: Option<String>,
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

#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
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
        let home = std::env::var_os("HOME").map(PathBuf::from);
        Self::load_from(repo, path, home.as_deref())
    }

    pub fn load_from(
        repo: &Path,
        path: Option<&Path>,
        home: Option<&Path>,
    ) -> Result<Self, ConfigError> {
        let mut tried: Vec<PathBuf> = vec![];
        if let Some(p) = path {
            tried.push(p.to_path_buf());
        } else {
            tried.push(repo.join("igris.toml"));
            if let Some(h) = home {
                tried.push(h.join(".config/igris/igris.toml"));
            }
        }
        let Some(found) = tried.iter().find(|p| p.is_file()) else {
            let list = tried
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ");
            return Err(ConfigError::NotFound(list));
        };
        let shown = found.display().to_string();
        let text = std::fs::read_to_string(found).map_err(|e| ConfigError::Read {
            path: shown.clone(),
            reason: e.to_string(),
        })?;
        let cfg: Config = toml::from_str(&text).map_err(|e| ConfigError::Parse {
            path: shown.clone(),
            reason: e.message().to_string(),
        })?;
        cfg.validate()?;
        Ok(cfg)
    }

    fn validate(&self) -> Result<(), ConfigError> {
        for (key, kind) in [
            ("agents.builder", &self.agents.builder),
            ("agents.reviewer", &self.agents.reviewer),
        ] {
            if !KINDS.contains(&kind.as_str()) {
                return Err(ConfigError::Invalid(format!(
                    "{key}: unknown agent kind '{kind}'. Use one of: {}",
                    KINDS.join(", ")
                )));
            }
        }
        if self.checks.command.trim().is_empty() {
            return Err(ConfigError::Invalid(
                "checks.command is required: the command that proves the work is done, for example \"npm test\"".into(),
            ));
        }
        match self.decision.backend.as_str() {
            "rules" => {}
            b @ ("jev" | "laya") => {
                return Err(ConfigError::Invalid(format!(
                    "decision.backend '{b}' arrives in a later milestone; use \"rules\" for now"
                )));
            }
            b => {
                return Err(ConfigError::Invalid(format!(
                    "decision.backend: unknown backend '{b}'; use \"rules\""
                )));
            }
        }
        if !(0.0..=1.0).contains(&self.decision.threshold) {
            return Err(ConfigError::Invalid(
                "decision.threshold must be between 0 and 1".into(),
            ));
        }
        if self.checks.timeout_s == 0 || self.policy.build_timeout_s == 0 {
            return Err(ConfigError::Invalid(
                "checks.timeout_s and policy.build_timeout_s must be above 0".into(),
            ));
        }
        Ok(())
    }
}
