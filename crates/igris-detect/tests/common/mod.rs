//! A fake machine: canned command output and files, so probes are tested against recorded text.
#![allow(dead_code)]
use igris_detect::Host;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

pub fn fixture(name: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    fs::read_to_string(&p).unwrap_or_else(|e| panic!("fixture {}: {e}", p.display()))
}

#[derive(Default)]
pub struct FakeHost {
    pub os: String,
    pub home: Option<String>,
    pub cores: Option<u32>,
    commands: HashMap<String, String>,
    files: HashMap<String, String>,
    bins: Vec<String>,
    envs: Vec<String>,
    http: HashMap<(u16, String), String>,
}

impl FakeHost {
    pub fn linux() -> Self {
        Self {
            os: "linux".into(),
            home: Some("/home/user".into()),
            ..Self::default()
        }
    }
    pub fn cmd(mut self, line: &str, out: &str) -> Self {
        self.commands.insert(line.into(), out.into());
        self
    }
    pub fn bin(mut self, name: &str) -> Self {
        self.bins.push(name.into());
        self
    }
    pub fn env(mut self, name: &str) -> Self {
        self.envs.push(name.into());
        self
    }
    pub fn http(mut self, port: u16, path: &str, body: &str) -> Self {
        self.http.insert((port, path.into()), body.into());
        self
    }
    pub fn file(mut self, path: &str, body: &str) -> Self {
        self.files.insert(path.into(), body.into());
        self
    }
}

impl Host for FakeHost {
    fn os(&self) -> &str {
        &self.os
    }
    fn home(&self) -> Option<String> {
        self.home.clone()
    }
    fn cpu_cores(&self) -> Option<u32> {
        self.cores
    }
    fn run(&self, cmd: &str, args: &[&str]) -> Option<String> {
        self.commands
            .get(&format!("{cmd} {}", args.join(" ")))
            .cloned()
    }
    fn read(&self, path: &str) -> Option<String> {
        self.files.get(path).cloned()
    }
    fn has_env(&self, name: &str) -> bool {
        self.envs.iter().any(|e| e == name)
    }
    fn http_get(&self, port: u16, path: &str) -> Option<String> {
        self.http.get(&(port, path.to_string())).cloned()
    }
    fn which(&self, name: &str) -> Option<String> {
        self.bins
            .iter()
            .any(|b| b == name)
            .then(|| format!("/usr/bin/{name}"))
    }
}
