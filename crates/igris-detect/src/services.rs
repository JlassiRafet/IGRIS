use crate::{Host, Options};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LocalLlm {
    pub name: String,
    pub port: u16,
    pub up: bool,
    pub models: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Laya {
    pub found: bool,
    pub version: Option<String>,
    /// `igris` (IGRIS's own install folder) or `system`.
    pub location: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Jev {
    pub key: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Decision {
    pub laya: Laya,
    pub jev: Jev,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Hermes {
    pub found: bool,
    pub version: Option<String>,
    pub provider: Option<String>,
    pub platforms: Vec<String>,
}

enum Api {
    Ollama,
    OpenAi,
    Kobold,
}

/// The well-known local servers and where they answer (default ports; others via `Options`).
const SERVERS: [(&str, u16, &str, Api); 6] = [
    ("ollama", 11434, "/api/tags", Api::Ollama),
    ("lmstudio", 1234, "/v1/models", Api::OpenAi),
    ("llamacpp", 8080, "/v1/models", Api::OpenAi),
    ("vllm", 8000, "/v1/models", Api::OpenAi),
    ("jan", 1337, "/v1/models", Api::OpenAi),
    ("koboldcpp", 5001, "/api/v1/model", Api::Kobold),
];

fn names(v: &serde_json::Value, list: &str, key: &str) -> Vec<String> {
    v[list]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|m| m[key].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn probe_server(host: &dyn Host, name: &str, port: u16, path: &str, api: &Api) -> LocalLlm {
    let body = host.http_get(port, path);
    let json = body
        .as_deref()
        .and_then(|b| serde_json::from_str::<serde_json::Value>(b).ok());
    let models = json.map_or_else(Vec::new, |v| match api {
        Api::Ollama => names(&v, "models", "name"),
        Api::OpenAi => names(&v, "data", "id"),
        Api::Kobold => v["result"]
            .as_str()
            .map(str::to_string)
            .into_iter()
            .collect(),
    });
    LocalLlm {
        name: name.to_string(),
        port,
        up: body.is_some(),
        models,
    }
}

pub fn local_llm(host: &dyn Host, opts: &Options) -> Vec<LocalLlm> {
    let mut out: Vec<LocalLlm> = SERVERS
        .iter()
        .map(|(n, p, path, api)| probe_server(host, n, *p, path, api))
        .collect();
    out.extend(
        opts.extra_llm_ports
            .iter()
            .map(|(n, p)| probe_server(host, n, *p, "/v1/models", &Api::OpenAi)),
    );
    out
}

/// `importlib.metadata` reads the version without importing torch, so the probe stays fast.
const LAYA_VERSION: &str = "import importlib.metadata as m;print(m.version('laya'))";

fn laya(host: &dyn Host) -> Laya {
    let own = host
        .home()
        .map(|h| format!("{h}/.local/share/igris/laya/venv/bin/python"));
    let candidates = own
        .iter()
        .map(|p| (p.as_str(), "igris"))
        .chain([("python3", "system")]);
    for (python, location) in candidates {
        if let Some(v) = host
            .run(python, &["-c", LAYA_VERSION])
            .map(|o| o.trim().to_string())
            .filter(|v| !v.is_empty())
        {
            return Laya {
                found: true,
                version: Some(v),
                location: Some(location.to_string()),
            };
        }
    }
    Laya {
        found: false,
        version: None,
        location: None,
    }
}

pub fn decision(host: &dyn Host) -> Decision {
    Decision {
        laya: laya(host),
        // Whether the key is set is the only thing ever reported, never its value or name.
        jev: Jev {
            key: host.has_env("TYPESAFE_API_KEY"),
        },
    }
}

/// The first token that looks like `1.2.3`, with an optional leading `v`.
fn find_version(text: &str) -> Option<String> {
    text.split_whitespace()
        .map(|t| t.trim_start_matches('v'))
        .find(|t| {
            let parts: Vec<_> = t.split('.').collect();
            parts.len() == 3 && parts.iter().all(|p| p.parse::<u32>().is_ok())
        })
        .map(str::to_string)
}

/// `model:` block, indented `provider:` line. Not a YAML parser, on purpose: it reads this one key.
fn provider_of(config: &str) -> Option<String> {
    let mut in_model = false;
    for line in config.lines() {
        if !line.starts_with([' ', '\t']) && !line.trim().is_empty() {
            in_model = line.trim_end() == "model:";
        } else if in_model && let Some(v) = line.trim().strip_prefix("provider:") {
            let v = v.trim().trim_matches(['"', '\'']);
            return (!v.is_empty()).then(|| v.to_string());
        }
    }
    None
}

/// Chat apps that have a bot token set, by name only. Values are never kept.
fn platforms_of(env_file: &str) -> Vec<String> {
    let mut found: Vec<String> = env_file
        .lines()
        .filter_map(|l| {
            let (key, value) = l.split_once('=')?;
            let name = key.trim().strip_suffix("_BOT_TOKEN")?;
            (!value.trim().is_empty()).then(|| name.to_lowercase())
        })
        .collect();
    found.sort();
    found.dedup();
    found
}

pub fn hermes(host: &dyn Host) -> Hermes {
    let found = host.which("hermes").is_some();
    let dir = host.home().map(|h| format!("{h}/.hermes"));
    let read = |name: &str| dir.as_ref().and_then(|d| host.read(&format!("{d}/{name}")));
    Hermes {
        found,
        version: found
            .then(|| host.run("hermes", &["--version"]))
            .flatten()
            .and_then(|o| find_version(&o)),
        provider: read("config.yaml").and_then(|c| provider_of(&c)),
        platforms: read(".env").map(|e| platforms_of(&e)).unwrap_or_default(),
    }
}
