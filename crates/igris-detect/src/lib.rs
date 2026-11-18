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
    Snapshot {
        machine: machine::probe(host),
        herdr: tools::herdr(host),
        agents: tools::agents(host),
        hermes: services::hermes(host),
        decision: services::decision(host),
        local_llm: services::local_llm(host, opts),
    }
}
