//! The tool catalog: every tool IGRIS can set up (workspace manager, agents, decision layer,
//! assistant), with its connect methods and live check, as fixed data. Every command here is a
//! fixed argv list: nothing is built from user text, a config file or detected data.
//! An entry that automates a command records how, with which version and when it was verified;
//! the rest carry a docs link only.

mod tools;

pub use tools::TOOLS;

/// The five groups of the Tools step, in the order the step shows them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    Workspace,
    Builder,
    Reviewer,
    Decision,
    Assistant,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MethodKind {
    /// A login the tool already has on this machine (found by the scan).
    Existing,
    /// The tool's own login command, run attached to the terminal.
    Login,
    /// An API key: through an env var (method A) or the tool's store-the-key command (method B).
    Key,
    SetupToken,
    /// A server on this machine, found by a port probe.
    Local,
}

/// What a catalog entry was checked against: the command or page, the tool's version, the date.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Verified {
    pub with: &'static str,
    pub version: &'static str,
    pub date: &'static str,
}

#[derive(Debug)]
pub struct Method {
    pub id: &'static str,
    pub label: &'static str,
    /// The dim bracket shown on the focused row, in IGRIS's own words.
    pub hint: &'static str,
    pub kind: MethodKind,
    /// The tool's own login argv, run attached.
    pub login: Option<&'static [&'static str]>,
    /// The env var that holds the key (method A shows it with the line to add to the shell profile).
    pub env: Option<&'static str>,
    /// A command of the tool that reads the key on stdin and stores it. Method B (the masked
    /// entry) exists only when this is `Some`: IGRIS never writes a key to a file itself.
    pub key_stdin: Option<&'static [&'static str]>,
    pub docs: &'static str,
}

#[derive(Debug)]
pub struct Tool {
    /// The agent kind (`igris_core::config::KINDS`) or `herdr`, `orca`, `rules`, `jev`, `laya`,
    /// `hermes`, `openclaw`, `llmfit` and the other assistants.
    pub id: &'static str,
    pub name: &'static str,
    pub groups: &'static [Group],
    pub install: Option<&'static [&'static str]>,
    /// The install needs administrator rights: shown for the user to run, never run by IGRIS.
    pub install_needs_admin: bool,
    /// The live check: a fixed argv, judged by its exit code.
    pub check: Option<&'static [&'static str]>,
    pub methods: &'static [Method],
    pub docs: &'static str,
    /// How the tool connects, in IGRIS's own words: the dim bracket of its row in the pick list.
    /// Empty when nothing about it was verified.
    pub connects: &'static str,
    /// False: listed, but shown as "not supported yet" or "not selectable yet".
    pub selectable: bool,
    pub verified: Option<Verified>,
}

pub fn tool(id: &str) -> Option<&'static Tool> {
    TOOLS.iter().find(|t| t.id == id)
}

