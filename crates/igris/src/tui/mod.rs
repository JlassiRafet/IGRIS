//! The terminal app: `igris` with no arguments.
//!
//! Keys and mouse clicks both become an `Action`; `state::update` is the only place state changes.
// Later tasks use what this task defines (more screens, overlays and effects).
#![allow(dead_code)]

mod action;
mod apply;
mod attached;
mod check;
mod detect_worker;
mod fix;
mod footer;
mod form;
mod header;
mod help;
mod hit;
mod input;
mod keymap;
mod next_step;
mod palette;
mod pipeline;
mod pipeline_screen;
mod review;
mod state;
mod term;
mod theme;
mod wordmark;

use action::Action;
use hit::HitMap;
use keymap::{Context, key_to_action};
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::{self, Event, KeyEventKind, MouseButton, MouseEventKind};
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};
use ratatui::{Frame, Terminal};
use state::{Effect, Overlay, Screen, State, type_key, update};
use std::error::Error;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
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
    let head = header::header(t, area.width, area.height);
    let top = head.len() as u16;
    f.render_widget(
        Paragraph::new(head),
        Rect::new(area.x, area.y, area.width, top),
    );
    let tab_y = area.y + top;
    let mut x = area.x;
    let mut tabs = Vec::new();
    for screen in Screen::ALL {
        let label = format!(" {} ", screen.title());
        let width = label.chars().count() as u16;
        if s.overlay.is_none() {
            hits.add(Rect::new(x, tab_y, width, 1), Action::Goto(screen));
        }
        let style = if screen == s.screen {
            t.accent()
        } else {
            Style::new()
        };
        tabs.push(Span::styled(label, style));
        x += width;
    }
    f.render_widget(
        Paragraph::new(Line::from(tabs)),
        Rect::new(area.x, tab_y, area.width, 1),
    );
    let body = Rect::new(area.x, tab_y + 2, area.width, area.height - top - 5);
    match s.screen {
        Screen::Check => check::draw(f, body, s, t, hits, s.overlay.is_none()),
        Screen::Pipeline => pipeline_screen::draw(f, body, s, t, hits, s.overlay.is_none()),
        Screen::Review => review::draw(f, body, s, t, hits, s.overlay.is_none()),
        other => f.render_widget(
            Paragraph::new(format!("{}: not built yet.", other.title())),
            body,
        ),
    }
    if let Some(msg) = &s.toast {
        let row = Rect::new(area.x, area.y + area.height - 3, area.width, 1);
        f.render_widget(Paragraph::new(msg.as_str()), row);
    }
    let next = Rect::new(area.x, area.y + area.height - 2, area.width, 1);
    f.render_widget(
        Paragraph::new(next_step::text(&next_step::next_step(s))),
        next,
    );
    if let Some(o) = &s.overlay {
        draw_overlay(f, area, o, s, hits);
    }
    let foot = Rect::new(area.x, area.y + area.height - 1, area.width, 1);
    footer::draw_footer(f, foot, context(s), hits);
}

/// A centred box. A click outside it closes the overlay; a click inside keeps it open.
fn draw_overlay(f: &mut Frame, area: Rect, o: &Overlay, s: &State, hits: &mut HitMap) {
    let (title, lines, w, h): (&str, Vec<Line>, u16, u16) = match o {
        Overlay::Help => (
            "Help",
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
                "Quick actions",
                lines,
                50,
                (rows.len() as u16 + 4).min(area.height),
            )
        }
        Overlay::Confirm(state::Confirm::Discard) => (
            "Discard changes?",
            vec![Line::from("You have unsaved changes. Quit anyway? (y / n)")],
            56,
            5,
        ),
        Overlay::Confirm(state::Confirm::Install) => (
            "Install Herdr?",
            vec![
                Line::from("This runs on your terminal:"),
                Line::from(format!("  {}", fix::INSTALL_HERDR.0)),
                Line::from(format!("Source: {}", fix::INSTALL_HERDR.1)),
                Line::from("Run it? (y / n)"),
            ],
            72,
            8,
        ),
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
            (d.title.as_str(), lines, 88, d.fixes.len() as u16 + 6)
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
    if let Overlay::Fix(d) = o {
        for i in 0..d.fixes.len() {
            hits.add(
                Rect::new(r.x + 1, r.y + 3 + i as u16, r.width.saturating_sub(2), 1),
                Action::Select(i),
            );
        }
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
            .block(Block::bordered().title(title)),
        r,
    );
}

/// SIGINT, SIGTERM and SIGHUP set this flag; the loop sees it and returns, so `TermGuard` drops.
fn stop_flag() -> std::io::Result<Arc<AtomicBool>> {
    let flag = Arc::new(AtomicBool::new(false));
    for sig in [
        signal_hook::consts::SIGINT,
        signal_hook::consts::SIGTERM,
        signal_hook::consts::SIGHUP,
    ] {
        signal_hook::flag::register(sig, Arc::clone(&flag))?;
    }
    Ok(flag)
}

