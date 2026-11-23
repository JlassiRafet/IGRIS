//! The terminal app: `igris` with no arguments.
//!
//! Keys and mouse clicks both become an `Action`; `state::update` is the only place state changes.
// Later tasks use what this task defines (more screens, overlays and effects).
#![allow(dead_code)]

mod action;
mod header;
mod hit;
mod keymap;
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
use state::{Effect, Overlay, Screen, State, update};
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
        hits.add(Rect::new(x, tab_y, width, 1), Action::Goto(screen));
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
    let body = Rect::new(area.x, tab_y + 2, area.width, area.height - top - 3);
    f.render_widget(
        Paragraph::new(format!("{}: not built yet.", s.screen.title())),
        body,
    );
    let foot = Rect::new(area.x, area.y + area.height - 1, area.width, 1);
    f.render_widget(Paragraph::new("q quit"), foot);
    if let Some(o) = &s.overlay {
        let w = 44.min(area.width);
        let r = Rect::new(
            area.x + (area.width - w) / 2,
            area.y + area.height / 2 - 2,
            w,
            5,
        );
        let text = match o {
            Overlay::Help => "Help is coming. Esc closes.",
            Overlay::Palette => "The palette is coming. Esc closes.",
            Overlay::Confirm(_) => "Quit and discard changes? y / n",
        };
        f.render_widget(Clear, r);
        f.render_widget(Paragraph::new(text).block(Block::bordered()), r);
    }
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
        Some(Overlay::Palette) => Context::Input,
        Some(Overlay::Help) => Context::Global,
        None => Context::List,
    }
}

pub fn run(opts: Opts) -> Result<(), Box<dyn Error>> {
    // The repository is used by the screens that arrive in later tasks.
    let _ = &opts.repo;
    let mouse = !opts.no_mouse && std::env::var("IGRIS_NO_MOUSE").as_deref() != Ok("1");
    let theme = Theme::detect(&Env::from_process());
    let stop = stop_flag()?;
    let _guard = term::TermGuard::enter(mouse)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(std::io::stdout()))?;
    let size = ratatui::crossterm::terminal::size()?;
    let mut state = State::new(size);
    let mut hits = HitMap::default();
    loop {
        terminal.draw(|f| draw(f, &state, &theme, &mut hits))?;
        if stop.load(Ordering::Relaxed) {
            return Ok(());
        }
        if !event::poll(Duration::from_millis(100))? {
            continue;
        }
        let action = match event::read()? {
            Event::Key(k) if k.kind != KeyEventKind::Release => key_to_action(context(&state), k),
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
        match action.map(|a| update(&mut state, a)) {
            Some(Effect::Quit) => return Ok(()),
            // The detection worker arrives with the Check screen.
            Some(Effect::Rescan) => state.detecting = false,
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
}
