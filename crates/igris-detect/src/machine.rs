use crate::Host;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Gpu {
    pub name: String,
    pub vram_mb: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Machine {
    pub os: String,
    pub cpu_cores: Option<u32>,
    pub ram_gb: Option<f64>,
    /// `None` means unknown (no tool to ask), `Some(vec![])` means none found.
    pub gpus: Option<Vec<Gpu>>,
    pub disk_free_gb: Option<f64>,
}

pub fn probe(host: &dyn Host) -> Machine {
    Machine {
        os: host.os().to_string(),
        cpu_cores: host.cpu_cores(),
        ram_gb: ram_gb(host),
        gpus: gpus(host),
        disk_free_gb: disk_free_gb(host),
    }
}

fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

fn ram_gb(host: &dyn Host) -> Option<f64> {
    if host.os() == "macos" {
        let bytes: f64 = host
            .run("sysctl", &["-n", "hw.memsize"])?
            .trim()
            .parse()
            .ok()?;
        return Some(round1(bytes / (1u64 << 30) as f64));
    }
    let meminfo = host.read("/proc/meminfo")?;
    let line = meminfo.lines().find(|l| l.starts_with("MemTotal:"))?;
    let kb: f64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(round1(kb / (1u64 << 20) as f64))
}

/// Only NVIDIA is asked for now; without `nvidia-smi` the answer is "unknown", not "none".
fn gpus(host: &dyn Host) -> Option<Vec<Gpu>> {
    let out = host.run(
        "nvidia-smi",
        &[
            "--query-gpu=name,memory.total",
            "--format=csv,noheader,nounits",
        ],
    )?;
    out.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let (name, mb) = l.rsplit_once(',')?;
            Some(Gpu {
                name: name.trim().to_string(),
                vram_mb: mb.trim().parse().ok()?,
            })
        })
        .collect()
}

fn disk_free_gb(host: &dyn Host) -> Option<f64> {
    let out = host.run("df", &["-Pk", &host.home()?])?;
    let kb: f64 = out
        .lines()
        .nth(1)?
        .split_whitespace()
        .nth(3)?
        .parse()
        .ok()?;
    Some(round1(kb / (1u64 << 20) as f64))
}
