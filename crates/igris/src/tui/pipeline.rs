//! The Pipeline form's data: `igris.toml` as editable fields, and back, with the doctor's own
//! messages as validation.
use super::form::{ListEditor, NumberInput, Select, SelectItem};
use super::input::Input;
use igris_core::config::{Agents, Config, ConfigError, KINDS};
use igris_detect::{Login, Snapshot};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldError {
    pub field: &'static str,
    pub message: String,
}

/// What the form starts from.
#[derive(Debug, PartialEq)]
pub enum Loaded {
    /// A file that parses (its values may still be wrong; the form shows the problems).
    Existing(Config, PathBuf),
    /// No file yet: defaults plus suggestions from this machine.
    Fresh(Config),
    /// A file that cannot be read or has an unknown key. Never overwritten without a choice.
    Broken(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct PipelineForm {
    pub builder: Select,
    pub reviewer: Select,
    pub builder_args: ListEditor,
    /// `None`: the default read-only flags for the chosen reviewer.
    pub reviewer_args: Option<ListEditor>,
    pub command: Input,
    pub checks_timeout: NumberInput,
    pub base: Input,
    pub risky_paths: ListEditor,
    pub risky_words: ListEditor,
    pub max_review_rounds: NumberInput,
    pub max_fix_rounds: NumberInput,
    pub build_timeout: NumberInput,
    pub backend: Select,
    pub threshold: NumberInput,
    pub herdr_bin: Input,
    pub herdr_machine: Input,
}

fn item(label: &str, note: Option<&str>, enabled: bool) -> SelectItem {
    SelectItem {
        label: label.into(),
        enabled,
        note: note.map(Into::into),
    }
}

/// Agents found on this machine first (with their login state), then every kind Herdr supports.
fn kinds(current: &str, snap: &Snapshot) -> Select {
    let mut items: Vec<SelectItem> = vec![];
    for a in snap
        .agents
        .iter()
        .filter(|a| a.found && KINDS.contains(&a.kind.as_str()))
    {
        let note = match a.login {
            Login::LoggedIn => "logged in",
            Login::LoggedOut => "not logged in",
            Login::Unknown => "found",
        };
        items.push(item(&a.kind, Some(note), true));
    }
    for k in KINDS {
        if !items.iter().any(|i| i.label == k) {
            items.push(item(k, None, true));
        }
    }
    if !items.iter().any(|i| i.label == current) {
        // A value from the file that is not a kind: keep it so the error names it.
        items.insert(0, item(current, Some("unknown"), true));
    }
    let index = items.iter().position(|i| i.label == current).unwrap_or(0);
    Select::new(items, index)
}

fn backend(current: &str) -> Select {
    let mut items = vec![
        item("rules", None, true),
        item("jev", Some("Soon"), false),
        item("laya", Some("Soon"), false),
    ];
    if !items.iter().any(|i| i.label == current) {
        items.push(item(current, Some("unknown"), false));
    }
    let index = items.iter().position(|i| i.label == current).unwrap_or(0);
    Select::new(items, index)
}

/// Builder: the first logged-in agent. Reviewer: a different logged-in one when there is one,
/// because a second agent catches more. With one agent both are that agent; with none, defaults.
pub fn suggest(snap: &Snapshot) -> Agents {
    let logged: Vec<&str> = snap
        .agents
        .iter()
        .filter(|a| a.found && a.login == Login::LoggedIn && KINDS.contains(&a.kind.as_str()))
        .map(|a| a.kind.as_str())
        .collect();
    let mut a = Agents::default();
    if let Some(first) = logged.first() {
        a.builder = (*first).into();
        a.reviewer = logged
            .iter()
            .find(|k| *k != first)
            .unwrap_or(first)
            .to_string();
    }
    a
}

fn list(v: &[String]) -> ListEditor {
    ListEditor { items: v.to_vec() }
}

fn number_error(field: &'static str, what: &str) -> FieldError {
    FieldError {
        field,
        message: format!("{field} must be {what}"),
    }
}

/// Which field a message from `Config::problems` belongs to.
fn field_of(msg: &str) -> &'static str {
    const FIELDS: [&str; 6] = [
        "agents.builder",
        "agents.reviewer",
        "checks.command",
        "decision.backend",
        "decision.threshold",
        "policy.risky_paths",
    ];
    if msg.starts_with("checks.timeout_s") {
        return "checks.timeout_s";
    }
    FIELDS
        .into_iter()
        .find(|f| msg.starts_with(f))
        .unwrap_or("config")
}

impl PipelineForm {
    pub fn from_config(c: &Config, snap: &Snapshot) -> PipelineForm {
        PipelineForm {
            builder: kinds(&c.agents.builder, snap),
            reviewer: kinds(&c.agents.reviewer, snap),
            builder_args: list(&c.agents.builder_args),
            reviewer_args: c.agents.reviewer_args.as_deref().map(list),
            command: Input::new(&c.checks.command),
            checks_timeout: NumberInput::whole(c.checks.timeout_s),
            base: Input::new(&c.policy.base),
            risky_paths: list(&c.policy.risky_paths),
            risky_words: list(&c.policy.risky_words),
            max_review_rounds: NumberInput::whole(c.policy.max_review_rounds.into()),
            max_fix_rounds: NumberInput::whole(c.policy.max_fix_rounds.into()),
            build_timeout: NumberInput::whole(c.policy.build_timeout_s),
            backend: backend(&c.decision.backend),
            // Through text, so an f32 0.85 shows as 0.85 and not 0.8500000238418579.
            threshold: NumberInput::decimal(
                c.decision.threshold.to_string().parse().unwrap_or(0.85),
            ),
            herdr_bin: Input::new(&c.herdr.bin),
            herdr_machine: Input::new(c.herdr.machine.as_deref().unwrap_or("")),
        }
    }

