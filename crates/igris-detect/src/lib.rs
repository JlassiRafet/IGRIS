//! Detection: what this machine can run. Every probe goes through a `Host`, so tests replay recorded
//! output, and a probe that cannot tell answers `None` (shown as `null`/"unknown") instead of guessing.

mod machine;
mod real;
mod services;
mod tools;

pub use machine::{Gpu, Machine};
pub use real::RealHost;
use serde::Serialize;
pub use services::{Decision, Hermes, Jev, Laya, LocalLlm};
pub use tools::{Agent, HerdrInfo, Login};

/// Everything a probe may ask of the machine. Commands run with a timeout and give `None` unless
/// they exit 0; no probe ever prints a secret.
pub trait Host {
    fn os(&self) -> &str;
    fn home(&self) -> Option<String>;
    fn cpu_cores(&self) -> Option<u32>;
    /// Stdout of `cmd args`, or `None` if it is missing, fails or times out.
    fn run(&self, cmd: &str, args: &[&str]) -> Option<String>;
    /// Like `run`, but stderr is appended to stdout. For tools that print their status there.
    fn run_with_stderr(&self, cmd: &str, args: &[&str]) -> Option<String>;
    fn read(&self, path: &str) -> Option<String>;
    /// Path of an executable on `PATH`.
    fn which(&self, name: &str) -> Option<String>;
    /// True when the variable is set and not empty. The value is never exposed.
    fn has_env(&self, name: &str) -> bool;
    /// Body of `GET http://127.0.0.1:<port><path>` when it answers 200 within 300 ms.
    fn http_get(&self, port: u16, path: &str) -> Option<String>;
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Snapshot {
    pub machine: Machine,
    pub herdr: HerdrInfo,
    pub agents: Vec<Agent>,
    pub hermes: Hermes,
    pub decision: Decision,
    pub local_llm: Vec<LocalLlm>,
}

#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Extra local servers to look for, as (name, port), besides the well-known ones.
    pub extra_llm_ports: Vec<(String, u16)>,
}

pub fn detect(host: &dyn Host) -> Snapshot {
    detect_with(host, &Options::default())
}

pub fn detect_with(host: &dyn Host, opts: &Options) -> Snapshot {
    detect_with_progress(host, opts, &mut |_, _, _| {})
}

/// Like `detect_with`, and reports `(done, total, what is next)` before each of the six probes, so
/// a screen can show real progress. The probes and their order are the same.
pub fn detect_with_progress(
    host: &dyn Host,
    opts: &Options,
    progress: &mut dyn FnMut(usize, usize, &'static str),
) -> Snapshot {
    const TOTAL: usize = 6;
    progress(0, TOTAL, "machine");
    let machine = machine::probe(host);
    progress(1, TOTAL, "herdr");
    let herdr = tools::herdr(host);
    progress(2, TOTAL, "agents");
    let agents = tools::agents(host);
    progress(3, TOTAL, "hermes");
    let hermes = services::hermes(host);
    progress(4, TOTAL, "decision layer");
    let decision = services::decision(host);
    progress(5, TOTAL, "local models");
    let local_llm = services::local_llm(host, opts);
    progress(TOTAL, TOTAL, "done");
    Snapshot {
        machine,
        herdr,
        agents,
        hermes,
        decision,
        local_llm,
    }
}
