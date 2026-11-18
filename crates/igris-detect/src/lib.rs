//! Detection: what this machine can run. Every probe goes through a `Host`, so tests replay recorded
//! output, and a probe that cannot tell answers `None` (shown as `null`/"unknown") instead of guessing.

mod machine;
mod real;
mod tools;

pub use machine::{Gpu, Machine};
pub use real::RealHost;
use serde::Serialize;
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
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Snapshot {
    pub machine: Machine,
    pub herdr: HerdrInfo,
    pub agents: Vec<Agent>,
}

pub fn detect(host: &dyn Host) -> Snapshot {
    Snapshot {
        machine: machine::probe(host),
        herdr: tools::herdr(host),
        agents: tools::agents(host),
    }
}
