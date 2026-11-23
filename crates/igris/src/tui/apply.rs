//! Writing `igris.toml` safely: show the change first, keep the old file, replace atomically.
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiffLine {
    Same(String),
    Add(String),
    Del(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub path: PathBuf,
    /// What the file holds now; `None` when it does not exist yet.
    pub existing: Option<String>,
    pub new: String,
    pub diff: Vec<DiffLine>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Applied {
    pub path: PathBuf,
    pub backup: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Repo,
    UserConfig,
}

impl Target {
    pub fn path(&self, repo: &Path, home: &Path) -> PathBuf {
        match self {
            Target::Repo => repo.join("igris.toml"),
            Target::UserConfig => igris_core::platform::user_config_file_here(home),
        }
    }
}

/// A line diff: lines in both stay `Same` in order, the rest are `Del` (old) and `Add` (new).
pub fn diff_lines(old: &str, new: &str) -> Vec<DiffLine> {
    let (a, b): (Vec<&str>, Vec<&str>) = (old.lines().collect(), new.lines().collect());
    // shortcut: O(n*m) longest-common-subsequence table; config files stay under 200 lines.
    let mut lcs = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            lcs[i][j] = if a[i] == b[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }
    let (mut i, mut j, mut out) = (0, 0, vec![]);
    while i < a.len() || j < b.len() {
        if i < a.len() && j < b.len() && a[i] == b[j] {
            out.push(DiffLine::Same(a[i].into()));
            (i, j) = (i + 1, j + 1);
        } else if i < a.len() && (j == b.len() || lcs[i + 1][j] >= lcs[i][j + 1]) {
            // A removed line is listed before its replacement.
            out.push(DiffLine::Del(a[i].into()));
            i += 1;
        } else {
            out.push(DiffLine::Add(b[j].into()));
            j += 1;
        }
    }
    out
}

pub fn plan_change(path: &Path, new: String) -> io::Result<Change> {
    let existing = match fs::read_to_string(path) {
        Ok(t) => Some(t),
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(e) => return Err(e),
    };
    let diff = diff_lines(existing.as_deref().unwrap_or(""), &new);
    Ok(Change {
        path: path.to_path_buf(),
        existing,
        new,
        diff,
    })
}

/// `igris.toml.bak`, or the first `.bak.N` that is free: a backup is never overwritten.
fn free_backup(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let first = path.with_file_name(format!("{name}.bak"));
    if !first.exists() {
        return first;
    }
    (1..)
        .map(|n| path.with_file_name(format!("{name}.bak.{n}")))
        .find(|p| !p.exists())
        .unwrap_or(first)
}

/// Write the new text to a temp file in the same directory, back up the old file byte for byte,
/// then rename over it. A failure leaves the original as it was.
pub fn apply(c: &Change) -> io::Result<Applied> {
    let dir = c
        .path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(dir)?;
    let name = c
        .path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = dir.join(format!(".{name}.igris-tmp-{}", std::process::id()));
    let result = (|| {
        let mut f = File::create(&tmp)?;
        f.write_all(c.new.as_bytes())?;
        f.sync_all()?;
        if let Ok(m) = fs::metadata(&c.path) {
            fs::set_permissions(&tmp, m.permissions())?;
        }
        let backup = if c.path.exists() {
            let b = free_backup(&c.path);
            fs::copy(&c.path, &b)?;
            Some(b)
        } else {
            None
        };
        fs::rename(&tmp, &c.path)?;
        Ok(Applied {
            path: c.path.clone(),
            backup,
        })
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    fn tmp(name: &str) -> PathBuf {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let d = std::env::temp_dir().join(format!("igris-apply-{}-{n}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn lines(d: &[DiffLine]) -> Vec<String> {
        d.iter()
            .map(|l| match l {
                DiffLine::Same(s) => format!("  {s}"),
                DiffLine::Add(s) => format!("+ {s}"),
                DiffLine::Del(s) => format!("- {s}"),
            })
            .collect()
    }

    #[test]
    fn a_new_file_is_created_with_exactly_the_text_and_no_backup() {
        let d = tmp("new");
        let c = plan_change(&d.join("igris.toml"), "a = 1\n".into()).unwrap();
        assert_eq!(c.existing, None);
        let done = apply(&c).unwrap();
        assert_eq!(done.backup, None);
        assert_eq!(fs::read_to_string(d.join("igris.toml")).unwrap(), "a = 1\n");
        let names: Vec<_> = fs::read_dir(&d)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names.len(), 1, "no temp file is left: {names:?}");
    }

    #[test]
    fn an_existing_file_is_backed_up_byte_for_byte_then_replaced() {
        let d = tmp("existing");
        let p = d.join("igris.toml");
        let old: &[u8] = b"[checks]\r\ncommand = \"x\"\r\n# caf\xc3\xa9\r\n";
        fs::write(&p, old).unwrap();
        #[cfg(unix)]
        fs::set_permissions(&p, fs::Permissions::from_mode(0o600)).unwrap();
        let done = apply(&plan_change(&p, "new\n".into()).unwrap()).unwrap();
        assert_eq!(fs::read(done.backup.as_ref().unwrap()).unwrap(), old);
        assert_eq!(done.backup.unwrap(), d.join("igris.toml.bak"));
        assert_eq!(fs::read_to_string(&p).unwrap(), "new\n");
        #[cfg(unix)]
        assert_eq!(
            fs::metadata(&p).unwrap().permissions().mode() & 0o777,
            0o600,
            "mode kept"
        );
    }

    #[test]
    fn applying_twice_never_overwrites_a_backup() {
        let d = tmp("twice");
        let p = d.join("igris.toml");
        fs::write(&p, "v1\n").unwrap();
        apply(&plan_change(&p, "v2\n".into()).unwrap()).unwrap();
        let second = apply(&plan_change(&p, "v3\n".into()).unwrap()).unwrap();
        assert_eq!(
            fs::read_to_string(d.join("igris.toml.bak")).unwrap(),
            "v1\n",
            "the first backup is kept"
        );
        assert_eq!(second.backup.unwrap(), d.join("igris.toml.bak.1"));
        assert_eq!(
            fs::read_to_string(d.join("igris.toml.bak.1")).unwrap(),
            "v2\n"
        );
        assert_eq!(fs::read_to_string(&p).unwrap(), "v3\n");
    }

    #[test]
    fn the_diff_marks_only_changed_lines_and_keeps_order() {
        let d = diff_lines("a\nb\nc\nd\n", "a\nB\nc\nd\ne\n");
        assert_eq!(lines(&d), ["  a", "- b", "+ B", "  c", "  d", "+ e"]);
        let same = diff_lines("x\ny\n", "x\ny\n");
        assert!(same.iter().all(|l| matches!(l, DiffLine::Same(_))));
        assert_eq!(lines(&diff_lines("", "x\n")), ["+ x"]);
    }

    #[test]
    fn a_reordered_pair_shows_as_one_move_not_a_rewrite() {
        let d = diff_lines("a\nb\nc\n", "a\nc\nb\n");
        let kept = d.iter().filter(|l| matches!(l, DiffLine::Same(_))).count();
        assert_eq!(kept, 2, "{:?}", lines(&d));
        assert_eq!(d.len(), 4);
        assert_eq!(d.first(), Some(&DiffLine::Same("a".into())));
    }

    #[cfg(unix)]
    #[test]
    fn a_directory_that_cannot_be_written_fails_and_leaves_the_original_alone() {
        let d = tmp("readonly");
        let p = d.join("igris.toml");
        fs::write(&p, "keep me\n").unwrap();
        fs::set_permissions(&d, fs::Permissions::from_mode(0o555)).unwrap();
        let can_write_anyway = File::create(d.join("probe")).is_ok(); // running as root
        let c = plan_change(&p, "new\n".into()).unwrap();
        let r = apply(&c);
        fs::set_permissions(&d, fs::Permissions::from_mode(0o755)).unwrap();
        if can_write_anyway {
            return;
        }
        assert!(r.is_err());
        assert_eq!(fs::read(&p).unwrap(), b"keep me\n");
        assert_eq!(
            fs::read_dir(&d).unwrap().count(),
            1,
            "no temp or backup left behind"
        );
    }

    #[cfg(unix)] // asserts the Unix config path; platform.rs tests the Windows one
    #[test]
    fn the_user_config_folder_is_created_when_missing() {
        let home = tmp("home");
        let p = Target::UserConfig.path(Path::new("/repo"), &home);
        assert_eq!(p, home.join(".config/igris/igris.toml"));
        apply(&plan_change(&p, "x\n".into()).unwrap()).unwrap();
        assert_eq!(fs::read_to_string(p).unwrap(), "x\n");
        assert_eq!(
            Target::Repo.path(Path::new("/repo"), &home),
            Path::new("/repo/igris.toml")
        );
    }
}
