//! The terminal app: `igris` with no arguments.
//!
//! Keys and mouse clicks both become an `Action`; `state::update` is the only place state changes.

mod action;
mod apply;
mod attached;
mod chain;
mod check;
mod connect;
pub(crate) mod detect_worker;
pub(crate) mod fix;
mod footer;
mod form;
mod header;
mod help;
mod hero;
mod hit;
mod input;
mod install;
mod keymap;
mod layout;
mod next_step;
mod palette;
mod pipeline;
mod pipeline_screen;
mod rail;
mod review;
mod sample;
mod state;
mod style;
mod term;
mod test_screen;
mod theme;
mod tools_draw;
mod tools_screen;
mod verify_screen;
mod widgets;
mod wordmark;

use action::Action;
use hit::HitMap;
use keymap::{Context, key_to_action};
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::{self, Event, KeyEventKind, MouseButton, MouseEventKind};
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph, Wrap};
use ratatui::{Frame, Terminal};
use state::{Effect, Overlay, Screen, State, type_key, update};
use std::error::Error;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;
use theme::{Env, Theme};

pub const MIN_WIDTH: u16 = 80;
pub const MIN_HEIGHT: u16 = 24;

pub struct Opts {
    pub repo: PathBuf,
    pub no_mouse: bool,
}

pub fn draw(f: &mut Frame, s: &State, t: &Theme, hits: &mut HitMap) {
    hits.clear();
    let area = f.area();
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        let msg = format!(
            "Make the window at least {MIN_WIDTH}x{MIN_HEIGHT} (now {}x{})",
            area.width, area.height
        );
        f.render_widget(Paragraph::new(msg).wrap(Wrap { trim: true }), area);
        return;
    }
    if let (Some(c), Some(snap)) = (&s.chain, &s.snapshot) {
        chain::draw::draw(f, area, c, snap, t, s.tick);
        return;
    }
    let big = area.height >= layout::TALL_HEIGHT && t.glyphs == theme::Glyphs::Unicode;
    let a = layout::compute(area, big);
    let clickable = s.overlay.is_none();
    if s.screen == Screen::Check && s.check.view.is_none() {
        // The first scan is running: the brand, centred, with real progress.
        hero::draw(f, area, s, t);
    } else {
        header::draw_band(f, a.band, t, s.tick, big);
        let rule = if t.glyphs == theme::Glyphs::Ascii {
            "-"
        } else {
            "─"
        };
        f.render_widget(
            Paragraph::new(Line::styled(
                rule.repeat(a.rule.width as usize),
                t.style(style::Role::Border),
            )),
            a.rule,
        );
        if let Some(rail) = a.rail {
            rail::draw_rail(f, rail, s, t, hits, clickable);
        }
        if let Some(strip) = a.strip {
            rail::draw_strip(f, strip, s, t, hits, clickable);
        }
        let block = widgets::panel(t, s.screen.title(), clickable);
        let body = block.inner(a.panel);
        f.render_widget(block, a.panel);
        match s.screen {
            Screen::Check => check::draw(f, body, s, t, hits, clickable),
            Screen::Tools => tools_draw::draw(f, body, s, t, hits, clickable),
            Screen::Pipeline => pipeline_screen::draw(f, body, s, t, hits, clickable),
            Screen::Verify => verify_screen::draw(f, body, s, t, hits, clickable),
            Screen::Review => review::draw(f, body, s, t, hits, clickable),
            Screen::Test => test_screen::draw(f, body, s, t, hits, clickable),
        }
        let busy = install::progress_line(s, t.glyphs == theme::Glyphs::Ascii);
        if let Some(msg) = s.toast.as_ref().or(busy.as_ref()) {
            let text = widgets::fit(t, msg, a.toast.width as usize);
            f.render_widget(Paragraph::new(text), a.toast);
        }
        draw_next(f, a.next, s, t);
    }
    if let Some(o) = &s.overlay {
        draw_overlay(f, area, o, s, t, hits);
    }
    footer::draw_footer(f, a.footer, context(s), t, hits);
    // Hover: tint the item under the pointer, but never a backdrop or an inert area.
    let hovered = s
        .hover
        .and_then(|(x, y)| hits.rect_at(x, y))
        .filter(|(_, action)| !matches!(action, Action::Cancel | Action::Stay));
    if let Some((rect, _)) = hovered {
        f.buffer_mut().set_style(rect, t.style(style::Role::Hover));
    }
}

