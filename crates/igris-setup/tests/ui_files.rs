//! Checks on the shared UI files in `ui/` (the one copy the app and the site both use).

use std::fs;
use std::path::PathBuf;

fn ui(name: &str) -> String {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("ui")
        .join(name);
    fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

#[test]
fn pipeline_files_are_split_out() {
    let html = ui("index.html");
    ui("pipeline.css");
    ui("pipeline.js");
    assert!(html.contains(r#"href="pipeline.css""#));
    assert!(html.contains(r#"src="pipeline.js""#));
    assert!(!html.contains("<style"), "no inline style block");
    for (at, _) in html.match_indices("<script") {
        let tag = &html[at..at + html[at..].find('>').unwrap()];
        assert!(tag.contains("src="), "inline script: {tag}");
    }
}
