//! The Check screen: what this machine has, in folding groups, and what to do about each gap.
use super::action::Action;
use super::fix::{Fix, fixes_for};
use super::hit::HitMap;
use super::state::{Effect, FixDialog, Overlay, State};
use super::theme::{Glyphs, Theme};
use crate::check::view::{Row, Status, View, build};
use igris_detect::Snapshot;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CheckUi {
    pub view: Option<View>,
    /// Folded or open, per group.
    pub open: Vec<bool>,
    /// The focused item (an index into `items()`).
    pub sel: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Item {
    Header(usize),
    Row(usize, usize),
}

impl CheckUi {
    /// Show a new snapshot. A group the user opened stays open; one that now needs a step opens.
    pub fn set(&mut self, snap: &Snapshot) {
        let v = build(snap);
        let old = std::mem::take(&mut self.open);
        self.open = v
            .groups
            .iter()
            .enumerate()
            .map(|(i, g)| g.open || old.get(i).copied().unwrap_or(false))
            .collect();
        self.view = Some(v);
        self.sel = self.sel.min(self.items().len().saturating_sub(1));
    }

    /// What can be focused, top to bottom: each group header and, when open, its rows.
    pub fn items(&self) -> Vec<Item> {
        let Some(v) = &self.view else { return vec![] };
        let mut out = vec![];
        for (g, group) in v.groups.iter().enumerate() {
            out.push(Item::Header(g));
            if self.open.get(g).copied().unwrap_or(false) {
                out.extend((0..group.rows.len()).map(|r| Item::Row(g, r)));
            }
        }
        out
    }
}

/// What stops tasks from running, in words, for the "Next:" line. Only the Build group blocks.
pub fn blocker(v: &View) -> Option<String> {
    let g = v.groups.iter().find(|g| g.id == "build")?;
    let r = g.rows.iter().find(|r| r.status == Status::NeedsStep)?;
    Some(match (r.name.as_str(), fixes_for(g.id, r).first()) {
        ("Herdr", _) => "install or update Herdr (Enter on the row)".into(),
        (_, Some(Fix::Login(_))) => format!("log in to {} (Enter on the row)", r.name),
        _ => format!("{}: {}", r.name, r.detail),
    })
}

pub fn update(s: &mut State, a: Action) -> Effect {
    let items = s.check.items();
    let last = items.len().saturating_sub(1);
    match a {
        Action::Up => s.check.sel = s.check.sel.saturating_sub(1),
        Action::Down => s.check.sel = (s.check.sel + 1).min(last),
        Action::PageUp => s.check.sel = s.check.sel.saturating_sub(8),
        Action::PageDown => s.check.sel = (s.check.sel + 8).min(last),
        Action::Home => s.check.sel = 0,
        Action::End => s.check.sel = last,
        Action::Toggle => {
            if let Some(Item::Header(g)) = items.get(s.check.sel) {
                s.check.open[*g] ^= true;
            }
        }
        Action::Select(i) => {
            s.check.sel = i.min(last);
            return activate(s);
        }
        Action::Activate => return activate(s),
        _ => {}
    }
    Effect::None
}

fn activate(s: &mut State) -> Effect {
    let items = s.check.items();
    match items.get(s.check.sel).copied() {
        Some(Item::Header(g)) => s.check.open[g] ^= true,
        Some(Item::Row(g, r)) => {
            let Some(view) = &s.check.view else {
                return Effect::None;
            };
            let (group, row) = (&view.groups[g], &view.groups[g].rows[r]);
            let fixes = fixes_for(group.id, row);
            if fixes.is_empty() {
                s.toast = Some(format!("{}: {}", row.name, row.detail));
            } else {
                s.overlay = Some(Overlay::Fix(FixDialog {
                    title: row.name.clone(),
                    detail: row.detail.clone(),
                    fixes,
                }));
                s.cursor = 0;
            }
        }
        None => {}
    }
    Effect::None
}

fn status_words(t: &Theme, st: Status) -> (&'static str, &'static str) {
    let ascii = t.glyphs == Glyphs::Ascii;
    match (st, ascii) {
        (Status::Ready, false) => ("⬢", "ready"),
        (Status::NeedsStep, false) => ("⬡", "needs a step"),
        (Status::Optional, false) => ("◇", "optional"),
        (Status::Info, false) => ("·", "info"),
        (Status::Ready, true) => ("[x]", "ready"),
        (Status::NeedsStep, true) => ("[ ]", "needs a step"),
        (Status::Optional, true) => ("[-]", "optional"),
        (Status::Info, true) => ("[i]", "info"),
    }
}

fn spinner(tick: usize, t: &Theme) -> &'static str {
    let frames: &[&str] = if t.glyphs == Glyphs::Ascii {
        &["|", "/", "-", "\\"]
    } else {
        &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]
    };
    frames[tick % frames.len()]
}

