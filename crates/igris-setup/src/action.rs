//! The fixed list of things the browser may ask for. There is no "run this command" action.

use crate::SetupError;
use igris_detect::Host;
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    /// Read the machine: the same snapshot as `igris doctor --json`.
    Detect {},
    /// The setup page: the same snapshot, as plain groups and sentences (`view::build`).
    Check {},
}

pub fn parse(body: &[u8]) -> Result<Action, SetupError> {
    serde_json::from_slice(body).map_err(|e| SetupError::BadRequest(e.to_string()))
}

pub fn run(action: &Action, host: &dyn Host) -> Value {
    match action {
        Action::Detect {} => {
            serde_json::to_value(igris_detect::detect(host)).unwrap_or(Value::Null)
        }
        Action::Check {} => serde_json::to_value(crate::view::build(&igris_detect::detect(host)))
            .unwrap_or(Value::Null),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use igris_detect::Host;

    struct Stub;
    impl Host for Stub {
        fn os(&self) -> &str {
            "linux"
        }
        fn home(&self) -> Option<String> {
            None
        }
        fn cpu_cores(&self) -> Option<u32> {
            None
        }
        fn run(&self, _: &str, _: &[&str]) -> Option<String> {
            None
        }
        fn run_with_stderr(&self, _: &str, _: &[&str]) -> Option<String> {
            None
        }
        fn read(&self, _: &str) -> Option<String> {
            None
        }
        fn which(&self, _: &str) -> Option<String> {
            None
        }
        fn has_env(&self, _: &str) -> bool {
            false
        }
        fn http_get(&self, _: u16, _: &str) -> Option<String> {
            None
        }
    }

    #[test]
    fn detect_parses() {
        assert!(matches!(
            parse(br#"{"action":"detect"}"#),
            Ok(Action::Detect {})
        ));
    }

    #[test]
    fn unknown_action_is_refused() {
        assert!(parse(br#"{"action":"rm"}"#).is_err());
        assert!(parse(br#"{"action":"run","cmd":"ls"}"#).is_err());
    }

    #[test]
    fn extra_fields_are_refused() {
        assert!(parse(br#"{"action":"detect","x":1}"#).is_err());
    }

    #[test]
    fn bad_bodies_are_errors_not_panics() {
        let big = vec![b'a'; 70_000];
        for body in [&b""[..], b"not json", b"[]", b"null", &[0xff, 0xfe], &big] {
            assert!(
                matches!(parse(body), Err(SetupError::BadRequest(_))),
                "{body:?}"
            );
        }
    }

    #[test]
    fn detect_returns_the_snapshot() {
        let v = run(&Action::Detect {}, &Stub);
        for key in [
            "machine",
            "herdr",
            "agents",
            "hermes",
            "decision",
            "local_llm",
        ] {
            assert!(v.get(key).is_some(), "{key}");
        }
    }

    #[test]
    fn check_parses() {
        assert!(matches!(
            parse(br#"{"action":"check"}"#),
            Ok(Action::Check {})
        ));
    }

    #[test]
    fn check_rejects_extra_fields() {
        assert!(parse(br#"{"action":"check","x":1}"#).is_err());
    }
}
