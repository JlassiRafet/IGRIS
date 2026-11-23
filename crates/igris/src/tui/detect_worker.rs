//! Detection runs on its own thread so the screen stays usable while it works.
use igris_detect::Snapshot;
use std::sync::mpsc::Sender;

#[derive(Debug)]
pub enum Msg {
    Detected(Box<Snapshot>),
}

/// Run `probe` off the UI thread and send its result. A closed channel (the app has quit) is fine.
pub fn spawn<F: FnOnce() -> Snapshot + Send + 'static>(tx: Sender<Msg>, probe: F) {
    std::thread::spawn(move || {
        let _ = tx.send(Msg::Detected(Box::new(probe())));
    });
}

/// The real probe: this machine, as `igris doctor --json` sees it.
pub fn spawn_real(tx: Sender<Msg>) {
    spawn(tx, || igris_detect::detect(&igris_detect::RealHost::new()));
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
        spawn(tx, move || {
            gate_rx.recv().unwrap();
            real()
        });
        assert!(
            rx.try_recv().is_err(),
            "the caller got control back before the probe finished"
        );
        gate_tx.send(()).unwrap();
        let Msg::Detected(s) = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(s.herdr.found);
    }
}
