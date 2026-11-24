//! Background commands: an install or a live check runs on its own thread with a timeout while the
//! app stays usable. Every command is a fixed argv (never built from user text or detected data),
//! gets no stdin, and cannot outlive the app: quitting, or a closed terminal, kills it and
//! everything it started.
use crate::activity::{Activity, Entry};
use crate::clean::last_lines;
use crate::tui::detect_worker::Msg;
use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

/// Output kept per job: only the tail is ever read, but the pipes must be drained to the end so
/// the command never blocks on a full pipe.
const KEEP: usize = 256 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct JobId(pub u32);

#[derive(Clone, Debug, PartialEq)]
pub struct JobEnd {
    /// The exit code; `None` when it did not start or was killed.
    pub code: Option<i32>,
    /// The last 20 output lines, clean of escape sequences.
    pub tail: Vec<String>,
    pub timed_out: bool,
    /// False when the program could not be started at all.
    pub started: bool,
}

#[derive(Default)]
struct Slot {
    child: Mutex<Option<Child>>,
    /// `kill_all` ran: a job that has not started yet must not start.
    cancelled: AtomicBool,
    done: AtomicBool,
}

/// The running jobs. Dropping the set kills them all, so no path out of the app leaves one behind.
#[derive(Default)]
pub struct JobSet {
    slots: Vec<Arc<Slot>>,
}

impl JobSet {
    /// Run `argv` in the background. The result arrives as `Msg::Job(id, ..)`; the command and how
    /// it ended are added to `log`. After `timeout` the command and everything it started is killed.
    pub fn spawn(
        &mut self,
        tx: Sender<Msg>,
        id: JobId,
        argv: &'static [&'static str],
        timeout: Duration,
        log: Arc<Mutex<Activity>>,
    ) {
        self.slots.retain(|s| !s.done.load(Ordering::SeqCst));
        let slot = Arc::new(Slot::default());
        self.slots.push(slot.clone());
        std::thread::spawn(move || {
            let started = Instant::now();
            let end = run(argv, timeout, &slot);
            slot.done.store(true, Ordering::SeqCst);
            if let Ok(mut log) = log.lock() {
                log.push(Entry {
                    argv: argv.iter().map(|a| (*a).to_string()).collect(),
                    code: end.code,
                    secs: started.elapsed().as_secs_f32(),
                });
            }
            let _ = tx.send(Msg::Job(id, end));
        });
    }

    /// Kill every running job and what it started; jobs that have not started yet never will.
    pub fn kill_all(&mut self) {
        for slot in &self.slots {
            slot.cancelled.store(true, Ordering::SeqCst);
            let mut child = slot.child.lock().unwrap_or_else(PoisonError::into_inner);
            if let Some(c) = child.as_mut() {
                kill_tree(c);
            }
        }
    }
}

impl Drop for JobSet {
    fn drop(&mut self) {
        self.kill_all();
    }
}

fn failed_to_start(what: String) -> JobEnd {
    JobEnd {
        code: None,
        tail: vec![what],
        timed_out: false,
        started: false,
    }
}