fn context(s: &State) -> Context {
    match s.overlay {
        Some(Overlay::Confirm(_)) => Context::Dialog,
        Some(Overlay::Fix(_)) => Context::List,
        // Both take typed text (the filter), so plain letters are not commands there.
        Some(Overlay::Palette | Overlay::Help) => Context::Input,
        None if pipeline_screen::editing(s) => Context::Input,
        None => Context::List,
    }
}

pub fn run(opts: Opts) -> Result<(), Box<dyn Error>> {
    let mouse = !opts.no_mouse && std::env::var("IGRIS_NO_MOUSE").as_deref() != Ok("1");
    let theme = Theme::detect(&Env::from_process());
    let stop = stop_flag()?;
    let _guard = term::TermGuard::enter(mouse)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(std::io::stdout()))?;
    let size = ratatui::crossterm::terminal::size()?;
    let mut state = State::new(size);
    state.repo = opts.repo.clone();
    state.home = std::env::var_os("HOME").map(PathBuf::from);
    let mut hits = HitMap::default();
    let (tx, rx) = std::sync::mpsc::channel();
    state.detecting = true;
    detect_worker::spawn_real(tx.clone());
    loop {
        while let Ok(m) = rx.try_recv() {
            state.apply(m);
        }
        state.tick = state.tick.wrapping_add(1);
        terminal.draw(|f| draw(f, &state, &theme, &mut hits))?;
        if stop.load(Ordering::Relaxed) {
            return Ok(());
        }
        if !event::poll(Duration::from_millis(100))? {
            continue;
        }
        let action = match event::read()? {
            Event::Key(k) if k.kind != KeyEventKind::Release => {
                let ctx = context(&state);
                let typed = key_to_action(ctx, k);
                if typed.is_none() && state.overlay.is_none() && pipeline_screen::editing(&state) {
                    pipeline_screen::edit_key(&mut state, k);
                    None
                } else {
                    typed.or_else(|| type_key(&mut state, k))
                }
            }
            Event::Paste(text) => {
                pipeline_screen::paste(&mut state, &text);
                None
            }
            Event::Mouse(m) => match m.kind {
                MouseEventKind::Down(MouseButton::Left) => hits.at(m.column, m.row),
                MouseEventKind::ScrollUp => Some(Action::Up),
                MouseEventKind::ScrollDown => Some(Action::Down),
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

    #[test]
    fn exactly_80x24_draws_the_tabs_and_they_are_clickable() {
        let (text, hits) = render(80, 24, &State::new((80, 24)));
        assert!(
            text.contains("Check")
                && text.contains("Pipeline")
                && text.contains("Review")
                && text.contains("Test")
        );
        let row = text.lines().position(|l| l.contains("Pipeline")).unwrap() as u16;
        let col = text
            .lines()
            .nth(row as usize)
            .unwrap()
            .find("Pipeline")
            .unwrap() as u16;
        assert_eq!(
            hits.at(col, row),
            Some(Action::Goto(state::Screen::Pipeline))
        );
    }

    #[test]
    fn the_next_step_and_the_key_hints_are_on_screen_and_clickable() {
        let (text, hits) = render(100, 30, &State::new((100, 30)));
        assert!(text.contains("Next: set the check command"), "{text}");
        let last = text.lines().last().unwrap();
        assert!(last.contains("q quit") && last.contains("? help"), "{last}");
        let col = last.find("? help").unwrap() as u16;
        assert_eq!(hits.at(col, 29), Some(Action::Help));
    }

    #[test]
    fn help_is_a_box_that_closes_when_you_click_outside_it() {
        let mut s = State::new((100, 30));
        update(&mut s, Action::Help);
        let (text, hits) = render(100, 30, &s);
        assert!(
            text.contains("How setup works") && text.contains("Keys"),
            "{text}"
        );
        assert_eq!(hits.at(0, 0), Some(Action::Cancel), "outside");
        assert_eq!(hits.at(50, 15), Some(Action::Stay), "inside");
        let tab = text.lines().position(|l| l.contains("Pipeline")).unwrap() as u16;
        assert_ne!(
            hits.at(12, tab),
            Some(Action::Goto(Screen::Pipeline)),
            "the tabs are not clickable behind it"
        );
    }

    #[test]
    fn the_palette_lists_actions_and_marks_the_chosen_row() {
        let mut s = State::new((100, 30));
        update(&mut s, Action::Palette);
        let (text, _) = render(100, 30, &s);
        assert!(
            text.contains("> _") && text.contains("> Go to Check"),
            "{text}"
        );
    }
}
