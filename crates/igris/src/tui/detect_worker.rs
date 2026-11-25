//! Detection runs on its own thread so the screen stays usable while it works.
use igris_detect::Snapshot;
use std::sync::mpsc::Sender;

#[derive(Debug)]
pub enum Msg {
    /// The scan is at `(done, total)` and about to probe the named thing.
    Progress(usize, usize, &'static str),
    Detected(Box<Snapshot>),
    /// Tools the scan has no field for that are on `PATH` (OpenClaw).
    Found(Vec<&'static str>),
    /// The doctor's checks finished.
    Checked(Vec<crate::doctor::Check>),
    /// The sample task finished: what it ended with, or why it could not run.
    Sampled(Result<String, String>),
    /// Which local models fit this machine (the fit adapter's answer, after the scan).
    Fit(Box<igris_detect::fit::Report>),
    /// A background job (an install or a live check) ended.
    Job(crate::job::JobId, crate::job::JobEnd),
}

pub fn spawn_checks(tx: Sender<Msg>, repo: std::path::PathBuf) {
    std::thread::spawn(move || {
        let _ = tx.send(Msg::Checked(crate::doctor::checks(&repo, None)));
    });
}

pub fn spawn_sample(tx: Sender<Msg>, repo: std::path::PathBuf) {
    std::thread::spawn(move || {
        let _ = tx.send(Msg::Sampled(super::sample::run_sample_in(&repo)));
    });
}

/// Run `probe` off the UI thread and send its result. A closed channel (the app has quit) is fine.
/// `probe` gets a callback for progress reports; each becomes a `Msg::Progress`.
pub fn spawn<F>(tx: Sender<Msg>, probe: F)
where
    F: FnOnce(&mut dyn FnMut(usize, usize, &'static str)) -> Snapshot + Send + 'static,
{
    std::thread::spawn(move || {
        let progress_tx = tx.clone();
        let snap = probe(&mut |done, total, what| {
            let _ = progress_tx.send(Msg::Progress(done, total, what));
        });
        let _ = tx.send(Msg::Detected(Box::new(snap)));
    });
}

/// The real probe: this machine, as `igris doctor --json` sees it.
pub fn spawn_real(tx: Sender<Msg>) {
    // The fit asks `llmfit` a few questions, which takes seconds: it has its own thread.
    let fit_tx = tx.clone();
    std::thread::spawn(move || {
        let report = igris_detect::fit::recommend(&igris_detect::RealHost::new());
        let _ = fit_tx.send(Msg::Fit(Box::new(report)));
    });
    let found_tx = tx.clone();
    spawn(tx, move |progress| {
        use igris_detect::Host;
        let host = igris_detect::RealHost::new();
        let snap =
            igris_detect::detect_with_progress(&host, &igris_detect::Options::default(), progress);
        let ids = super::tools_screen::found_on_path(&super::tools_screen::extra_ids(), |n| {
            host.which(n).is_some()
        });
        let _ = found_tx.send(Msg::Found(ids));
        snap
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::view::tests::real;
    use std::time::Duration;

    #[test]
    fn the_snapshot_arrives_by_channel_without_blocking_the_caller() {
        let (tx, rx) = std::sync::mpsc::channel();
        let (gate_tx, gate_rx) = std::sync::mpsc::channel::<()>();
        spawn(tx, move |_| {
            gate_rx.recv().unwrap();
            real()
        });
        assert!(
            rx.try_recv().is_err(),
            "the caller got control back before the probe finished"
        );
        gate_tx.send(()).unwrap();
        let Msg::Detected(s) = rx.recv_timeout(Duration::from_secs(5)).unwrap() else {
            panic!()
        };
        assert!(s.herdr.found);
    }

    #[test]
    fn progress_reports_arrive_in_order_before_the_snapshot() {
        let (tx, rx) = std::sync::mpsc::channel();
        spawn(tx, |progress| {
            progress(0, 6, "machine");
            progress(3, 6, "hermes");
            real()
        });
        let got: Vec<Msg> = std::iter::from_fn(|| rx.recv_timeout(Duration::from_secs(5)).ok())
            .take(3)
            .collect();
        assert!(
            matches!(got[0], Msg::Progress(0, 6, "machine")),
            "{:?}",
            got[0]
        );
        assert!(matches!(got[1], Msg::Progress(3, 6, "hermes")));
        assert!(matches!(got[2], Msg::Detected(_)));
    }
}
