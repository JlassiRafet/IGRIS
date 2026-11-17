use std::process::Command;

#[test]
fn version_prints_name_and_version() {
    let out = Command::new(env!("CARGO_BIN_EXE_igris"))
        .arg("--version")
        .output()
        .expect("run igris");
    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout), "igris 0.0.0\n");
}
