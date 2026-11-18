//! Test support: a fake `herdr` that records calls and replays recorded output.
#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

pub const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/herdr");
pub const SCRIPT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fake-herdr.sh");

pub struct FakeHerdr {
    pub dir: PathBuf,
}

impl FakeHerdr {
    pub fn new() -> Self {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("igris-fake-herdr-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("calls")).unwrap();
        fs::create_dir_all(dir.join("queue")).unwrap();
        Self { dir }
    }

    /// The path to hand to code that expects a `herdr` binary.
    pub fn bin(&self) -> PathBuf {
        let p = self.dir.join("herdr");
        // A symlink, not a copy: writing an executable while other tests fork gives "text file busy".
        if !p.exists() {
            std::os::unix::fs::symlink(SCRIPT, &p).expect("link fake-herdr.sh");
        }
        p
    }

    fn put(&self, key: &str, ext: &str, fixture: &str, queued: bool) {
        let body = fs::read(Path::new(FIXTURES).join(fixture)).expect("fixture");
        let target = if queued {
            let n = fs::read_dir(self.dir.join("queue")).unwrap().count();
            self.dir.join("queue").join(format!("{key}.{n:04}.{ext}"))
        } else {
            self.dir.join(format!("{key}.{ext}"))
        };
        fs::write(target, body).unwrap();
    }

    /// `key` is the first two non-flag arguments joined by `-`, e.g. `pane-split`.
    pub fn on(&self, key: &str, fixture: &str) -> &Self {
        self.put(key, "out", fixture, false);
        self
    }
    pub fn fail(&self, key: &str, fixture: &str) -> &Self {
        self.put(key, "err", fixture, false);
        self
    }
    pub fn queue_ok(&self, key: &str, fixture: &str) -> &Self {
        self.put(key, "out", fixture, true);
        self
    }
    pub fn queue_fail(&self, key: &str, fixture: &str) -> &Self {
        self.put(key, "err", fixture, true);
        self
    }

    pub fn run(&self, args: &[&str]) -> Output {
        Command::new(self.bin())
            .args(args)
            .env("IGRIS_FAKE_HERDR", &self.dir)
            .output()
            .expect("run fake herdr")
    }

    /// Every call so far, in order, as argument lists.
    pub fn calls(&self) -> Vec<Vec<String>> {
        let mut files: Vec<_> = fs::read_dir(self.dir.join("calls"))
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        files.sort();
        files
            .iter()
            .map(|f| {
                fs::read(f)
                    .unwrap()
                    .split(|b| *b == 0)
                    .filter(|s| !s.is_empty())
                    .map(|s| String::from_utf8_lossy(s).into_owned())
                    .collect()
            })
            .collect()
    }
}

impl FakeHerdr {
    /// Stdout (exit 0) with literal text instead of a fixture file.
    pub fn on_raw(&self, key: &str, text: &str) -> &Self {
        fs::write(self.dir.join(format!("{key}.out")), text).unwrap();
        self
    }
    /// Stderr (exit 1) with literal text instead of a fixture file.
    pub fn fail_raw(&self, key: &str, text: &str) -> &Self {
        fs::write(self.dir.join(format!("{key}.err")), text).unwrap();
        self
    }
}