fn row_line(t: &Theme, marker: &str, r: &Row) -> String {
    let (glyph, words) = status_words(t, r.status);
    format!("{marker}   {glyph} {}  {}  [{words}]", r.name, r.detail)
}

/// Draw into `area`. Each focusable line is registered as a clickable area when `clickable`.
pub fn draw(f: &mut Frame, area: Rect, s: &State, t: &Theme, hits: &mut HitMap, clickable: bool) {
    let Some(view) = &s.check.view else {
        let msg = format!("{} Scanning this machine…", spinner(s.tick, t));
        f.render_widget(Paragraph::new(msg), area);
        return;
    };
    let ascii = t.glyphs == Glyphs::Ascii;
    let mut head = vec![Line::from(view.lead.clone())];
    let c = &view.counts;
    let scan = if s.detecting {
        format!("   {} scanning", spinner(s.tick, t))
    } else {
        String::new()
    };
    head.push(Line::from(format!(
        "{} ready · {} need a step · {} optional{scan}",
        c.ready, c.needs_step, c.optional
    )));
    head.push(Line::from(""));
    let top = head.len() as u16;
    f.render_widget(
        Paragraph::new(head),
        Rect::new(area.x, area.y, area.width, top.min(area.height)),
    );

    // Display lines; `Some(i)` marks the ones that are items.
    let items = s.check.items();
    let mut lines: Vec<(String, Option<usize>)> = vec![];
    for (i, item) in items.iter().enumerate() {
        let marker = if i == s.check.sel { ">" } else { " " };
        match *item {
            Item::Header(g) => {
                let group = &view.groups[g];
                let fold = match (s.check.open[g], ascii) {
                    (true, false) => "▾",
                    (false, false) => "▸",
                    (true, true) => "v",
                    (false, true) => ">",
                };
                let (glyph, words) = status_words(t, group.status);
                lines.push((
                    format!(
                        "{marker} {fold} {}: {}  {glyph} {words}",
                        group.title, group.summary
                    ),
                    Some(i),
                ));
            }
            Item::Row(g, r) => lines.push((row_line(t, marker, &view.groups[g].rows[r]), Some(i))),
        }
        // Facts, hint and the folded "more" line belong to the group and follow its last row.
        let (g, ends) = match *item {
            Item::Header(g) => (g, view.groups[g].rows.is_empty()),
            Item::Row(g, r) => (g, r + 1 == view.groups[g].rows.len()),
        };
        if ends && s.check.open[g] {
            let group = &view.groups[g];
            lines.extend(
                group
                    .facts
                    .iter()
                    .map(|x| (format!("        {}: {}", x.label, x.value), None)),
            );
            if let Some(h) = &group.hint {
                lines.push((format!("        {h}"), None));
            }
            if let Some(m) = &group.more {
                lines.push((format!("        + {}", m.title), None));
            }
        }
    }
    let list = Rect::new(
        area.x,
        area.y + top,
        area.width,
        area.height.saturating_sub(top),
    );
    let focus = lines
        .iter()
        .position(|(_, i)| *i == Some(s.check.sel))
        .unwrap_or(0);
    let first = (focus + 1).saturating_sub(list.height as usize);
    for (row, (text, item)) in lines
        .iter()
        .skip(first)
        .take(list.height as usize)
        .enumerate()
    {
        let r = Rect::new(list.x, list.y + row as u16, list.width, 1);
        let style = if *item == Some(s.check.sel) {
            Style::new().bold()
        } else {
            Style::new()
        };
        f.render_widget(Paragraph::new(text.clone()).style(style), r);
        if let (true, Some(i)) = (clickable, item) {
            hits.add(r, Action::Select(*i));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::view::tests::real;
    use crate::tui::state::{Overlay, State, update as app_update};
    use igris_detect::Login;
    use ratatui::{Terminal, backend::TestBackend};

    fn loaded(snap: &Snapshot) -> State {
        let mut s = State::new((100, 30));
        s.check.set(snap);
        s
    }

    fn render(s: &State, w: u16, h: u16) -> (String, HitMap) {
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        let mut hits = HitMap::default();
        let t = Theme::detect(&crate::tui::theme::Env::default());
        term.draw(|f| draw(f, f.area(), s, &t, &mut hits, true))
            .unwrap();
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

    fn logged_out() -> Snapshot {
        let mut snap = real();
        for a in &mut snap.agents {
            if a.kind == "claude" {
                a.login = Login::LoggedOut;
            }
        }
        snap
    }

    #[test]
    fn the_real_machine_shows_the_four_groups_and_says_what_needs_a_step() {
        let s = loaded(&real());
        let (text, _) = render(&s, 100, 24);
        for g in &s.check.view.as_ref().unwrap().groups {
            assert!(text.contains(g.title), "{} missing:\n{text}", g.title);
        }
        assert!(text.contains("needs a step"), "{text}");
    }

    #[test]
    fn only_the_group_with_a_gap_starts_open() {
        let s = loaded(&real());
        assert_eq!(s.check.open, [false, false, false, true]);
        let (text, _) = render(&s, 100, 24);
        assert!(
            text.contains("Hermes"),
            "the open group shows its rows:\n{text}"
        );
        assert!(
            !text.contains("Herdr  Version"),
            "a folded group does not:\n{text}"
        );
    }

    #[test]
    fn space_and_a_click_both_fold_and_unfold() {
        let mut s = loaded(&real());
        s.check.sel = 0;
        app_update(&mut s, Action::Toggle);
        assert!(s.check.open[0]);
        app_update(&mut s, Action::Toggle);
        assert!(!s.check.open[0]);
        let (_, hits) = render(&s, 100, 24);
        let row = (0..24u16)
            .find(|y| hits.at(0, *y) == Some(Action::Select(1)))
            .expect("group 1 header is clickable");
        let click = hits.at(0, row).unwrap();
        app_update(&mut s, click);
        assert!(s.check.open[1], "a click on the header unfolds it");
    }

    #[test]
    fn enter_on_a_row_that_needs_a_step_opens_the_fix_dialog() {
        let mut s = loaded(&logged_out());
        let items = s.check.items();
        let idx = items
            .iter()
            .position(|i| matches!(i, Item::Row(g, r) if s.check.view.as_ref().unwrap().groups[*g].rows[*r].name == "Claude Code"))
            .expect("the open Build group lists Claude Code");
        s.check.sel = idx;
        app_update(&mut s, Action::Activate);
        let Some(Overlay::Fix(d)) = &s.overlay else {
            panic!("{:?}", s.overlay)
        };
        assert_eq!(d.title, "Claude Code");
        assert_eq!(d.fixes[0], Fix::Login(&["claude", "auth", "login"]));
    }

    #[test]
    fn the_screen_keeps_working_while_a_scan_runs() {
        let mut s = loaded(&real());
        assert_eq!(app_update(&mut s, Action::Rescan), Effect::Rescan);
        assert!(s.detecting);
        app_update(&mut s, Action::Down);
        assert_eq!(s.check.sel, 1, "keys still move the focus");
    }

    #[test]
    fn a_new_snapshot_replaces_the_view_and_the_blocker() {
        let mut s = loaded(&real());
        s.detecting = true;
        assert_eq!(s.blocker, None);
        s.apply(super::super::detect_worker::Msg::Detected(Box::new(
            logged_out(),
        )));
        assert!(!s.detecting);
        assert_eq!(
            s.blocker.as_deref(),
            Some("log in to Claude Code (Enter on the row)")
        );
    }

    #[test]
    fn ascii_terminals_get_ascii_status_glyphs() {
        let s = loaded(&real());
        let mut term = Terminal::new(TestBackend::new(100, 24)).unwrap();
        let mut hits = HitMap::default();
        let t = Theme {
            depth: crate::tui::theme::ColorDepth::None,
            glyphs: Glyphs::Ascii,
        };
        term.draw(|f| draw(f, f.area(), &s, &t, &mut hits, true))
            .unwrap();
        let buf = term.backend().buffer().clone();
        let text: String = (0..24u16)
            .flat_map(|y| (0..100u16).map(move |x| (x, y)))
            .map(|(x, y)| buf[(x, y)].symbol().to_string())
            .collect();
        assert!(
            text.contains("[ ] needs a step") || text.contains("[ ]"),
            "ASCII needs-a-step glyph"
        );
        assert!(!text.contains('⬡') && !text.contains('▸') && !text.contains('⠋'));
    }

    #[test]
    fn nothing_to_fix_says_so_instead_of_opening_an_empty_dialog() {
        let mut s = loaded(&real());
        let hermes = s
            .check
            .items()
            .iter()
            .position(|i| matches!(i, Item::Row(g, r) if s.check.view.as_ref().unwrap().groups[*g].rows[*r].name == "Hermes"))
            .expect("the open Chat group lists Hermes");
        s.check.sel = hermes;
        app_update(&mut s, Action::Activate);
        assert_eq!(s.overlay, None);
        assert!(
            s.toast.as_deref().unwrap_or("").starts_with("Hermes"),
            "{:?}",
            s.toast
        );
    }

    #[test]
    fn choosing_a_fix_acts_on_it() {
        use crate::tui::state::{Confirm, Effect};
        let mut s = loaded(&logged_out());
        let claude = s
            .check
            .items()
            .iter()
            .position(|i| matches!(i, Item::Row(g, r) if s.check.view.as_ref().unwrap().groups[*g].rows[*r].name == "Claude Code"))
            .unwrap();
        s.check.sel = claude;
        app_update(&mut s, Action::Activate);
        assert_eq!(
            app_update(&mut s, Action::Activate),
            Effect::RunAttached(&["claude", "auth", "login"])
        );
        assert_eq!(s.overlay, None);

        // Herdr: choosing Install only asks; the command runs after a yes.
        let mut snap = real();
        snap.herdr.found = false;
        let mut s = loaded(&snap);
        let herdr = s
            .check
            .items()
            .iter()
            .position(|i| matches!(i, Item::Row(g, r) if s.check.view.as_ref().unwrap().groups[*g].rows[*r].name == "Herdr"))
            .unwrap();
        s.check.sel = herdr;
        app_update(&mut s, Action::Activate);
        assert_eq!(app_update(&mut s, Action::Activate), Effect::None);
        assert_eq!(s.overlay, Some(Overlay::Confirm(Confirm::Install)));
        assert_eq!(app_update(&mut s, Action::No), Effect::None);
        assert_eq!(s.overlay, None, "no means nothing ran");
        app_update(&mut s, Action::Activate);
        app_update(&mut s, Action::Activate);
        assert_eq!(
            app_update(&mut s, Action::Yes),
            Effect::RunAttached(crate::tui::fix::INSTALL_HERDR_ARGV)
        );
    }
}
