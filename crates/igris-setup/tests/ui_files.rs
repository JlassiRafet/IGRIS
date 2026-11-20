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

/// Every tag in `html`, as the text between `<` and `>`.
fn tags(html: &str) -> Vec<&str> {
    html.match_indices('<')
        .filter_map(|(at, _)| html[at + 1..].find('>').map(|e| &html[at + 1..at + 1 + e]))
        .collect()
}

#[test]
fn setup_page_has_no_inline_code() {
    let html = ui("setup.html");
    assert!(!html.contains("<style"), "no inline style block");
    assert!(!html.to_lowercase().contains("javascript:"));
    for tag in tags(&html) {
        let mut parts = tag.split_whitespace();
        let name = parts.next().unwrap_or("");
        if name == "script" {
            assert!(tag.contains("src="), "inline script: {tag}");
        }
        for attr in parts {
            assert!(!attr.starts_with("style="), "inline style attribute: {tag}");
            assert!(
                !(attr.starts_with("on") && attr.contains('=')),
                "inline event handler: {tag}"
            );
        }
    }
}

/// The one link out: the docs. It is a link the person clicks, not something the page loads.
const DOCS: &str = "https://jlassirafet.github.io/IGRIS/docs/";

#[test]
fn setup_files_load_nothing_external() {
    for name in ["setup.html", "setup.css", "setup.js", "palette.js"] {
        let text = ui(name)
            .replace("http://www.w3.org/2000/svg", "")
            .replace(DOCS, "");
        assert!(!text.contains("http://"), "{name}: external http");
        assert!(!text.contains("https://"), "{name}: external https");
        assert!(!text.contains("@import"), "{name}: @import");
    }
}

#[test]
fn setup_js_never_writes_html() {
    for name in ["setup.js", "palette.js"] {
        let js = ui(name);
        for bad in [
            "innerHTML",
            "outerHTML",
            "insertAdjacentHTML",
            "document.write",
            "eval(",
            "new Function",
        ] {
            assert!(!js.contains(bad), "{name} uses {bad}");
        }
    }
}

/// The app's favicon is a copy of the logo's favicon. A copy can drift, so this pins it. (Skipped
/// when the repository's `assets/` folder is absent, as in a packaged crate.)
#[test]
fn favicon_matches_the_logo_asset() {
    let asset = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/logo/favicon/IGRIS_favicon.svg");
    let Ok(want) = fs::read_to_string(&asset) else {
        return;
    };
    assert_eq!(
        ui("favicon.svg"),
        want,
        "copy assets/logo/favicon/IGRIS_favicon.svg to ui/favicon.svg"
    );
}

#[test]
fn fonts_ship_with_their_licence() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("ui")
        .join("fonts");
    let licence = fs::read_to_string(dir.join("OFL.txt")).expect("ui/fonts/OFL.txt");
    assert!(licence.contains("SIL OPEN FONT LICENSE"));
    for name in [
        "JetBrainsMono-Regular.woff2",
        "JetBrainsMono-SemiBold.woff2",
    ] {
        let bytes = fs::read(dir.join(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(&bytes[..4], b"wOF2", "{name}");
    }
}
