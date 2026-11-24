//! The catalog table. Only Claude Code, Codex, Herdr, Hermes, OpenClaw and llmfit carry commands,
//! each checked against the tool's own `--help` or a real run on 2026-11-24 (see `verified`).
//! Every other entry is a name, with a docs link left empty until someone reads the tool's site:
//! a link nobody read is a guess.
use super::{Group, Method, MethodKind, Tool, Verified};
use crate::tui::fix::INSTALL_HERDR_ARGV;

const DOCS_CLAUDE: &str = "https://docs.anthropic.com/en/docs/claude-code/overview";
const DOCS_CODEX: &str = "https://developers.openai.com/codex";
const DOCS_HERDR: &str = "https://herdr.dev";
const DOCS_HERMES: &str = "https://hermes-agent.nousresearch.com";
const DOCS_OPENCLAW: &str = "https://docs.openclaw.ai";
const DOCS_LLMFIT: &str = "https://github.com/AlexsJones/llmfit";

const DATE: &str = "2026-11-24";
const PING: &str = "Reply with the single word ok.";

/// An agent kind Herdr can start: builder and reviewer, nothing else known about it yet.
const fn agent(id: &'static str, name: &'static str) -> Tool {
    Tool {
        id,
        name,
        groups: &[Group::Builder, Group::Reviewer],
        install: None,
        install_needs_admin: false,
        check: None,
        methods: &[],
        docs: "",
        connects: "",
        selectable: true,
        verified: None,
    }
}

/// A tool that is listed by name only: no commands, no link, not selectable.
const fn listed(id: &'static str, name: &'static str, groups: &'static [Group]) -> Tool {
    Tool {
        selectable: false,
        groups,
        ..agent(id, name)
    }
}

const fn existing(hint: &'static str) -> Method {
    Method {
        id: "existing",
        label: "Existing login",
        hint,
        kind: MethodKind::Existing,
        login: None,
        env: None,
        key_stdin: None,
        docs: "",
    }
}