    fn label(s: &Select) -> String {
        s.current().map(|i| i.label.clone()).unwrap_or_default()
    }

    /// Set when the same agent builds and reviews: legal, but the weaker choice.
    pub fn same_agent(&self) -> bool {
        Self::label(&self.builder) == Self::label(&self.reviewer)
    }

    /// The config these fields describe, or every problem in it, in the doctor's own words.
    pub fn to_config(&self) -> Result<Config, Vec<FieldError>> {
        let mut errors = vec![];
        let mut whole = |n: &NumberInput, field: &'static str, default: u64| {
            n.as_u64().unwrap_or_else(|| {
                errors.push(number_error(field, "a whole number"));
                default
            })
        };
        let d = Config::default();
        let checks_timeout = whole(&self.checks_timeout, "checks.timeout_s", d.checks.timeout_s);
        let review_rounds = whole(&self.max_review_rounds, "policy.max_review_rounds", 2);
        let fix_rounds = whole(&self.max_fix_rounds, "policy.max_fix_rounds", 2);
        let build_timeout = whole(
            &self.build_timeout,
            "policy.build_timeout_s",
            d.policy.build_timeout_s,
        );
        let threshold = self.threshold.as_f64().unwrap_or_else(|| {
            errors.push(number_error("decision.threshold", "a number"));
            0.85
        });
        let machine = self.herdr_machine.value().trim();
        let mut c = Config::default();
        c.agents.builder = Self::label(&self.builder);
        c.agents.reviewer = Self::label(&self.reviewer);
        c.agents.builder_args = self.builder_args.items.clone();
        c.agents.reviewer_args = self.reviewer_args.as_ref().map(|l| l.items.clone());
        c.checks.command = self.command.value().to_string();
        c.checks.timeout_s = checks_timeout;
        c.policy.base = self.base.value().to_string();
        c.policy.risky_paths = self.risky_paths.items.clone();
        c.policy.risky_words = self.risky_words.items.clone();
        c.policy.max_review_rounds = u32::try_from(review_rounds).unwrap_or(u32::MAX);
        c.policy.max_fix_rounds = u32::try_from(fix_rounds).unwrap_or(u32::MAX);
        c.policy.build_timeout_s = build_timeout;
        c.decision.backend = Self::label(&self.backend);
        c.decision.threshold = threshold as f32;
        c.herdr.bin = self.herdr_bin.value().to_string();
        c.herdr.machine = (!machine.is_empty()).then(|| machine.to_string());
        errors.extend(c.problems().into_iter().map(|message| FieldError {
            field: field_of(&message),
            message,
        }));
        if errors.is_empty() {
            Ok(c)
        } else {
            Err(errors)
        }
    }
}