fn run(argv: &'static [&'static str], timeout: Duration, slot: &Slot) -> JobEnd {
    let Some((program, args)) = argv.split_first() else {
        return failed_to_start("could not start: no command".to_string());
    };
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        // Its own process group, so a kill reaches everything the command starts.
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return failed_to_start(format!("could not start {program}: {e}")),
    };
    let output = Arc::new(Mutex::new(Vec::<u8>::new()));
    let (done_tx, done_rx) = mpsc::channel::<()>();
    let mut readers = 0;
    if let Some(out) = child.stdout.take() {
        read_into(out, output.clone(), done_tx.clone());
        readers += 1;
    }
    if let Some(err) = child.stderr.take() {
        read_into(err, output.clone(), done_tx.clone());
        readers += 1;
    }
    drop(done_tx);
    {
        let mut held = slot.child.lock().unwrap_or_else(PoisonError::into_inner);
        if slot.cancelled.load(Ordering::SeqCst) {
            kill_tree(&mut child);
        }
        *held = Some(child);
    }
    let deadline = Instant::now() + timeout;
    let (code, timed_out) = loop {
        {
            let mut held = slot.child.lock().unwrap_or_else(PoisonError::into_inner);
            let Some(c) = held.as_mut() else {
                break (None, false);
            };
            match c.try_wait() {
                Ok(Some(status)) => break (status.code(), false),
                Ok(None) if Instant::now() >= deadline => {
                    kill_tree(c);
                    let _ = c.wait();
                    break (None, true);
                }
                Ok(None) => {}
                Err(_) => break (None, false),
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    *slot.child.lock().unwrap_or_else(PoisonError::into_inner) = None;
    // A background process of the command may keep the pipes open: wait a moment, then read
    // what there is.
    for _ in 0..readers {
        if done_rx.recv_timeout(Duration::from_millis(300)).is_err() {
            break;
        }
    }
    let bytes = output
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    JobEnd {
        code,
        tail: last_lines(&String::from_utf8_lossy(&bytes), 20),
        timed_out,
        started: true,
    }
}

/// Read a pipe to its end on its own thread, keeping the last `KEEP` bytes in `into`.
fn read_into(mut pipe: impl Read + Send + 'static, into: Arc<Mutex<Vec<u8>>>, done: Sender<()>) {
    std::thread::spawn(move || {
        let mut chunk = [0u8; 8192];
        while let Ok(n) = pipe.read(&mut chunk) {
            if n == 0 {
                break;
            }
            let mut kept = into.lock().unwrap_or_else(PoisonError::into_inner);
            kept.extend_from_slice(&chunk[..n]);
            if kept.len() > 2 * KEEP {
                let cut = kept.len() - KEEP;
                kept.drain(..cut);
            }
        }
        let _ = done.send(());
    });
}

/// Kill the command and everything it started.
fn kill_tree(child: &mut Child) {
    #[cfg(unix)]
    {
        // A negative pid names the process group the command leads.
        let _ = Command::new("kill")
            .args(["-KILL", "--", &format!("-{}", child.id())])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill(); // the command itself, if the group kill could not run
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    // The commands below need `sh`, so they and their tests are Unix-only.
    #[cfg(unix)]
    const LINES: &[&str] = &[
        "sh",
        "-c",
        "i=1; while [ $i -le 30 ]; do echo line$i; i=$((i+1)); done; exit 3",
    ];
    #[cfg(unix)]
    const SLEEPS: &[&str] = &["sh", "-c", "sleep 30"];
    #[cfg(unix)]
    const ESCAPES: &[&str] = &[
        "sh",
        "-c",
        "printf 'a\\033[31mred\\033[0m \\033]52;c;aGk=\\007done\\n'",
    ];
    #[cfg(unix)]
    const READS_STDIN: &[&str] = &["sh", "-c", "cat; echo done"];
    const MISSING: &[&str] = &["igris-no-such-tool-for-tests"];
    /// A shell that starts a grandchild and records its pid.
    #[cfg(unix)]
    const GRANDCHILD_A: &[&str] = &[
        "sh",
        "-c",
        "sleep 60 & echo $! > /tmp/igris-job-test-a.pid; wait",
    ];
    #[cfg(unix)]
    const GRANDCHILD_B: &[&str] = &[
        "sh",
        "-c",
        "sleep 60 & echo $! > /tmp/igris-job-test-b.pid; wait",
    ];

    fn start(
        argv: &'static [&'static str],
        timeout_ms: u64,
    ) -> (JobSet, mpsc::Receiver<Msg>, Arc<Mutex<Activity>>) {
        let (tx, rx) = mpsc::channel();
        let log = Arc::new(Mutex::new(Activity::default()));
        let mut jobs = JobSet::default();
        jobs.spawn(
            tx,
            JobId(7),
            argv,
            Duration::from_millis(timeout_ms),
            log.clone(),
        );
        (jobs, rx, log)
    }

    fn finished(rx: &mpsc::Receiver<Msg>) -> JobEnd {
        match rx.recv_timeout(Duration::from_secs(20)) {
            Ok(Msg::Job(id, end)) => {
                assert_eq!(id, JobId(7));
                end
            }
            other => panic!("no job result: {other:?}"),
        }
    }

    fn run(argv: &'static [&'static str], timeout_ms: u64) -> (JobEnd, Arc<Mutex<Activity>>) {
        let (_jobs, rx, log) = start(argv, timeout_ms);
        (finished(&rx), log)
    }

    #[cfg(unix)]
    fn alive(pid: u32) -> bool {
        let out = std::process::Command::new("ps")
            .args(["-o", "stat=", "-p", &pid.to_string()])
            .output();
        out.is_ok_and(|o| {
            let stat = String::from_utf8_lossy(&o.stdout).trim().to_string();
            o.status.success() && !stat.is_empty() && !stat.starts_with('Z')
        })
    }

    #[cfg(unix)]
    fn pid_from(path: &str) -> u32 {
        for _ in 0..150 {
            if let Some(pid) = std::fs::read_to_string(path)
                .ok()
                .and_then(|t| t.trim().parse().ok())
            {
                return pid;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("{path} never appeared");
    }

    #[cfg(unix)]
    fn gone_soon(pid: u32) -> bool {
        (0..150).any(|_| {
            std::thread::sleep(Duration::from_millis(20));
            !alive(pid)
        })
    }

    #[cfg(unix)]
    #[test]
    fn a_job_reports_the_exit_code_and_the_last_20_lines() {
        let (end, _) = run(LINES, 20_000);
        assert_eq!(end.code, Some(3));
        assert!(end.started && !end.timed_out);
        assert_eq!(end.tail.len(), 20);
        assert_eq!(
            (end.tail[0].as_str(), end.tail[19].as_str()),
            ("line11", "line30")
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_job_past_its_timeout_is_killed_and_flagged() {
        let began = Instant::now();
        let (end, _) = run(SLEEPS, 300);
        assert!(end.timed_out && end.started);
        assert_eq!(end.code, None);
        assert!(
            began.elapsed() < Duration::from_secs(10),
            "it waited for the sleep"
        );
    }

    #[cfg(unix)]
    #[test]
    fn kill_all_ends_running_children_and_what_they_started() {
        let path = "/tmp/igris-job-test-a.pid";
        let _ = std::fs::remove_file(path);
        let (mut jobs, rx, _) = start(GRANDCHILD_A, 60_000);
        let pid = pid_from(path);
        assert!(alive(pid));
        jobs.kill_all();
        let _ = finished(&rx);
        assert!(gone_soon(pid), "the grandchild {pid} is still running");
        let _ = std::fs::remove_file(path);
    }

    #[cfg(unix)]
    #[test]
    fn dropping_the_job_set_kills_the_children_too() {
        let path = "/tmp/igris-job-test-b.pid";
        let _ = std::fs::remove_file(path);
        let (jobs, _rx, _) = start(GRANDCHILD_B, 60_000);
        let pid = pid_from(path);
        assert!(alive(pid));
        drop(jobs);
        assert!(gone_soon(pid), "the grandchild {pid} is still running");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn a_command_that_cannot_start_reports_started_false() {
        let (end, _) = run(MISSING, 5_000);
        assert!(!end.started);
        assert_eq!(end.code, None);
        assert!(end.tail[0].starts_with("could not start igris-no-such-tool-for-tests"));
    }

    #[cfg(unix)]
    #[test]
    fn escape_sequences_in_output_are_stripped_from_the_tail() {
        let (end, _) = run(ESCAPES, 5_000);
        assert_eq!(end.tail, ["ared done"]);
    }

    #[cfg(unix)]
    #[test]
    fn stdin_is_closed_so_a_tool_that_reads_it_does_not_wait() {
        let (end, _) = run(READS_STDIN, 5_000);
        assert_eq!(end.tail, ["done"]);
        assert!(!end.timed_out);
    }

    #[cfg(unix)]
    #[test]
    fn every_job_adds_an_activity_entry() {
        let (_, log) = run(LINES, 20_000);
        let log = log.lock().unwrap();
        assert_eq!(log.entries().len(), 1);
        assert_eq!(log.entries()[0].argv[0], "sh");
        assert_eq!(log.entries()[0].code, Some(3));
    }
}