pub static TOOLS: &[Tool] = &[
    // Workspace manager.
    Tool {
        groups: &[Group::Workspace],
        install: Some(INSTALL_HERDR_ARGV),
        check: Some(&["herdr", "--version"]),
        docs: DOCS_HERDR,
        connects: "one install command, no login",
        verified: Some(Verified {
            with: "herdr --help; the documented installer (herdr.dev answered 200 on 2026-11-23, not run on Windows)",
            version: "0.9.1",
            date: DATE,
        }),
        ..agent("herdr", "Herdr")
    },
    listed("orca", "Orca", &[Group::Workspace]),
    // Agents: the 24 kinds Herdr can start, in `config::KINDS` order.
    agent("pi", "Pi"),
    Tool {
        check: Some(&["claude", "-p", PING, "--no-session-persistence"]),
        methods: &[
            existing("detected on this machine, nothing to do"),
            Method {
                id: "login",
                label: "Log in",
                hint: "runs `claude auth login` here, then checks",
                kind: MethodKind::Login,
                login: Some(&["claude", "auth", "login"]),
                env: None,
                key_stdin: None,
                docs: DOCS_CLAUDE,
            },
            Method {
                id: "setup-token",
                label: "Setup token",
                hint: "a long-lived token for a subscription, made by `claude setup-token`",
                kind: MethodKind::SetupToken,
                login: Some(&["claude", "setup-token"]),
                env: None,
                key_stdin: None,
                docs: DOCS_CLAUDE,
            },
            Method {
                id: "api-key",
                label: "API key",
                hint: "set ANTHROPIC_API_KEY in your shell, then re-check",
                kind: MethodKind::Key,
                login: None,
                env: Some("ANTHROPIC_API_KEY"),
                key_stdin: None,
                docs: DOCS_CLAUDE,
            },
        ],
        docs: DOCS_CLAUDE,
        connects: "subscription login or API key",
        verified: Some(Verified {
            with: "claude --help, claude auth --help, claude setup-token --help; one real `claude -p` call",
            version: "2.1.331",
            date: DATE,
        }),
        ..agent("claude", "Claude Code")
    },
    Tool {
        check: Some(&[
            "codex",
            "exec",
            "--skip-git-repo-check",
            "--ephemeral",
            PING,
        ]),
        methods: &[
            existing("detected on this machine, nothing to do"),
            Method {
                id: "login",
                label: "Log in",
                hint: "runs `codex login` here (ChatGPT sign-in), then checks",
                kind: MethodKind::Login,
                login: Some(&["codex", "login"]),
                env: None,
                key_stdin: None,
                docs: DOCS_CODEX,
            },
            Method {
                id: "api-key",
                label: "API key",
                hint: "Codex stores the key itself: paste it here, or set OPENAI_API_KEY",
                kind: MethodKind::Key,
                login: None,
                env: Some("OPENAI_API_KEY"),
                key_stdin: Some(&["codex", "login", "--with-api-key"]),
                docs: DOCS_CODEX,
            },
        ],
        docs: DOCS_CODEX,
        connects: "ChatGPT login or API key",
        verified: Some(Verified {
            with: "codex login --help, codex exec --help; a logged-out `codex exec` run",
            version: "0.171.0",
            date: DATE,
        }),
        ..agent("codex", "Codex")
    },
    agent("gemini", "Gemini CLI"),
    agent("cursor", "Cursor"),
    agent("devin", "Devin"),
    agent("agy", "agy"),
    agent("cline", "Cline"),
    agent("omp", "omp"),
    agent("mastracode", "mastracode"),
    agent("opencode", "OpenCode"),
    agent("copilot", "Copilot"),
    agent("kimi", "Kimi"),
    agent("kiro", "Kiro"),
    agent("droid", "droid"),
    agent("amp", "Amp"),
    agent("grok", "Grok"),
    // Hermes is both an agent kind and an assistant.
    Tool {
        groups: &[Group::Builder, Group::Reviewer, Group::Assistant],
        check: Some(&["hermes", "status"]),
        methods: &[
            existing("already set up on this machine"),
            Method {
                id: "setup",
                label: "Set up",
                hint: "runs Hermes's own setup wizard here, then checks",
                kind: MethodKind::Login,
                login: Some(&["hermes", "setup"]),
                env: None,
                key_stdin: None,
                docs: DOCS_HERMES,
            },
        ],
        docs: DOCS_HERMES,
        connects: "its own setup wizard, then a chat app",
        verified: Some(Verified {
            with: "hermes --help, hermes status (exit 0 and read-only); `hermes doctor` exits 1 for an optional missing key, so it is not the check",
            version: "0.23.3",
            date: DATE,
        }),
        ..agent("hermes", "Hermes")
    },
    agent("kilo", "kilo"),
    agent("qodercli", "qodercli"),
    agent("qwen", "Qwen"),
    agent("letta", "Letta"),
    agent("maki", "maki"),
    agent("muse", "muse"),
    // Decision layer: `rules` is built in; Jev and Laya wait for their backends.
    Tool {
        groups: &[Group::Decision],
        connects: "built in, always ready",
        ..agent("rules", "Rules")
    },
    Tool {
        methods: &[Method {
            id: "api-key",
            label: "API key",
            hint: "set TYPESAFE_API_KEY in your shell",
            kind: MethodKind::Key,
            login: None,
            env: Some("TYPESAFE_API_KEY"),
            key_stdin: None,
            docs: "",
        }],
        ..listed("jev", "Jev", &[Group::Decision])
    },
    listed("laya", "Laya", &[Group::Decision]),
    // Assistants: Hermes is above; OpenClaw has a verified handoff; the rest are listed by name.
    Tool {
        methods: &[
            existing("already set up on this machine"),
            Method {
                id: "onboard",
                label: "Set up",
                hint: "runs OpenClaw's own onboarding here, then checks",
                kind: MethodKind::Login,
                login: Some(&["openclaw", "onboard"]),
                env: None,
                key_stdin: None,
                docs: DOCS_OPENCLAW,
            },
        ],
        docs: DOCS_OPENCLAW,
        connects: "its own onboarding, then a chat app",
        selectable: true,
        verified: Some(Verified {
            with: "`openclaw onboard` driven through its wizard in a sandbox",
            version: "2026.11.8",
            date: DATE,
        }),
        ..listed("openclaw", "OpenClaw", &[Group::Assistant])
    },
    listed("zeroclaw", "ZeroClaw", &[Group::Assistant]),
    listed("nanoclaw", "NanoClaw", &[Group::Assistant]),
    listed("ironclaw", "IronClaw", &[Group::Assistant]),
    // The external hardware-fit tool: in no group, used by the fit adapter.
    Tool {
        groups: &[],
        check: Some(&["llmfit", "--version"]),
        docs: DOCS_LLMFIT,
        connects: "optional, for local-model fit",
        verified: Some(Verified {
            with: "llmfit --version, llmfit info --json; the release checksum",
            version: "1.1.21",
            date: DATE,
        }),
        ..agent("llmfit", "llmfit")
    },
];