pub fn load_for_form(repo: &Path, snap: &Snapshot) -> Loaded {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    load_for_form_in(repo, home.as_deref(), snap)
}

pub fn load_for_form_in(repo: &Path, home: Option<&Path>, snap: &Snapshot) -> Loaded {
    let Ok(path) = Config::find(repo, None, home) else {
        return Loaded::Fresh(Config {
            agents: suggest(snap),
            ..Config::default()
        });
    };
    let shown = path.display();
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) => return Loaded::Broken(format!("cannot read {shown}: {e}")),
    };
    match Config::from_toml_str(&text) {
        Ok(c) => Loaded::Existing(c, path),
        Err(ConfigError::Parse { reason, .. }) => Loaded::Broken(format!("{shown}: {reason}")),
        Err(e) => Loaded::Broken(format!("{shown}: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::view::tests::{agent, real};

    fn snap_with(agents: &[(&str, Login)]) -> Snapshot {
        let mut s = real();
        s.agents = agents
            .iter()
            .map(|(k, l)| agent(k, true, *l, None))
            .collect();
        s
    }

    fn valid() -> Config {
        let mut c = Config::default();
        c.checks.command = "cargo test".into();
        c
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("igris-pipe-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn two_logged_in_agents_split_the_roles() {
        let a = suggest(&snap_with(&[
            ("pi", Login::Unknown),
            ("codex", Login::LoggedIn),
            ("claude", Login::LoggedIn),
        ]));
        assert_eq!(
            (a.builder.as_str(), a.reviewer.as_str()),
            ("codex", "claude")
        );
    }

    #[test]
    fn one_logged_in_agent_does_both_jobs_and_the_form_says_so() {
        let snap = snap_with(&[("claude", Login::LoggedIn), ("codex", Login::LoggedOut)]);
        let a = suggest(&snap);
        assert_eq!(
            (a.builder.as_str(), a.reviewer.as_str()),
            ("claude", "claude")
        );
        let f = PipelineForm::from_config(
            &Config {
                agents: a,
                ..valid()
            },
            &snap,
        );
        assert!(f.same_agent());
    }

    #[test]
    fn with_no_logged_in_agent_the_defaults_stay() {
        assert_eq!(
            suggest(&snap_with(&[("claude", Login::LoggedOut)])),
            Agents::default()
        );
    }

    #[test]
    fn detected_agents_come_first_with_their_login_state() {
        let snap = snap_with(&[("codex", Login::LoggedIn), ("claude", Login::LoggedOut)]);
        let f = PipelineForm::from_config(&valid(), &snap);
        let labels: Vec<_> = f.builder.items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(&labels[..2], ["codex", "claude"]);
        assert_eq!(f.builder.items[0].note.as_deref(), Some("logged in"));
        assert_eq!(f.builder.items[1].note.as_deref(), Some("not logged in"));
        assert_eq!(
            f.builder.items.len(),
            KINDS.len(),
            "every supported kind is there once"
        );
        assert_eq!(f.builder.current().unwrap().label, "claude");
    }

    #[test]
    fn a_valid_config_goes_through_the_form_unchanged() {
        let mut c = valid();
        c.policy.risky_paths = vec!["{a,b}/**".into()];
        c.agents.builder_args = vec!["--x".into()];
        c.herdr.machine = Some("box".into());
        let f = PipelineForm::from_config(&c, &real());
        assert_eq!(f.to_config().unwrap(), c);
    }

    #[test]
    fn the_empty_check_command_gets_the_doctors_own_message() {
        let f = PipelineForm::from_config(&Config::default(), &real());
        let errs = f.to_config().unwrap_err();
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert_eq!(errs[0].field, "checks.command");
        assert_eq!(
            errs[0].message,
            Config::default().validate().unwrap_err().to_string()
        );
    }

    #[test]
    fn jev_and_laya_are_shown_as_soon_and_cannot_be_chosen() {
        let mut f = PipelineForm::from_config(&valid(), &real());
        let note: Vec<_> = f
            .backend
            .items
            .iter()
            .map(|i| (i.label.as_str(), i.enabled, i.note.as_deref()))
            .collect();
        assert_eq!(
            note,
            [
                ("rules", true, None),
                ("jev", false, Some("Soon")),
                ("laya", false, Some("Soon"))
            ]
        );
        assert!(!f.backend.choose(1) && !f.backend.choose(2));
        assert!(f.to_config().is_ok());
    }

    #[test]
    fn a_loaded_jev_config_is_shown_with_the_doctors_reason() {
        let mut c = valid();
        c.decision.backend = "jev".into();
        let errs = PipelineForm::from_config(&c, &real())
            .to_config()
            .unwrap_err();
        assert_eq!(errs[0].field, "decision.backend");
        assert!(errs[0].message.contains("arrives in a later milestone"));
    }

    #[test]
    fn numbers_out_of_range_or_not_numbers_are_field_errors() {
        let mut c = valid();
        c.decision.threshold = 3.0;
        let errs = PipelineForm::from_config(&c, &real())
            .to_config()
            .unwrap_err();
        assert_eq!(errs[0].field, "decision.threshold");

        let mut f = PipelineForm::from_config(&valid(), &real());
        f.checks_timeout = NumberInput::whole(0);
        f.checks_timeout
            .handle(ratatui::crossterm::event::KeyEvent::new(
                ratatui::crossterm::event::KeyCode::Backspace,
                ratatui::crossterm::event::KeyModifiers::NONE,
            ));
        let errs = f.to_config().unwrap_err();
        assert_eq!(errs[0].field, "checks.timeout_s");
        assert!(errs[0].message.contains("whole number"));
    }

    #[test]
    fn a_missing_file_starts_fresh_with_suggestions() {
        let (repo, home) = (tmp("fresh"), tmp("freshhome"));
        let snap = snap_with(&[("claude", Login::LoggedIn), ("codex", Login::LoggedIn)]);
        let Loaded::Fresh(c) = load_for_form_in(&repo, Some(&home), &snap) else {
            panic!()
        };
        assert_eq!(c.agents.builder, "claude");
        assert_eq!(c.agents.reviewer, "codex");
    }

    #[test]
    fn an_existing_file_loads_even_when_its_values_are_incomplete() {
        let (repo, home) = (tmp("exist"), tmp("existhome"));
        std::fs::write(repo.join("igris.toml"), "[agents]\nbuilder = \"codex\"\n").unwrap();
        let Loaded::Existing(c, path) = load_for_form_in(&repo, Some(&home), &real()) else {
            panic!()
        };
        assert_eq!(c.agents.builder, "codex");
        assert_eq!(path, repo.join("igris.toml"));
    }

    #[test]
    fn a_file_with_an_unknown_key_is_broken_and_says_why() {
        let (repo, home) = (tmp("broken"), tmp("brokenhome"));
        std::fs::write(
            repo.join("igris.toml"),
            "[checks]\ncommand = \"t\"\nbogus = 1\n",
        )
        .unwrap();
        let Loaded::Broken(msg) = load_for_form_in(&repo, Some(&home), &real()) else {
            panic!()
        };
        assert!(
            msg.contains("igris.toml") && msg.contains("unknown field `bogus`"),
            "{msg}"
        );
    }
}