/// `▶ Next  <what to do now>`: the one instruction, in its own row above the key hints.
fn draw_next(f: &mut Frame, row: Rect, s: &State, t: &Theme) {
    let step = next_step::next_step(s);
    let done = matches!(step, next_step::Step::Done);
    let arrow = if t.glyphs == theme::Glyphs::Ascii {
        ">"
    } else {
        "▶"
    };
    let lead = if done {
        String::new()
    } else {
        format!("{arrow} Next  ")
    };
    let room = (row.width as usize).saturating_sub(lead.chars().count());
    let text = widgets::fit(t, &next_step::sentence(&step), room);
    let spans = if done {
        vec![Span::styled(text, t.style(style::Role::Ok))]
    } else {
        vec![
            Span::styled(
                lead,
                t.style(style::Role::Bad)
                    .add_modifier(ratatui::style::Modifier::BOLD),
            ),
            Span::styled(text, t.style(style::Role::Text)),
        ]
    };
    f.render_widget(Paragraph::new(Line::from(spans)), row);
}

/// A centred box. A click outside it closes the overlay; a click inside keeps it open.
fn draw_overlay(f: &mut Frame, area: Rect, o: &Overlay, s: &State, t: &Theme, hits: &mut HitMap) {
    let (title, lines, w, h): (String, Vec<Line>, u16, u16) = match o {
        Overlay::Help => (
            "Help".into(),
            help::lines(&s.query),
            76,
            area.height.saturating_sub(4),
        ),
        Overlay::Palette => {
            let rows = palette::entries(&s.query);
            let mut lines = vec![Line::from(format!("> {}_", s.query))];
            lines.extend(rows.iter().enumerate().map(|(i, (_, name))| {
                Line::from(format!("{} {name}", if i == s.cursor { ">" } else { " " }))
            }));
            if rows.is_empty() {
                lines.push(Line::from("  nothing matches"));
            }
            (
                "Quick actions".into(),
                lines,
                50,
                (rows.len() as u16 + 4).min(area.height),
            )
        }
        Overlay::Confirm(state::Confirm::Discard) => (
            "Discard changes?".into(),
            vec![Line::from("You have unsaved changes. Quit anyway? (y / n)")],
            56,
            5,
        ),
        Overlay::Confirm(state::Confirm::SampleTask) => (
            "Run a sample task?".into(),
            vec![
                Line::from("This starts your builder and reviewer agents on a throwaway"),
                Line::from("repository, so it uses your own agent subscriptions or keys."),
                Line::from("Your repository is not touched; the throwaway one is deleted after."),
                Line::from("Run it? (y / n)"),
            ],
            72,
            8,
        ),
        Overlay::Confirm(state::Confirm::Install) => (
            "Install Herdr?".into(),
            vec![
                Line::from("This runs on your terminal:"),
                Line::from(format!("  {}", fix::INSTALL_HERDR.0)),
                Line::from(format!("Source: {}", fix::INSTALL_HERDR.1)),
                Line::from("Run it? (y / n)"),
            ],
            72,
            8,
        ),
        Overlay::Confirm(c @ (state::Confirm::InstallTool(_) | state::Confirm::Pull(_))) => {
            let (title, text) = install::dialog_text(c);
            let h = text.len() as u16 + 2;
            (title, text.into_iter().map(Line::from).collect(), 88, h)
        }
        Overlay::Fix(d) => {
            let mut lines = vec![Line::from(d.detail.clone()), Line::from("")];
            lines.extend(d.fixes.iter().enumerate().map(|(i, x)| {
                Line::from(format!(
                    "{} {}. {}",
                    if i == s.cursor { ">" } else { " " },
                    i + 1,
                    x.label()
                ))
            }));
            lines.push(Line::from(""));
            lines.push(Line::from("Enter runs the chosen one. Esc closes."));
            (d.title.clone(), lines, 88, d.fixes.len() as u16 + 6)
        }
        Overlay::Connect(c) => {
            let (title, text) = connect::overlay_text(s, c, t.glyphs == theme::Glyphs::Ascii);
            let h = text.len() as u16 + 2;
            (title, text.into_iter().map(Line::from).collect(), 88, h)
        }
        Overlay::Failure(id) => {
            let (title, text) = connect::failure_text(s, id, t.glyphs == theme::Glyphs::Ascii);
            let h = text.len() as u16 + 2;
            (title, text.into_iter().map(Line::from).collect(), 72, h)
        }
    };
    let w = w.min(area.width);
    let h = h.min(area.height).max(3);
    let r = Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    );
    hits.add(area, Action::Cancel);
    hits.add(r, Action::Stay);
    let rows = match o {
        Overlay::Fix(d) => d.fixes.len(),
        Overlay::Connect(c) => connect::clickable_rows(s, c),
        _ => 0,
    };
    for i in 0..rows {
        hits.add(
            Rect::new(r.x + 1, r.y + 3 + i as u16, r.width.saturating_sub(2), 1),
            Action::Select(i),
        );
    }
    f.render_widget(Clear, r);
    let scroll = if matches!(o, Overlay::Help) {
        s.cursor as u16
    } else {
        0
    };
    f.render_widget(
        Paragraph::new(lines)
            .scroll((scroll, 0))
            .block(widgets::panel(t, &title, true)),
        r,
    );
}

