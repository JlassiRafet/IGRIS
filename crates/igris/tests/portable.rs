//! Tests that run the same on Linux, macOS and Windows.
use std::process::Command;

fn stderr(o: &std::process::Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

#[test]
fn version_prints_name_and_version() {
    let out = Command::new(env!("CARGO_BIN_EXE_igris"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        format!("igris {}\n", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn no_arguments_without_a_terminal_prints_usage_and_exits_2() {
    let o = Command::new(env!("CARGO_BIN_EXE_igris"))
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(2), "{}", stderr(&o));
    assert!(stderr(&o).contains("Usage: igris"), "{}", stderr(&o));
    assert!(o.stdout.is_empty(), "stdout stays free for JSON");
}
