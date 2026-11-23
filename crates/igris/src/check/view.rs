//! The setup page's view of a detection snapshot: plain groups, rows and sentences. Pure, so the
//! page only draws it and every sentence is covered by `cargo test`.

use igris_detect::LocalLlm;
use igris_detect::{Agent, Decision, Hermes, Login, Machine, Snapshot};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Ready,
    NeedsStep,
    Optional,
    /// Shown, but not counted and not a to-do (for example an agent whose login cannot be read).
    Info,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub name: String,
    pub detail: String,
    pub status: Status,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Fact {
    pub label: &'static str,
    pub value: String,
}

/// The folded part of a group: more rows, or small labels such as the ports that were checked.
#[derive(Debug, Clone, PartialEq)]
pub struct More {
    pub title: String,
    pub rows: Vec<Row>,
    pub chips: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    pub id: &'static str,
    pub title: &'static str,
    pub summary: String,
    pub status: Status,
    /// Open only when something here needs a step; everything else folds to its summary line.
    pub open: bool,
    pub facts: Vec<Fact>,
    pub hint: Option<String>,
    pub rows: Vec<Row>,
    pub more: Option<More>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Counts {
    pub ready: usize,
    pub needs_step: usize,
    pub optional: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct View {
    pub lead: String,
    pub counts: Counts,
    pub groups: Vec<Group>,
}

fn row(name: impl Into<String>, detail: impl Into<String>, status: Status) -> Row {
    Row {
        name: name.into(),
        detail: detail.into(),
        status,
    }
}

fn group_status(rows: &[Row]) -> Status {
    let has = |s| rows.iter().any(|r| r.status == s);
    if has(Status::NeedsStep) {
        Status::NeedsStep
    } else if has(Status::Ready) {
        Status::Ready
    } else {
        Status::Optional
    }
}

/// "A", "A and B", "A, B and C".
fn join_names(names: &[&str]) -> String {
    match names {
        [] => String::new(),
        [one] => (*one).to_string(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

fn agent_name(kind: &str) -> String {
    match kind {
        "claude" => "Claude Code".into(),
        "codex" => "Codex".into(),
        other => other.into(),
    }
}

fn local_name(name: &str) -> String {
    match name {
        "ollama" => "Ollama".into(),
        "lmstudio" => "LM Studio".into(),
        "llamacpp" => "llama.cpp".into(),
        "vllm" => "vLLM".into(),
        "jan" => "Jan".into(),
        "koboldcpp" => "KoboldCpp".into(),
        other => other.into(),
    }
}

const HINT_NO_CARD: &str = "No supported graphics card found. Laya runs on the processor; local models will be slow. These hints are approximate.";

fn machine_group(m: &Machine) -> Group {
    let cores = m
        .cpu_cores
        .map(|c| format!("{c} core{}", if c == 1 { "" } else { "s" }));
    let ram = m.ram_gb.map(|g| format!("{g:.1} GiB"));
    let disk = m.disk_free_gb.map(|g| format!("{g:.0} GB"));
    let gpu = m.gpus.as_ref().and_then(|g| g.first());
    let gpu_name = gpu.map(|g| {
        g.name
            .replace("NVIDIA GeForce ", "")
            .replace(" Laptop GPU", "")
            .trim()
            .to_string()
    });
    let gpu_gb = gpu.map(|g| (g.vram_mb as f64 / 1024.0).round() as u64);

    let parts: Vec<String> = [
        cores.clone(),
        ram.as_ref().map(|r| format!("{r} memory")),
        gpu_name
            .as_ref()
            .zip(gpu_gb)
            .map(|(n, g)| format!("{n} ({g} GB)")),
        disk.as_ref().map(|d| format!("{d} free")),
    ]
    .into_iter()
    .flatten()
    .collect();
    let summary = if parts.is_empty() {
        "Details unavailable".to_string()
    } else {
        parts.join(" · ")
    };

    let unknown = || "Unknown".to_string();
    let graphics = match (&gpu_name, gpu_gb, &m.gpus) {
        (Some(n), Some(g), _) => format!("{n}, {g} GB"),
        (_, _, Some(_)) => "None found".to_string(),
        _ => unknown(),
    };
    let hint = match gpu.map(|g| g.vram_mb) {
        Some(mb) if mb < 8192 => format!(
            "With {} GB of graphics memory, only small local models fit. Laya runs fine on the processor. These hints are approximate.",
            gpu_gb.unwrap_or(0)
        ),
        Some(_) => "Enough graphics memory for mid-size local models. These hints are approximate."
            .to_string(),
        None => HINT_NO_CARD.to_string(),
    };

    Group {
        id: "machine",
        title: "Your machine",
        summary,
        status: Status::Info,
        open: false,
        facts: vec![
            Fact {
                label: "Processor",
                value: cores.unwrap_or_else(unknown),
            },
            Fact {
                label: "Memory",
                value: ram.unwrap_or_else(unknown),
            },
            Fact {
                label: "Graphics",
                value: graphics,
            },
            Fact {
                label: "Disk free",
                value: disk.unwrap_or_else(unknown),
            },
        ],
        hint: Some(hint),
        rows: vec![],
        more: None,
    }
}

/// The build group, and the first thing that stops tasks from running (if anything does).
fn build_group(s: &Snapshot) -> (Group, Option<&'static str>) {
    let mut rows = vec![
        match (s.herdr.found, s.herdr.ok, s.herdr.version.as_deref()) {
            (true, true, Some(v)) => row(
                "Herdr",
                format!("Version {v} · runs each agent in its own pane"),
                Status::Ready,
            ),
            (true, true, None) => row("Herdr", "Runs each agent in its own pane", Status::Ready),
            (true, false, Some(v)) => row(
                "Herdr",
                format!("Version {v} is too old. Update Herdr."),
                Status::NeedsStep,
            ),
            (true, false, None) => row(
                "Herdr",
                "This version is too old. Update Herdr.",
                Status::NeedsStep,
            ),
            (false, _, _) => row(
                "Herdr",
                "Not found on this computer. Install Herdr.",
                Status::NeedsStep,
            ),
        },
    ];

    let found: Vec<&Agent> = s.agents.iter().filter(|a| a.found).collect();
    let mut other = vec![];
    for a in &found {
        let name = agent_name(&a.kind);
        match a.login {
            Login::LoggedIn => rows.push(row(
                name,
                a.method
                    .as_ref()
                    .map_or("Logged in".to_string(), |m| format!("Logged in with {m}")),
                Status::Ready,
            )),
            Login::LoggedOut => rows.push(row(name, "Not logged in", Status::NeedsStep)),
            Login::Unknown => {
                let r = row(name, "Found · login state unknown", Status::Info);
                if matches!(a.kind.as_str(), "claude" | "codex") {
                    rows.push(r);
                } else {
                    other.push(r);
                }
            }
        }
    }
    if found.is_empty() {
        rows.push(row(
            "Coding agent",
            "No coding agent found. Install Claude Code or Codex.",
            Status::NeedsStep,
        ));
    }

    // Found agents, but none logged in, and no row says what to do: add the step the lead talks about.
    let agent_needs_step = rows.iter().skip(1).any(|r| r.status == Status::NeedsStep);
    if !found.is_empty() && !found.iter().any(|a| a.login == Login::LoggedIn) && !agent_needs_step {
        rows.push(row(
            "Coding agent",
            "No coding agent is logged in. Log in to Claude Code or Codex.",
            Status::NeedsStep,
        ));
    }

    let blocker = if !s.herdr.found {
        Some("Herdr is missing.")
    } else if !s.herdr.ok {
        Some("Herdr needs an update.")
    } else if found.is_empty() {
        Some("No coding agent was found.")
    } else if !found.iter().any(|a| a.login == Login::LoggedIn) {
        Some("No coding agent is logged in.")
    } else {
        None
    };

    let status = group_status(&rows);
    let summary = match status {
        Status::NeedsStep => {
            let first = rows.iter().find(|r| r.status == Status::NeedsStep).unwrap();
            format!("{} needs a step", first.name)
        }
        _ => {
            let ready: Vec<&str> = rows
                .iter()
                .filter(|r| r.status == Status::Ready)
                .map(|r| r.name.as_str())
                .collect();
            format!(
                "{} {} ready",
                join_names(&ready),
                if ready.len() == 1 { "is" } else { "are" }
            )
        }
    };
    let more = (!other.is_empty()).then(|| More {
        title: format!(
            "{} other agent{} found",
            other.len(),
            if other.len() == 1 { "" } else { "s" }
        ),
        rows: other,
        chips: vec![],
    });
    (
        Group {
            id: "build",
            title: "Build and review",
            summary,
            status,
            open: status == Status::NeedsStep,
            facts: vec![],
            hint: None,
            rows,
            more,
        },
        blocker,
    )
}

fn decision_group(d: &Decision) -> Group {
    let laya = if d.laya.found {
        row(
            "Laya",
            d.laya
                .version
                .as_ref()
                .map_or("Runs on this computer".to_string(), |v| {
                    format!("Version {v} · runs on this computer")
                }),
            Status::Ready,
        )
    } else {
        row(
            "Laya",
            "Not installed. Rules-based decisions still work.",
            Status::Optional,
        )
    };
    let jev = if d.jev.key {
        row(
            "Jev",
            "Key found · task text goes to TypeSafe",
            Status::Ready,
        )
    } else {
        row(
            "Jev",
            "Hosted. Add a key if you want it. Task text then leaves this computer.",
            Status::Optional,
        )
    };
    let summary = match (d.laya.found, d.jev.key) {
        (true, false) => "Laya is ready · Jev is optional",
        (true, true) => "Laya and Jev are ready",
        (false, true) => "Jev is ready · Laya is optional",
        (false, false) => "Rules are ready · Laya and Jev are optional",
    };
    Group {
        id: "decision",
        title: "Decision layer",
        summary: summary.to_string(),
        // Rules-based decisions are always available, so this group never blocks anything.
        status: Status::Ready,
        open: false,
        facts: vec![],
        hint: None,
        rows: vec![laya, jev],
        more: None,
    }
}

fn chat_group(h: &Hermes, local: &[LocalLlm]) -> Group {
    let hermes = if h.found {
        let mut parts = vec![];
        if let Some(v) = &h.version {
            parts.push(format!("Version {v}"));
        }
        if let Some(p) = &h.provider {
            parts.push(format!("provider {p}"));
        }
        if !h.platforms.is_empty() {
            parts.push(format!("chat apps: {}", h.platforms.join(", ")));
        }
        let detail = if parts.is_empty() {
            "Found".to_string()
        } else {
            parts.join(" · ")
        };
        row("Hermes", detail, Status::Ready)
    } else {
        row(
            "Hermes",
            "Takes your requests from a chat app. Not found on this computer.",
            Status::NeedsStep,
        )
    };
    let up: Vec<String> = local
        .iter()
        .filter(|l| l.up)
        .map(|l| match l.models.len() {
            0 => local_name(&l.name),
            n => format!(
                "{} ({n} model{})",
                local_name(&l.name),
                if n == 1 { "" } else { "s" }
            ),
        })
        .collect();
    let servers = if up.is_empty() {
        row("Local model servers", "None running", Status::Optional)
    } else {
        row(
            "Local model servers",
            format!("Running: {}", up.join(", ")),
            Status::Ready,
        )
    };
    let rows = vec![hermes, servers];
    let status = group_status(&rows);
    Group {
        id: "chat",
        title: "Chat and local models",
        summary: if h.found {
            "Hermes is ready".to_string()
        } else {
            "Hermes is not installed".to_string()
        },
        status,
        open: status == Status::NeedsStep,
        facts: vec![],
        hint: None,
        rows,
        more: Some(More {
            title: "Ports checked".into(),
            rows: vec![],
            chips: local
                .iter()
                .map(|l| format!("{} {}", local_name(&l.name), l.port))
                .collect(),
        }),
    }
}

pub fn build(s: &Snapshot) -> View {
    let (build, blocker) = build_group(s);
    let groups = vec![
        machine_group(&s.machine),
        build,
        decision_group(&s.decision),
        chat_group(&s.hermes, &s.local_llm),
    ];
    let count = |st| {
        groups
            .iter()
            .skip(1)
            .flat_map(|g| &g.rows)
            .filter(|r| r.status == st)
            .count()
    };
    let counts = Counts {
        ready: count(Status::Ready),
        needs_step: count(Status::NeedsStep),
        optional: count(Status::Optional),
    };
    let lead = match blocker {
        Some(b) => format!("Not ready to run tasks yet. {b}"),
        None if s.hermes.found => {
            "You can run tasks from the command line now. Hermes is ready for chat.".to_string()
        }
        None => "You can run tasks from the command line now. Install Hermes to take requests from chat."
            .to_string(),
    };
    View {
        lead,
        counts,
        groups,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use igris_detect::{
        Agent, Decision, Gpu, HerdrInfo, Hermes, Jev, Laya, LocalLlm, Login, Machine, Snapshot,
    };

    pub(crate) fn agent(kind: &str, found: bool, login: Login, method: Option<&str>) -> Agent {
        Agent {
            kind: kind.into(),
            found,
            login,
            method: method.map(str::to_string),
            how: "test".into(),
        }
    }

    fn port(name: &str, n: u16) -> LocalLlm {
        LocalLlm {
            name: name.into(),
            port: n,
            up: false,
            models: vec![],
        }
    }

    /// This machine, as `igris doctor --json` reported it on 2026-11-19.
    pub(crate) fn real() -> Snapshot {
        Snapshot {
            machine: Machine {
                os: "linux".into(),
                cpu_cores: Some(12),
                ram_gb: Some(7.2),
                gpus: Some(vec![Gpu {
                    name: "NVIDIA GeForce RTX 3050 Laptop GPU".into(),
                    vram_mb: 4096,
                }]),
                disk_free_gb: Some(387.7),
            },
            herdr: HerdrInfo {
                found: true,
                version: Some("0.9.1".into()),
                ok: true,
            },
            agents: vec![
                agent("pi", true, Login::Unknown, None),
                agent("claude", true, Login::LoggedIn, Some("claude.ai")),
                agent("codex", true, Login::LoggedIn, Some("ChatGPT")),
                agent("omp", true, Login::Unknown, None),
                agent("muse", true, Login::Unknown, None),
                agent("aider", false, Login::Unknown, None),
            ],
            hermes: Hermes {
                found: false,
                version: None,
                provider: None,
                platforms: vec![],
            },
            decision: Decision {
                laya: Laya {
                    found: true,
                    version: Some("0.4.35".into()),
                    location: Some("igris".into()),
                },
                jev: Jev { key: false },
            },
            local_llm: vec![
                port("ollama", 11434),
                port("lmstudio", 1234),
                port("llamacpp", 8080),
                port("vllm", 8000),
                port("jan", 1337),
                port("koboldcpp", 5001),
            ],
        }
    }

    fn group<'a>(v: &'a View, id: &str) -> &'a Group {
        v.groups.iter().find(|g| g.id == id).expect(id)
    }

    fn row<'a>(g: &'a Group, name: &str) -> &'a Row {
        g.rows.iter().find(|r| r.name == name).expect(name)
    }

    #[test]
    fn real_machine_counts_and_lead() {
        let v = build(&real());
        assert_eq!(
            (v.counts.ready, v.counts.needs_step, v.counts.optional),
            (4, 1, 2)
        );
        assert_eq!(
            v.lead,
            "You can run tasks from the command line now. Install Hermes to take requests from chat."
        );
    }

    #[test]
    fn groups_in_order_and_only_the_one_needing_a_step_is_open() {
        let v = build(&real());
        let ids: Vec<_> = v.groups.iter().map(|g| g.id).collect();
        assert_eq!(ids, ["machine", "build", "decision", "chat"]);
        let open: Vec<_> = v.groups.iter().map(|g| g.open).collect();
        assert_eq!(open, [false, false, false, true]);
    }

    #[test]
    fn machine_summary_facts_and_hint() {
        let v = build(&real());
        let g = group(&v, "machine");
        assert_eq!(
            g.summary,
            "12 cores · 7.2 GiB memory · RTX 3050 (4 GB) · 388 GB free"
        );
        let facts: Vec<_> = g
            .facts
            .iter()
            .map(|f| (f.label, f.value.as_str()))
            .collect();
        assert_eq!(
            facts,
            [
                ("Processor", "12 cores"),
                ("Memory", "7.2 GiB"),
                ("Graphics", "RTX 3050, 4 GB"),
                ("Disk free", "388 GB"),
            ]
        );
        let hint = g.hint.as_deref().unwrap();
        assert!(hint.contains("only small local models fit") && hint.contains("approximate"));
    }

    #[test]
    fn build_group() {
        let v = build(&real());
        let g = group(&v, "build");
        assert_eq!(g.summary, "Herdr, Claude Code and Codex are ready");
        assert_eq!(g.status, Status::Ready);
        let herdr = row(g, "Herdr");
        assert_eq!(herdr.status, Status::Ready);
        assert_eq!(
            herdr.detail,
            "Version 0.9.1 · runs each agent in its own pane"
        );
        assert_eq!(row(g, "Claude Code").detail, "Logged in with claude.ai");
        assert_eq!(row(g, "Codex").detail, "Logged in with ChatGPT");
        assert_eq!(g.rows.len(), 3);
        let more = g.more.as_ref().unwrap();
        assert_eq!(more.title, "3 other agents found");
        let names: Vec<_> = more.rows.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["pi", "omp", "muse"]);
        assert!(
            more.rows
                .iter()
                .all(|r| r.status == Status::Info && r.detail == "Found · login state unknown")
        );
    }

    #[test]
    fn decision_group() {
        let v = build(&real());
        let g = group(&v, "decision");
        assert_eq!(g.status, Status::Ready);
        assert_eq!(g.summary, "Laya is ready · Jev is optional");
        let laya = row(g, "Laya");
        assert_eq!(
            (laya.status, laya.detail.as_str()),
            (Status::Ready, "Version 0.4.35 · runs on this computer")
        );
        let jev = row(g, "Jev");
        assert_eq!(
            (jev.status, jev.detail.as_str()),
            (
                Status::Optional,
                "Hosted. Add a key if you want it. Task text then leaves this computer."
            )
        );

        let mut s = real();
        s.decision.laya.found = false;
        let g = &build(&s).groups[2];
        assert_eq!(g.status, Status::Ready, "rules are always available");
        assert_eq!(g.summary, "Rules are ready · Laya and Jev are optional");
    }

    #[test]
    fn chat_group() {
        let v = build(&real());
        let g = group(&v, "chat");
        assert_eq!(g.status, Status::NeedsStep);
        assert_eq!(g.summary, "Hermes is not installed");
        let hermes = row(g, "Hermes");
        assert_eq!(hermes.status, Status::NeedsStep);
        assert_eq!(
            hermes.detail,
            "Takes your requests from a chat app. Not found on this computer."
        );
        let local = row(g, "Local model servers");
        assert_eq!(
            (local.status, local.detail.as_str()),
            (Status::Optional, "None running")
        );
        let chips = &g.more.as_ref().unwrap().chips;
        assert_eq!(
            chips,
            &[
                "Ollama 11434",
                "LM Studio 1234",
                "llama.cpp 8080",
                "vLLM 8000",
                "Jan 1337",
                "KoboldCpp 5001"
            ]
        );
    }

    #[test]
    fn hermes_present_changes_the_lead() {
        let mut s = real();
        s.hermes = Hermes {
            found: true,
            version: Some("1.2.3".into()),
            provider: Some("openai".into()),
            platforms: vec!["telegram".into()],
        };
        let v = build(&s);
        assert!(v.lead.ends_with("Hermes is ready for chat."), "{}", v.lead);
        let hermes = row(group(&v, "chat"), "Hermes");
        assert_eq!(hermes.status, Status::Ready);
        assert_eq!(
            hermes.detail,
            "Version 1.2.3 · provider openai · chat apps: telegram"
        );
    }

    #[test]
    fn no_agent_blocks_running_tasks() {
        let mut s = real();
        for a in &mut s.agents {
            a.found = false;
        }
        let v = build(&s);
        assert!(
            v.lead.starts_with("Not ready to run tasks yet."),
            "{}",
            v.lead
        );
        let g = group(&v, "build");
        assert!(g.open);
        let r = row(g, "Coding agent");
        assert_eq!(r.status, Status::NeedsStep);
        assert_eq!(
            r.detail,
            "No coding agent found. Install Claude Code or Codex."
        );
    }

    #[test]
    fn logged_out_agent_needs_a_step() {
        let mut s = real();
        s.agents[1] = agent("claude", true, Login::LoggedOut, None);
        let v = build(&s);
        let r = row(group(&v, "build"), "Claude Code");
        assert_eq!(
            (r.status, r.detail.as_str()),
            (Status::NeedsStep, "Not logged in")
        );
    }

    #[test]
    fn unknown_values_never_panic() {
        let mut s = real();
        s.machine = Machine {
            os: "plan9".into(),
            cpu_cores: None,
            ram_gb: None,
            gpus: None,
            disk_free_gb: None,
        };
        s.agents.clear();
        s.herdr = HerdrInfo {
            found: false,
            version: None,
            ok: false,
        };
        let v = build(&s);
        let g = group(&v, "machine");
        assert_eq!(g.summary, "Details unavailable");
        assert!(g.facts.iter().all(|f| f.value == "Unknown"));
        assert_eq!(
            g.hint.as_deref(),
            Some(
                "No supported graphics card found. Laya runs on the processor; local models will be slow. These hints are approximate."
            )
        );
        assert_eq!(row(group(&v, "build"), "Herdr").status, Status::NeedsStep);
    }

    /// A lead that says "not ready" must come with a row to act on and a count to match.
    fn assert_login_step(s: &Snapshot) {
        let v = build(s);
        assert!(
            v.lead
                .starts_with("Not ready to run tasks yet. No coding agent is logged in."),
            "{}",
            v.lead
        );
        let g = group(&v, "build");
        assert!(g.open && g.status == Status::NeedsStep);
        let r = row(g, "Coding agent");
        assert_eq!(r.status, Status::NeedsStep);
        assert_eq!(
            r.detail,
            "No coding agent is logged in. Log in to Claude Code or Codex."
        );
        assert_eq!(v.counts.needs_step, 2, "this step and Hermes");
    }

    #[test]
    fn found_but_not_logged_in_needs_a_step() {
        // Only `pi`, whose login can never be read.
        let mut s = real();
        for a in &mut s.agents {
            a.found = a.kind == "pi";
        }
        assert_login_step(&s);
        // Claude Code found, but its login state could not be read.
        let mut s = real();
        for a in &mut s.agents {
            a.found = a.kind == "claude";
        }
        s.agents[1].login = Login::Unknown;
        assert_login_step(&s);
    }

    #[test]
    fn a_logged_out_agent_is_not_listed_twice() {
        let mut s = real();
        s.agents[1] = agent("claude", true, Login::LoggedOut, None);
        s.agents[2] = agent("codex", false, Login::Unknown, None);
        let v = build(&s);
        let g = group(&v, "build");
        assert!(g.rows.iter().all(|r| r.name != "Coding agent"));
        assert_eq!(row(g, "Claude Code").status, Status::NeedsStep);
    }
}