/// Stop every background command and what it started (the one place the loop's exits share).
pub fn shutdown(jobs: &mut crate::job::JobSet) {
    jobs.kill_all();
}

/// Dropped on every way out of `run` (including an error return), it stops the background commands.
struct JobGuard(Arc<Mutex<crate::job::JobSet>>);

impl Drop for JobGuard {
    fn drop(&mut self) {
        shutdown(&mut self.0.lock().unwrap_or_else(PoisonError::into_inner));
    }
}

/// SIGINT and SIGTERM set the returned flag; the loop sees it and returns, so `TermGuard` drops and
/// `JobGuard` stops the jobs. SIGHUP (the terminal went away) cannot wait for the loop: reading from
/// a dead terminal can spin inside the event reader, where the loop would never see a flag. A watcher
/// thread stops the jobs (each runs in its own process group, which a hangup does not reach) and
/// ends the process. Windows has no SIGTERM or SIGHUP; Ctrl-C arrives as a key event there.
fn watch_signals(jobs: &Arc<Mutex<crate::job::JobSet>>) -> std::io::Result<Arc<AtomicBool>> {
    let stop = Arc::new(AtomicBool::new(false));
    #[cfg(unix)]
    {
        let hup = Arc::new(AtomicBool::new(false));
        let (watched, jobs) = (hup.clone(), jobs.clone());
        std::thread::spawn(move || {
            while !watched.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(50));
            }
            // A lock held for good must not keep the process alive: try for a second, then go.
            for _ in 0..20 {
                match jobs.try_lock() {
                    Ok(mut set) => {
                        shutdown(&mut set);
                        break;
                    }
                    Err(std::sync::TryLockError::Poisoned(p)) => {
                        shutdown(&mut p.into_inner());
                        break;
                    }
                    Err(std::sync::TryLockError::WouldBlock) => {
                        std::thread::sleep(Duration::from_millis(50));
                    }
                }
            }
            std::process::exit(129);
        });
        signal_hook::flag::register(signal_hook::consts::SIGHUP, hup)?;
        for sig in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
            signal_hook::flag::register(sig, Arc::clone(&stop))?;
        }
    }
    #[cfg(not(unix))]
    let _ = jobs;
    Ok(stop)
}

/// A paste goes to the key entry when it is open (and only there: a key must never land in a
/// field that is drawn or saved), otherwise to the field being edited.
fn route_paste(s: &mut State, text: &str) {
    if connect::entering(s) {
        connect::paste(s, text);
    } else {
        pipeline_screen::paste(s, text);
        tools_screen::paste(s, text);
    }
}

fn context(s: &State) -> Context {
    match s.overlay {
        Some(Overlay::Confirm(_)) => Context::Dialog,
        Some(Overlay::Fix(_) | Overlay::Failure(_)) => Context::List,
        // The key is typed here: plain letters are text, not commands.
        Some(Overlay::Connect(_)) if connect::entering(s) => Context::Input,
        Some(Overlay::Connect(_)) => Context::List,
        // Both take typed text (the filter), so plain letters are not commands there.
        Some(Overlay::Palette | Overlay::Help) => Context::Input,
        None if pipeline_screen::editing(s) || tools_screen::editing(s) => Context::Input,
        None => Context::List,
    }
}

