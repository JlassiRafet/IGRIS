mod common;
use common::FakeHost;
use igris_detect::{Options, detect_with, detect_with_progress};

#[test]
fn progress_is_reported_before_each_of_the_six_probes_and_the_result_is_unchanged() {
    let host = FakeHost::linux()
        .cmd("herdr --version", "herdr 0.9.1")
        .bin("herdr");
    let mut seen: Vec<(usize, usize, &'static str)> = vec![];
    let with = detect_with_progress(&host, &Options::default(), &mut |d, t, w| {
        seen.push((d, t, w))
    });
    let labels: Vec<_> = seen.iter().map(|s| s.2).collect();
    assert_eq!(
        labels,
        [
            "machine",
            "herdr",
            "agents",
            "hermes",
            "decision layer",
            "local models",
            "done"
        ]
    );
    let counts: Vec<_> = seen.iter().map(|s| (s.0, s.1)).collect();
    assert_eq!(
        counts,
        [(0, 6), (1, 6), (2, 6), (3, 6), (4, 6), (5, 6), (6, 6)]
    );
    assert_eq!(with, detect_with(&host, &Options::default()));
}