/// The tools of one group, in catalog order (the recommendation order is the Tools screen's).
pub fn tools_in(g: Group) -> Vec<&'static Tool> {
    TOOLS.iter().filter(|t| t.groups.contains(&g)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use igris_core::config::KINDS;

    fn argvs(t: &Tool) -> Vec<&'static [&'static str]> {
        let mut v: Vec<&'static [&'static str]> = vec![];
        v.extend(t.install);
        v.extend(t.check);
        for m in t.methods {
            v.extend(m.login);
            v.extend(m.key_stdin);
        }
        v
    }

    fn method(t: &'static Tool, kind: MethodKind) -> &'static Method {
        t.methods
            .iter()
            .find(|m| m.kind == kind)
            .unwrap_or_else(|| panic!("{} has no {kind:?} method", t.id))
    }

    #[test]
    fn every_agent_kind_has_exactly_one_entry() {
        let mut agents: Vec<&str> = TOOLS
            .iter()
            .filter(|t| t.groups.contains(&Group::Builder))
            .map(|t| t.id)
            .collect();
        agents.sort_unstable();
        let mut kinds = KINDS.to_vec();
        kinds.sort_unstable();
        assert_eq!(agents, kinds);
        for t in TOOLS.iter().filter(|t| t.groups.contains(&Group::Builder)) {
            assert!(
                t.groups.contains(&Group::Reviewer),
                "{} is not a reviewer",
                t.id
            );
        }
    }

    #[test]
    fn ids_are_unique() {
        let mut ids: Vec<&str> = TOOLS.iter().map(|t| t.id).collect();
        ids.sort_unstable();
        let n = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), n);
    }

    #[test]
    fn an_entry_with_any_command_has_verified() {
        for t in TOOLS {
            if argvs(t).is_empty() {
                continue;
            }
            let v = t
                .verified
                .unwrap_or_else(|| panic!("{} automates a command but is not verified", t.id));
            assert!(!v.with.is_empty() && !v.version.is_empty() && !v.date.is_empty());
        }
    }

    #[test]
    fn argv_elements_have_no_placeholders() {
        for t in TOOLS {
            for argv in argvs(t) {
                assert!(!argv.is_empty(), "{} has an empty argv", t.id);
                for a in argv {
                    assert!(
                        !a.contains('{') && !a.contains('}'),
                        "{}: placeholder in {a:?}",
                        t.id
                    );
                }
            }
        }
    }

    #[test]
    fn a_tool_that_is_not_selectable_runs_no_command() {
        for t in TOOLS.iter().filter(|t| !t.selectable) {
            assert!(
                argvs(t).is_empty(),
                "{} is not selectable but has a command",
                t.id
            );
        }
    }

    #[test]
    fn a_key_method_has_an_env_name_or_key_stdin() {
        for t in TOOLS {
            for m in t.methods.iter().filter(|m| m.kind == MethodKind::Key) {
                assert!(
                    m.env.is_some() || m.key_stdin.is_some(),
                    "{}/{}",
                    t.id,
                    m.id
                );
            }
        }
    }

    #[test]
    fn claude_methods_are_exact() {
        let t = tool("claude").unwrap();
        assert_eq!(
            method(t, MethodKind::Login).login,
            Some(&["claude", "auth", "login"][..])
        );
        assert_eq!(
            method(t, MethodKind::SetupToken).login,
            Some(&["claude", "setup-token"][..])
        );
        let key = method(t, MethodKind::Key);
        assert_eq!(key.env, Some("ANTHROPIC_API_KEY"));
        assert_eq!(key.key_stdin, None);
        assert_eq!(
            t.check,
            Some(
                &[
                    "claude",
                    "-p",
                    "Reply with the single word ok.",
                    "--no-session-persistence"
                ][..]
            )
        );
    }

    #[test]
    fn codex_methods_are_exact() {
        let t = tool("codex").unwrap();
        assert_eq!(
            method(t, MethodKind::Login).login,
            Some(&["codex", "login"][..])
        );
        let key = method(t, MethodKind::Key);
        assert_eq!(
            key.key_stdin,
            Some(&["codex", "login", "--with-api-key"][..])
        );
        assert_eq!(key.env, Some("OPENAI_API_KEY"));
        assert_eq!(
            t.check,
            Some(
                &[
                    "codex",
                    "exec",
                    "--skip-git-repo-check",
                    "--ephemeral",
                    "Reply with the single word ok."
                ][..]
            )
        );
    }

    #[test]
    fn herdr_is_the_only_selectable_workspace_tool() {
        let ws = tools_in(Group::Workspace);
        let selectable: Vec<&str> = ws.iter().filter(|t| t.selectable).map(|t| t.id).collect();
        assert_eq!(selectable, ["herdr"]);
        assert!(ws.iter().any(|t| t.id == "orca" && !t.selectable));
    }

    #[test]
    fn rules_is_selectable_and_jev_laya_are_not() {
        let d = tools_in(Group::Decision);
        let sel = |id: &str| d.iter().find(|t| t.id == id).map(|t| t.selectable);
        assert_eq!(sel("rules"), Some(true));
        assert_eq!(sel("jev"), Some(false));
        assert_eq!(sel("laya"), Some(false));
    }

    #[test]
    fn hermes_and_openclaw_are_the_selectable_assistants() {
        let a = tools_in(Group::Assistant);
        let mut selectable: Vec<&str> = a.iter().filter(|t| t.selectable).map(|t| t.id).collect();
        selectable.sort_unstable();
        assert_eq!(selectable, ["hermes", "openclaw"]);
    }

    #[test]
    fn a_verified_selectable_tool_says_how_it_connects() {
        for t in TOOLS
            .iter()
            .filter(|t| t.selectable && t.verified.is_some())
        {
            assert!(!t.connects.is_empty(), "{} has no connects text", t.id);
        }
    }

    #[test]
    fn lookup_finds_a_tool_and_misses_an_unknown_id() {
        assert_eq!(tool("herdr").map(|t| t.name), Some("Herdr"));
        assert!(tool("no-such-tool").is_none());
    }
}