pub fn run(opts: Opts) -> Result<(), Box<dyn Error>> {
    let mouse = !opts.no_mouse && std::env::var("IGRIS_NO_MOUSE").as_deref() != Ok("1");
    let theme = Theme::detect(&Env::from_process());
    // Background commands: the guard stops them on every way out of this function.
    let jobs = Arc::new(Mutex::new(crate::job::JobSet::default()));
    let _jobs_guard = JobGuard(jobs.clone());
    let stop = watch_signals(&jobs)?;
    let _guard = term::TermGuard::enter(mouse)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(std::io::stdout()))?;
    let size = ratatui::crossterm::terminal::size()?;
    let mut state = State::new(size);
    state.repo = opts.repo.clone();
    state.home = igris_core::platform::home();
    if chain::enabled(std::env::var("IGRIS_CHAIN").ok().as_deref()) {
        state.chain = Some(chain::model::ChainUi::new());
    }
    let mut hits = HitMap::default();
    let started = std::time::Instant::now();
    let (tx, rx) = std::sync::mpsc::channel();
    let activity = Arc::new(Mutex::new(crate::activity::Activity::default()));
    state.detecting = true;
    detect_worker::spawn_real(tx.clone());
    loop {
        while let Ok(m) = rx.try_recv() {
            state.apply(m);
        }
        // An install or a download finished: scan again once no scan is running.
        // Checks the Verify step queued after its scan: start them all.
        for id in std::mem::take(&mut state.verify.queue) {
            if let Effect::Check(id) = connect::start_check(&mut state, id) {
                let mut set = jobs.lock().unwrap_or_else(PoisonError::into_inner);
                connect::spawn_check(&mut set, &tx, &activity, &state, id);
            }
        }
        if state.rescan_wanted && !state.detecting {
            state.rescan_wanted = false;
            state.detecting = true;
            detect_worker::spawn_real(tx.clone());
        }
        // Time, not loop turns: mouse movement must not speed the animation up.
        state.tick = (started.elapsed().as_millis() / 100) as usize;
        chain::sync(&mut state);
        terminal.draw(|f| draw(f, &state, &theme, &mut hits))?;
        if stop.load(Ordering::Relaxed) {
            return Ok(());
        }
        if !event::poll(Duration::from_millis(100))? {
            continue;
        }
        let action = match event::read()? {
            Event::Key(k) if k.kind != KeyEventKind::Release && state.chain.is_some() => {
                match state.chain.as_mut() {
                    Some(c) => chain::key(c, state.snapshot.as_ref(), k),
                    None => None,
                }
            }
            Event::Mouse(_) | Event::Paste(_) if state.chain.is_some() => None,
            Event::Key(k) if k.kind != KeyEventKind::Release => {
                let ctx = context(&state);
                let typed = key_to_action(ctx, k);
                if typed.is_none() && state.overlay.is_none() && pipeline_screen::editing(&state) {
                    pipeline_screen::edit_key(&mut state, k);
                    None
                } else if typed.is_none()
                    && state.overlay.is_none()
                    && tools_screen::editing(&state)
                {
                    tools_screen::edit_key(&mut state, k);
                    None
                } else {
                    typed.or_else(|| type_key(&mut state, k))
                }
            }
            Event::Paste(text) => {
                route_paste(&mut state, &text);
                None
            }
            Event::Mouse(m) => match m.kind {
                MouseEventKind::Down(MouseButton::Left) => hits.at(m.column, m.row),
                MouseEventKind::ScrollUp => Some(Action::Up),
                MouseEventKind::ScrollDown => Some(Action::Down),
                MouseEventKind::Moved => {
                    state.hover = Some((m.column, m.row));
                    None
                }
                _ => None,
            },
            Event::Resize(w, h) => {
                state.size = (w, h);
                None
            }
            _ => None,
        };
        if action.is_some() {
            state.toast = None;
        }
        match action.map(|a| update(&mut state, a)) {
            Some(Effect::Quit) => return Ok(()),
            Some(Effect::Rescan) => detect_worker::spawn_real(tx.clone()),
            Some(Effect::RunAttached(argv)) => {
                let result = attached::run_attached(argv, mouse);
                terminal.clear()?; // the screen was the child's meanwhile
                state.toast = Some(match result {
                    Ok(st) if st.success() => format!("{} finished. Scanning again.", argv[0]),
                    Ok(st) => format!("{} ended with {st}. Scanning again.", argv[0]),
                    Err(e) => format!("Could not start {}: {e}", argv[0]),
                });
                state.detecting = true;
                detect_worker::spawn_real(tx.clone());
            }
            Some(Effect::Check(id)) => {
                let mut set = jobs.lock().unwrap_or_else(PoisonError::into_inner);
                connect::spawn_check(&mut set, &tx, &activity, &state, id);
            }
            Some(Effect::Install(id)) => {
                let mut set = jobs.lock().unwrap_or_else(PoisonError::into_inner);
                install::spawn_install(&mut set, &tx, &activity, id);
            }
            Some(Effect::Pull(i)) => {
                let mut set = jobs.lock().unwrap_or_else(PoisonError::into_inner);
                install::spawn_pull(&mut set, &tx, &activity, i);
            }
            Some(Effect::StoreKey(argv)) => {
                if let Some(id) = connect::store_key(&mut state, argv, &activity)
                    && let Effect::Check(id) = connect::begin_check(&mut state, id)
                {
                    let mut set = jobs.lock().unwrap_or_else(PoisonError::into_inner);
                    connect::spawn_check(&mut set, &tx, &activity, &state, id);
                }
            }
            Some(Effect::RunChecks) => detect_worker::spawn_checks(tx.clone(), state.repo.clone()),
            Some(Effect::RunSample) => detect_worker::spawn_sample(tx.clone(), state.repo.clone()),
            Some(Effect::Write(change)) => {
                let result =
                    apply::apply(&change).map_err(|e| format!("{}: {e}", change.path.display()));
                state.finish_apply(result);
            }
            Some(Effect::Copy(text)) => {
                use std::io::Write;
                let mut out = std::io::stdout();
                let _ = out
                    .write_all(fix::osc52(&text).as_bytes())
                    .and_then(|_| out.flush());
                // Not every terminal allows OSC 52, so the command is also in words.
                state.toast = Some(format!(
                    "Copied: {text}  (if your terminal blocks copying, run it yourself)"
                ));
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;

    #[test]
    fn a_pasted_key_goes_only_to_the_key_entry() {
        use crate::tui::tools_screen::{
            Edit,
            tests::{app, both},
        };
        let mut s = app(both(), None);
        s.tools.edit = Some(Edit::Filter); // a field was being edited when the box opened
        connect::open(&mut s, "codex");
        update(&mut s, Action::Down); // codex rows: Existing login, Log in, API key
        update(&mut s, Action::Down);
        update(&mut s, Action::Activate);
        assert!(connect::entering(&s));
        route_paste(&mut s, "sk-secret\n");
        assert_eq!(s.tools.key.value(), "sk-secret");
        assert_eq!(s.tools.filter.value(), "", "the filter is drawn on screen");
        s.overlay = None;
        route_paste(&mut s, "abc");
        assert_eq!(
            s.tools.filter.value(),
            "abc",
            "other pastes still reach their field"
        );
    }

    #[test]
    fn a_model_row_shows_its_pull_command_and_the_dialogs_show_the_exact_command() {
        use crate::catalog::Group;
        use crate::tui::tools_screen::tests::{app, both};
        use igris_detect::fit::{CANDIDATES, Fit, Report, Source, Suggestion};
        let mut s = app(both(), None);
        s.apply(detect_worker::Msg::Fit(Box::new(Report {
            source: Source::Llmfit {
                version: "1.1.21".into(),
            },
            suggestions: vec![Suggestion {
                candidate: &CANDIDATES[0],
                fit: Fit::Good,
                needs_gb: Some(3.0),
                available_gb: Some(8.0),
                tps: None,
            }],
        })));
        s.tools.group = Group::Assistant;
        let at = tools_screen::rows(
            Group::Assistant,
            s.snapshot.as_ref().unwrap(),
            &s.tools,
            None,
        )
        .iter()
        .position(|r| r.model.is_some())
        .unwrap();
        s.tools.cursor[tools_screen::index(Group::Assistant)] = at;
        let (text, _) = render(100, 30, &s);
        assert!(
            text.contains("Pull: ollama pull qwen3:4b-instruct-2507-q4_K_M"),
            "{text}"
        );
        s.overlay = Some(Overlay::Confirm(state::Confirm::Pull(0)));
        let (text, _) = render(100, 30, &s);
        assert!(
            text.contains("Download qwen3:4b-instruct-2507-q4_K_M?"),
            "{text}"
        );
        s.overlay = Some(Overlay::Confirm(state::Confirm::InstallTool("herdr")));
        let (text, _) = render(100, 30, &s);
        let cmd = install::quote_cmd(crate::catalog::tool("herdr").unwrap().install.unwrap());
        assert!(
            text.contains("Install Herdr?") && text.contains(&cmd),
            "{text}"
        );
    }

    #[test]
    fn the_scan_screen_shows_the_fit_line_even_while_your_machine_is_folded() {
        use igris_detect::fit::{CANDIDATES, Fit, Report, Source, Suggestion};
        let mut s =
            crate::tui::tools_screen::tests::app(crate::tui::tools_screen::tests::both(), None);
        s.screen = Screen::Check;
        s.apply(detect_worker::Msg::Fit(Box::new(Report {
            source: Source::Llmfit {
                version: "1.1.21".into(),
            },
            suggestions: vec![Suggestion {
                candidate: &CANDIDATES[0],
                fit: Fit::Marginal,
                needs_gb: None,
                available_gb: None,
                tps: None,
            }],
        })));
        let machine = s
            .check
            .view
            .as_ref()
            .and_then(|v| v.groups.iter().position(|g| g.id == "machine"))
            .unwrap();
        assert!(!s.check.open[machine], "the group is folded");
        let (text, _) = render(100, 30, &s);
        assert!(
            text.contains("Local models: Qwen3-4B-Instruct-2507 fits tightly"),
            "{text}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn quitting_during_an_install_kills_the_child() {
        use std::process::Command;
        static SLOW: &[&str] = &["sh", "-c", "sleep 60177 & wait"];
        let alive = || {
            Command::new("pgrep")
                .args(["-f", "sleep 60177"])
                .output()
                .is_ok_and(|o| o.status.success())
        };
        let until = |want: bool| {
            (0..100).any(|_| {
                std::thread::sleep(Duration::from_millis(50));
                alive() == want
            })
        };
        let (tx, _rx) = std::sync::mpsc::channel();
        let log = Arc::new(Mutex::new(crate::activity::Activity::default()));
        let mut jobs = crate::job::JobSet::default();
        jobs.spawn(
            tx,
            crate::job::JobId(install::INSTALL_BASE),
            SLOW,
            Duration::from_secs(600),
            log,
        );
        assert!(until(true), "the install never started");
        shutdown(&mut jobs);
        assert!(until(false), "the install outlived shutdown");
    }

    #[test]
    fn typing_a_key_makes_every_letter_text_not_a_command() {
        let at = |step| {
            let mut s = State::new((100, 30));
            s.overlay = Some(Overlay::Connect(connect::Connect {
                tool: "codex",
                step,
                cursor: 0,
            }));
            s
        };
        let entry = at(connect::Step::Entry);
        assert_eq!(context(&entry), Context::Input);
        for c in ['q', 'r', 's', 'c', 'f'] {
            let k = ratatui::crossterm::event::KeyEvent::new(
                ratatui::crossterm::event::KeyCode::Char(c),
                ratatui::crossterm::event::KeyModifiers::NONE,
            );
            assert_eq!(
                key_to_action(context(&entry), k),
                None,
                "{c} would be a command"
            );
        }
        assert_eq!(context(&at(connect::Step::Methods)), Context::List);
        assert_eq!(context(&at(connect::Step::Hint)), Context::List);
        let mut failure = State::new((100, 30));
        failure.overlay = Some(Overlay::Failure("claude"));
        assert_eq!(context(&failure), Context::List);
    }

    #[test]
    fn the_failure_box_and_the_method_list_are_drawn_and_the_rows_clickable() {
        use crate::tui::tools_screen::{CheckState, Failure};
        let mut s =
            crate::tui::tools_screen::tests::app(crate::tui::tools_screen::tests::both(), None);
        s.tools.checks.insert(
            "claude".into(),
            CheckState::Failed(Failure {
                reason: "not logged in".into(),
                unchanged: "no setting was changed.".into(),
                fix: Some("claude auth login".into()),
            }),
        );
        s.overlay = Some(Overlay::Failure("claude"));
        let (text, _) = render(100, 30, &s);
        for want in [
            "✗ claude: not logged in.",
            "Unchanged: no setting was changed.",
            "Fix: claude auth login",
        ] {
            assert!(text.contains(want), "{want} missing:\n{text}");
        }
        connect::open(&mut s, "claude");
        let (text, hits) = render(100, 30, &s);
        assert!(
            text.contains("Connect Claude Code") && text.contains("1. Existing login"),
            "{text}"
        );
        let clicks = (0..30u16)
            .flat_map(|y| (0..100u16).map(move |x| (x, y)))
            .filter_map(|(x, y)| hits.at(x, y))
            .filter(|a| matches!(a, Action::Select(_)))
            .count();
        assert!(clicks > 0, "the method rows can be clicked");
    }

    fn render(w: u16, h: u16, s: &State) -> (String, HitMap) {
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        let mut hits = HitMap::default();
        let t = Theme::detect(&Env::default());
        term.draw(|f| draw(f, s, &t, &mut hits)).unwrap();
        let buf = term.backend().buffer().clone();
        let text = (0..h)
            .map(|y| {
                (0..w)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        (text, hits)
    }

    #[test]
    fn a_small_terminal_gets_only_the_size_message() {
        let (text, hits) = render(60, 20, &State::new((60, 20)));
        assert!(text.contains("at least 80x24"), "{text}");
        assert!(text.contains("now 60x20"), "{text}");
        assert!(!text.contains("Pipeline"), "nothing else is drawn: {text}");
        assert_eq!(hits.at(1, 0), None);
    }

    /// A state after the first scan, with the app opened on a repository without an igris.toml.
    fn scanned(size: (u16, u16)) -> State {
        let mut s = State::new(size);
        s.repo = std::env::temp_dir().join("igris-mod-none");
        s.apply(detect_worker::Msg::Detected(Box::new(
            crate::check::view::tests::real(),
        )));
        s
    }

    fn row_of(text: &str, needle: &str) -> (u16, u16) {
        let (y, line) = text
            .lines()
            .enumerate()
            .find(|(_, l)| l.contains(needle))
            .unwrap_or_else(|| panic!("{needle} not on screen:\n{text}"));
        (
            line[..line.find(needle).unwrap()].chars().count() as u16,
            y as u16,
        )
    }

    #[test]
    fn at_80x24_the_steps_are_a_strip_and_each_one_is_clickable() {
        let (text, hits) = render(80, 24, &scanned((80, 24)));
        for label in ["Scan", "Tools", "Pipeline", "Verify", "Review", "Test"] {
            assert!(text.contains(label), "{label}:\n{text}");
        }
        let (x, y) = row_of(&text, "Pipeline");
        assert_eq!(hits.at(x, y), Some(Action::Goto(Screen::Pipeline)));
        assert!(text.lines().all(|l| l.chars().count() <= 80));
    }

    #[test]
    fn digits_1_to_6_jump_to_the_screens() {
        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut s = scanned((100, 30));
        verify_screen::clear_all(&mut s); // Review opens only after the checks
        for (digit, screen) in ['1', '2', '3', '4', '5', '6'].into_iter().zip(Screen::ALL) {
            let k = KeyEvent::new(KeyCode::Char(digit), KeyModifiers::NONE);
            assert_eq!(
                key_to_action(Context::List, k),
                Some(Action::Goto(screen)),
                "{digit}"
            );
            update(&mut s, Action::Goto(screen));
            assert_eq!(s.screen, screen, "{digit}");
        }
    }

    #[test]
    fn the_verify_screen_lists_each_chosen_tool_and_its_rows_are_clickable() {
        let mut s =
            crate::tui::tools_screen::tests::app(crate::tui::tools_screen::tests::both(), None);
        s.screen = Screen::Verify;
        let (text, hits) = render(100, 30, &s);
        for want in [
            "Verify · 3 tools",
            "Herdr",
            "Claude Code",
            "Codex",
            "Builder",
            "Reviewer",
            "not checked yet",
        ] {
            assert!(text.contains(want), "{want}:\n{text}");
        }
        let (x, y) = row_of(&text, "Codex");
        assert_eq!(hits.at(x, y), Some(Action::Select(2)));
        assert!(
            text.contains("5  Review") && text.contains("6  Test"),
            "{text}"
        );
    }

    #[test]
    fn at_100x24_the_six_steps_and_the_progress_all_fit_in_the_rail() {
        let (text, _) = render(100, 24, &scanned((100, 24)));
        for want in [
            "1  Scan",
            "2  Tools",
            "3  Pipeline",
            "4  Verify",
            "5  Review",
            "6  Test",
            "Setup",
            "1/6",
        ] {
            assert!(text.contains(want), "{want}:\n{text}");
        }
    }

    #[test]
    fn at_100x30_the_steps_are_a_rail_beside_the_panel() {
        let (text, hits) = render(100, 30, &scanned((100, 30)));
        let (x, y) = row_of(&text, "5  Review");
        assert!(x < 22, "the rail is on the left: {x}");
        assert_eq!(hits.at(x, y), Some(Action::Goto(Screen::Review)));
        assert!(
            text.contains("╭") && text.contains(" Scan "),
            "the panel is framed and titled:\n{text}"
        );
        assert!(text.contains("Runs on your machine"), "{text}");
    }

    #[test]
    fn the_next_step_and_the_key_hints_are_on_screen_and_clickable() {
        let mut s = scanned((100, 30));
        let (text, _) = render(100, 30, &s);
        assert!(text.contains("▶ Next  Choose your assistant"), "{text}");
        s.tools.assistant = crate::tui::tools_screen::AssistantChoice::CliOnly;
        let (text, hits) = render(100, 30, &s);
        assert!(text.contains("▶ Next  Set the check command"), "{text}");
        let last = text.lines().last().unwrap();
        assert!(last.contains("q quit") && last.contains("? help"), "{last}");
        let (col, _) = row_of(&text, "? help");
        assert_eq!(hits.at(col, 29), Some(Action::Help));
    }

    #[test]
    fn before_the_first_scan_the_screen_is_the_centred_hero_with_key_hints() {
        let mut s = State::new((100, 30));
        s.scan = Some((2, 6, "agents"));
        let (text, hits) = render(100, 30, &s);
        assert!(
            text.contains("Scanning this machine · agents") && !text.contains("╭"),
            "{text}"
        );
        let (col, _) = row_of(&text, "q quit");
        assert_eq!(
            hits.at(col, 29),
            Some(Action::Quit),
            "the hints work during the scan"
        );
        s.screen = Screen::Pipeline;
        let (text, _) = render(100, 30, &s);
        assert!(
            text.contains("╭") && !text.contains("Scanning this machine"),
            "other screens use the work layout"
        );
    }

    #[test]
    fn help_is_a_box_that_closes_when_you_click_outside_it() {
        let mut s = scanned((100, 30));
        update(&mut s, Action::Help);
        let (text, hits) = render(100, 30, &s);
        assert!(
            text.contains("How setup works") && text.contains("Keys"),
            "{text}"
        );
        assert_eq!(hits.at(0, 0), Some(Action::Cancel), "outside");
        assert_eq!(hits.at(50, 15), Some(Action::Stay), "inside");
        let (x, y) = row_of(&text, "Pipeline");
        assert_ne!(
            hits.at(x, y),
            Some(Action::Goto(Screen::Pipeline)),
            "the steps are not clickable behind it"
        );
    }

    #[test]
    fn hover_tints_the_item_under_the_pointer_and_never_a_backdrop() {
        let t = Theme {
            depth: theme::ColorDepth::True,
            glyphs: theme::Glyphs::Unicode,
            motion: false,
        };
        let bg = |s: &State, x: u16, y: u16| {
            let mut term = Terminal::new(TestBackend::new(100, 30)).unwrap();
            let mut hits = HitMap::default();
            term.draw(|f| draw(f, s, &t, &mut hits)).unwrap();
            term.backend().buffer()[(x, y)].bg
        };
        let mut s = scanned((100, 30));
        let (text, _) = render(100, 30, &s);
        let (x, y) = row_of(&text, "5  Review");
        assert_eq!(
            bg(&s, x, y),
            ratatui::style::Color::Reset,
            "no pointer, no tint"
        );
        s.hover = Some((x, y));
        assert_eq!(bg(&s, x, y), ratatui::style::Color::Rgb(0x16, 0x22, 0x2b));
        update(&mut s, Action::Help);
        s.hover = Some((0, 0)); // over the backdrop of the help overlay
        assert_eq!(
            bg(&s, 0, 0),
            ratatui::style::Color::Reset,
            "a backdrop never highlights"
        );
    }

    #[test]
    fn resizing_across_the_rail_width_keeps_the_screen_and_the_open_overlay() {
        let mut s = scanned((100, 30));
        update(&mut s, Action::Goto(Screen::Pipeline));
        update(&mut s, Action::Help);
        let (wide, _) = render(100, 30, &s);
        let (narrow, _) = render(90, 30, &s);
        assert!(wide.contains("How setup works") && narrow.contains("How setup works"));
        assert_eq!(
            (s.screen, s.overlay.clone()),
            (Screen::Pipeline, Some(Overlay::Help))
        );
        for (w, h) in [(80, 24), (79, 23), (99, 29), (100, 30), (101, 31)] {
            let (text, _) = render(w, h, &s);
            assert!(
                text.lines().all(|l| l.chars().count() <= w as usize),
                "{w}x{h}"
            );
        }
    }

    #[test]
    fn the_palette_lists_actions_and_marks_the_chosen_row() {
        let mut s = scanned((100, 30));
        update(&mut s, Action::Palette);
        let (text, _) = render(100, 30, &s);
        assert!(
            text.contains("> _") && text.contains("> Go to Scan"),
            "{text}"
        );
    }

    /// Names and values come from other programs (model servers, `claude auth status`, a hand-edited
    /// igris.toml). A terminal escape sequence in one must never reach the terminal as an escape.
    #[test]
    fn escape_sequences_in_outside_text_never_reach_the_screen() {
        use crate::check::view::tests::real;
        let evil = "x\x1b]52;c;aGk=\x07\x1b[2Jy";
        let mut snap = real();
        snap.local_llm[0].name = evil.into();
        snap.local_llm[0].up = true;
        snap.local_llm[0].models = vec![evil.into()];
        snap.hermes.found = true;
        snap.hermes.provider = Some(evil.into());
        snap.agents[0].kind = evil.into();
        let mut s = State::new((100, 30));
        s.repo = std::env::temp_dir().join("igris-evil-none");
        s.apply(detect_worker::Msg::Detected(Box::new(snap)));
        s.pipeline.form.as_mut().unwrap().command = input::Input::new(evil);
        s.toast = Some(evil.into());
        let mut screens = vec![];
        for screen in Screen::ALL {
            s.screen = screen;
            screens.push(s.clone());
        }
        // The Check screen with every group open, and with a fix dialog on each row.
        let mut open = s.clone();
        open.screen = Screen::Check;
        open.check.open = vec![true; open.check.open.len()];
        screens.push(open.clone());
        for i in 0..open.check.items().len() {
            let mut d = open.clone();
            d.check.sel = i;
            update(&mut d, Action::Activate);
            screens.push(d);
        }
        let mut shown = String::new();
        for st in screens {
            let mut term = Terminal::new(TestBackend::new(100, 30)).unwrap();
            let mut hits = HitMap::default();
            let t = Theme::detect(&Env::default());
            term.draw(|f| draw(f, &st, &t, &mut hits)).unwrap();
            let buf = term.backend().buffer().clone();
            for y in 0..30u16 {
                for x in 0..100u16 {
                    let sym = buf[(x, y)].symbol();
                    shown.push_str(sym);
                    assert!(
                        !sym.chars().any(char::is_control),
                        "{:?} on {:?} at {x},{y}",
                        sym,
                        st.screen
                    );
                }
            }
        }
        // The text was drawn (it is not simply missing), minus its control characters.
        assert!(
            shown.contains("52;c;aGk="),
            "hostile text was not rendered at all"
        );
    }

    #[test]
    fn the_next_row_and_the_hints_stay_inside_the_window_on_every_screen_at_80x24() {
        let mut s = scanned((80, 24));
        s.toast = Some("A long message ".repeat(12));
        s.blocker =
            Some("log in to a coding agent with a very long name (Enter on the row) ".repeat(4));
        for screen in Screen::ALL {
            s.screen = screen;
            let (text, _) = render(80, 24, &s);
            let rows: Vec<&str> = text.lines().collect();
            for y in [21usize, 22, 23] {
                let row = rows[y];
                assert!(
                    row.ends_with(' ') && row.chars().nth(79) == Some(' '),
                    "{screen:?} row {y} touches the edge: {row:?}"
                );
                assert!(row.starts_with(' '), "{screen:?} row {y}: {row:?}");
            }
            assert!(
                rows[22].trim_end().ends_with('…'),
                "{screen:?}: a long Next sentence is shortened: {:?}",
                rows[22]
            );
        }
    }
}
