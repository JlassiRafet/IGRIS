//! The few places where Linux, macOS and Windows differ: where the user's folders are and which
//! file names an executable can have. Everything takes its environment as an argument so the
//! tests need no process state.
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Os {
    Unix,
    Windows,
}

impl Os {
    pub fn current() -> Os {
        if cfg!(windows) { Os::Windows } else { Os::Unix }
    }
}

/// The user's home directory: `HOME` on Linux and macOS, `USERPROFILE` on Windows.
pub fn home() -> Option<PathBuf> {
    std::env::home_dir().filter(|p| !p.as_os_str().is_empty())
}

/// Where a user-level `igris.toml` lives: `~/.config/igris/igris.toml`, or on Windows
/// `%APPDATA%\igris\igris.toml` (falling back to `~\AppData\Roaming`).
pub fn user_config_file(os: Os, home: &Path, appdata: Option<&Path>) -> PathBuf {
    match os {
        Os::Unix => home.join(".config").join("igris").join("igris.toml"),
        Os::Windows => appdata
            .map(Path::to_path_buf)
            .unwrap_or_else(|| home.join("AppData").join("Roaming"))
            .join("igris")
            .join("igris.toml"),
    }
}

/// `user_config_file` for the platform this program runs on.
pub fn user_config_file_here(home: &Path) -> PathBuf {
    let appdata = std::env::var_os("APPDATA").map(PathBuf::from);
    user_config_file(Os::current(), home, appdata.as_deref())
}

/// The file names an executable called `name` can have: just `name` on Unix; on Windows also `name`
/// with each extension of `PATHEXT` (`.EXE;.CMD;...`), because `claude` is `claude.cmd` there.
pub fn executable_names(os: Os, name: &str, pathext: Option<&str>) -> Vec<String> {
    match os {
        Os::Unix => vec![name.to_string()],
        Os::Windows => {
            let mut v = vec![name.to_string()];
            let ext = pathext.unwrap_or(".COM;.EXE;.BAT;.CMD");
            v.extend(
                ext.split(';')
                    .filter(|e| e.starts_with('.'))
                    .map(|e| format!("{name}{}", e.to_ascii_lowercase())),
            );
            v
        }
    }
}

/// `executable_names` for the platform this program runs on.
pub fn executable_names_here(name: &str) -> Vec<String> {
    let pathext = std::env::var("PATHEXT").ok();
    executable_names(Os::current(), name, pathext.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_user_config_lives_where_each_platform_expects() {
        let home = Path::new("/home/u");
        assert_eq!(
            user_config_file(Os::Unix, home, None),
            Path::new("/home/u/.config/igris/igris.toml")
        );
        assert_eq!(
            user_config_file(
                Os::Windows,
                Path::new(r"C:\Users\u"),
                Some(Path::new(r"C:\Users\u\AppData\Roaming"))
            ),
            Path::new(r"C:\Users\u\AppData\Roaming")
                .join("igris")
                .join("igris.toml")
        );
        assert_eq!(
            user_config_file(Os::Windows, Path::new("home"), None),
            Path::new("home")
                .join("AppData")
                .join("Roaming")
                .join("igris")
                .join("igris.toml"),
            "no APPDATA falls back to the usual place"
        );
    }

    #[test]
    fn on_windows_an_executable_can_carry_an_extension() {
        assert_eq!(
            executable_names(Os::Unix, "claude", Some(".EXE")),
            ["claude"]
        );
        assert_eq!(
            executable_names(Os::Windows, "claude", Some(".EXE;.CMD")),
            ["claude", "claude.exe", "claude.cmd"]
        );
        assert_eq!(
            executable_names(Os::Windows, "herdr", None),
            ["herdr", "herdr.com", "herdr.exe", "herdr.bat", "herdr.cmd"],
            "the default PATHEXT"
        );
        assert_eq!(
            executable_names(Os::Windows, "x", Some("junk;.EXE")),
            ["x", "x.exe"],
            "garbage entries are ignored"
        );
    }

    #[test]
    fn the_home_directory_is_known_and_not_empty() {
        // This program runs with a home directory in CI and on a developer machine.
        assert!(home().is_some_and(|h| h.is_absolute()));
    }
}
