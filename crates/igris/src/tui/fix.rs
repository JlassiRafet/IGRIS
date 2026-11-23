//! What can be done about a row that needs a step. Every command here is a fixed constant: nothing
//! typed by the user, read from a config file or detected on the machine is ever run.
use crate::check::view::{Row, Status};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fix {
    /// The tool's own login, run attached to this terminal.
    Login(&'static [&'static str]),
    /// A fixed install command: shown first, run only after a yes.
    Install {
        cmd: &'static str,
        url: &'static str,
    },
    Docs(&'static str),
    Copy(String),
}

/// Agent kind to the argv of its own login. `codex login` with no subcommand is the login command
/// (`codex login --help` lists only `status` besides it).
pub const LOGIN: &[(&str, &[&str])] = &[
    ("claude", &["claude", "auth", "login"]),
    ("codex", &["codex", "login"]),
];

pub const INSTALL_HERDR: (&str, &str) = (
    "curl -fsSL https://herdr.dev/install.sh | sh",
    "https://herdr.dev",
);
/// The same command as one fixed argv for `sh`.
pub const INSTALL_HERDR_ARGV: &[&str] = &["sh", "-c", INSTALL_HERDR.0];

const DOCS_CLAUDE: &str = "https://docs.anthropic.com/en/docs/claude-code/overview";
const DOCS_CODEX: &str = "https://developers.openai.com/codex";
const DOCS_HERDR: &str = "https://herdr.dev/docs/";

/// The agent kind a view row stands for (rows carry display names).
fn kind_of(name: &str) -> &str {
    match name {
        "Claude Code" => "claude",
        "Codex" => "codex",
        other => other,
    }
}

/// `group` is the view group the row sits in; only the Build group's rows have fixes here.
pub fn fixes_for(group: &str, row: &Row) -> Vec<Fix> {
    if row.status != Status::NeedsStep || group != "build" {
        return vec![];
    }
    match row.name.as_str() {
        "Herdr" => vec![
            Fix::Install {
                cmd: INSTALL_HERDR.0,
                url: INSTALL_HERDR.1,
            },
            Fix::Copy(INSTALL_HERDR.0.to_string()),
        ],
        "Coding agent" => vec![Fix::Docs(DOCS_CLAUDE), Fix::Docs(DOCS_CODEX)],
        name => {
            let kind = kind_of(name);
            let docs = match kind {
                "claude" => DOCS_CLAUDE,
                "codex" => DOCS_CODEX,
                _ => DOCS_HERDR,
            };
            let mut v = Vec::new();
            if let Some((_, argv)) = LOGIN.iter().find(|(k, _)| *k == kind) {
                v.push(Fix::Login(argv));
            }
            v.push(Fix::Docs(docs));
            v
        }
    }
}

impl Fix {
    pub fn label(&self) -> String {
        match self {
            Fix::Login(argv) => format!("Log in: runs `{}` here", argv.join(" ")),
            Fix::Install { cmd, .. } => format!("Install: `{cmd}`"),
            Fix::Docs(url) => format!("Docs: {url}"),
            Fix::Copy(text) => format!("Copy `{text}`"),
        }
    }
}

/// An OSC 52 sequence that puts `text` on the clipboard of terminals that allow it.
pub fn osc52(text: &str) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in text.as_bytes().chunks(3) {
        let n = (chunk[0] as u32) << 16
            | (*chunk.get(1).unwrap_or(&0) as u32) << 8
            | *chunk.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    format!("\x1b]52;c;{out}\x07")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(name: &str, detail: &str, status: Status) -> Row {
        Row {
            name: name.into(),
            detail: detail.into(),
            status,
        }
    }

    #[test]
    fn a_logged_out_agent_can_log_in_with_its_own_command() {
        let f = fixes_for(
            "build",
            &row("Claude Code", "Not logged in", Status::NeedsStep),
        );
        assert_eq!(
            f,
            vec![
                Fix::Login(&["claude", "auth", "login"]),
                Fix::Docs(DOCS_CLAUDE)
            ]
        );
        let f = fixes_for("build", &row("Codex", "Not logged in", Status::NeedsStep));
        assert_eq!(f[0], Fix::Login(&["codex", "login"]));
    }

    #[test]
    fn an_agent_without_a_login_entry_gets_only_a_docs_link() {
        let f = fixes_for("build", &row("aider", "Not logged in", Status::NeedsStep));
        assert_eq!(f, vec![Fix::Docs(DOCS_HERDR)]);
    }

    #[test]
    fn a_missing_herdr_offers_exactly_the_documented_install() {
        let f = fixes_for(
            "build",
            &row(
                "Herdr",
                "Not found on this computer. Install Herdr.",
                Status::NeedsStep,
            ),
        );
        assert_eq!(
            f[0],
            Fix::Install {
                cmd: "curl -fsSL https://herdr.dev/install.sh | sh",
                url: "https://herdr.dev"
            }
        );
        assert_eq!(
            INSTALL_HERDR_ARGV,
            ["sh", "-c", "curl -fsSL https://herdr.dev/install.sh | sh"]
        );
    }

    #[test]
    fn only_the_build_group_has_fixes() {
        assert!(fixes_for("chat", &row("Hermes", "Not found", Status::NeedsStep)).is_empty());
    }

    #[test]
    fn rows_that_are_fine_have_no_fixes() {
        assert!(fixes_for("build", &row("Herdr", "Version 0.9.1", Status::Ready)).is_empty());
        assert!(fixes_for("build", &row("Hermes", "x", Status::Optional)).is_empty());
    }

    #[test]
    fn the_clipboard_sequence_is_base64() {
        assert_eq!(osc52("hello"), "\x1b]52;c;aGVsbG8=\x07");
        assert_eq!(osc52("ab"), "\x1b]52;c;YWI=\x07");
        assert_eq!(osc52("abc"), "\x1b]52;c;YWJj\x07");
        assert_eq!(osc52(""), "\x1b]52;c;\x07");
    }
}
