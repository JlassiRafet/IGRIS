//! The optional sample task: one tiny task on a throwaway repository, through the normal
//! orchestrator, so the user sees the whole pipeline work once.
use igris_core::config::Config;
use igris_core::decide::igris_home;
use igris_core::orchestrate::Orchestrator;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const TASK: &str = "Create a file named HELLO.txt that contains the word hello.";
/// Fixed, not taken from the user's config: it only has to prove the file exists.
pub const CHECK: &str = "test -f HELLO.txt";

/// Removes the throwaway repository when dropped, on success, error and panic alike.
struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn git(dir: &Path, args: &[&str]) -> Result<(), Box<dyn Error>> {
    let out = Command::new("git")
        .args([
            "-c",
            "user.name=igris",
            "-c",
            "user.email=igris@example.invalid",
        ])
        .args(args)
        .current_dir(dir)
        .output()?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        )
        .into())
    }
}

/// Make a throwaway repository under `root` (a `main` branch with one commit), hand it and a copy of
/// the config to `run`, and remove the repository afterwards.
pub fn sample_task_with<R>(
    root: &Path,
    cfg: &Config,
    run: impl FnOnce(&Path, &Config) -> Result<R, Box<dyn Error>>,
) -> Result<R, Box<dyn Error>> {
    let dir = root.join(format!("igris-sample-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    let scratch = Scratch(dir);
    let repo = &scratch.0;
    git(repo, &["init", "-q", "-b", "main"])?;
    std::fs::write(
        repo.join("README.md"),
        "A throwaway repository for the igris sample task.\n",
    )?;
    git(repo, &["add", "README.md"])?;
    git(repo, &["commit", "-q", "-m", "Initial commit"])?;
    let mut sample = cfg.clone();
    sample.checks.command = CHECK.into();
    sample.policy.base = "main".into();
    run(repo, &sample)
}

/// The real thing: the user's own agents, the user's own config, one tiny task.
pub fn run_sample_in(repo: &Path) -> Result<String, String> {
    let cfg = Config::load(repo, None).map_err(|e| e.to_string())?;
    sample_task_with(&std::env::temp_dir(), &cfg, |scratch, cfg| {
        let o = Orchestrator {
            herdr: crate::herdr_of(cfg),
            cfg,
            home: igris_home(),
        };
        let r = o.run(scratch, TASK, false)?;
        let said = if r.next.is_empty() {
            &r.reason
        } else {
            &r.next
        };
        Ok(format!("{:?}: {said}", r.status).to_lowercase())
    })
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let d = std::env::temp_dir().join(format!(
            "igris-sample-test-{}-{n}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn user_cfg() -> Config {
        let mut c = Config::default();
        c.checks.command = "cargo test".into();
        c.policy.base = "develop".into();
        c
    }

    #[test]
    fn the_throwaway_repository_is_real_and_uses_the_fixed_check() {
        let root = tmp("ok");
        let seen = sample_task_with(&root, &user_cfg(), |repo, cfg| {
            let log = Command::new("git")
                .args(["log", "--oneline"])
                .current_dir(repo)
                .output()?;
            let branch = Command::new("git")
                .args(["branch", "--show-current"])
                .current_dir(repo)
                .output()?;
            assert_eq!(String::from_utf8_lossy(&branch.stdout).trim(), "main");
            assert_eq!(String::from_utf8_lossy(&log.stdout).lines().count(), 1);
            Ok((repo.to_path_buf(), cfg.clone()))
        })
        .unwrap();
        assert_eq!(
            seen.1.checks.command, CHECK,
            "the user's command is never used here"
        );
        assert_eq!(seen.1.policy.base, "main");
        assert_eq!(seen.1.agents, user_cfg().agents, "the user's agents are");
        assert!(!seen.0.exists(), "removed after a success");
    }

    #[test]
    fn the_throwaway_repository_is_removed_after_an_error_too() {
        let root = tmp("err");
        let mut path = None;
        let r: Result<(), _> = sample_task_with(&root, &user_cfg(), |repo, _| {
            path = Some(repo.to_path_buf());
            Err("boom".into())
        });
        assert_eq!(r.unwrap_err().to_string(), "boom");
        assert!(!path.unwrap().exists());
    }

    #[test]
    fn the_task_is_one_small_instruction() {
        assert!(TASK.contains("HELLO.txt") && CHECK.contains("HELLO.txt"));
    }
}
