//! The Check screen: what this machine has, in folding groups, and what to do about each gap.
use super::action::Action;
use super::fix::{Fix, fixes_for};
use super::hit::HitMap;
use super::state::{Effect, FixDialog, Overlay, State};
use super::style::Role;
use super::theme::{Glyphs, Theme};
use super::widgets::{chip, detail_strip, fit, pad, progress, status_cell, status_width};
use crate::check::view::{Status, View, build};
use igris_detect::Snapshot;
use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

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

fn marker(t: &Theme, selected: bool) -> Span<'static> {
    let m = match (selected, t.glyphs) {
        (false, _) => " ",
        (true, Glyphs::Unicode) => "▌",
        (true, Glyphs::Ascii) => ">",
    };
    Span::styled(m, t.style(Role::Bad))
}

/// One line of the list, exactly `width` columns, with its status in the column at the right.
fn item_line(
    t: &Theme,
    view: &View,
    ui: &CheckUi,
    item: Item,
    selected: bool,
    width: usize,
) -> Line<'static> {
    let ascii = t.glyphs == Glyphs::Ascii;
    let bold = if selected {
        Modifier::BOLD
    } else {
        Modifier::empty()
    };
    let text = t.style(Role::Text).add_modifier(bold);
    let dim = t.style(Role::Dim);
    let sw = status_width(t);
    match item {
        Item::Header(g) => {
            let group = &view.groups[g];
            let fold = match (ui.open[g], ascii) {
                (true, false) => "▾",
                (false, false) => "▸",
                (true, true) => "v",
                (false, true) => ">",
            };
            let title_w = 22;
            let summary_w = width.saturating_sub(3 + title_w + 1 + 1 + sw);
            Line::from(vec![
                marker(t, selected),
                Span::styled(fold, t.style(Role::Link)),
                Span::raw(" "),
                Span::styled(
                    pad(t, group.title, title_w),
                    text.add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(pad(t, &group.summary, summary_w), dim),
                Span::raw(" "),
                status_cell(t, group.status),
            ])
        }
        Item::Row(g, r) => {
            let row = &view.groups[g].rows[r];
            let name_w = 20;
            let detail_w = width.saturating_sub(1 + 4 + name_w + 1 + 1 + sw);
            Line::from(vec![
                marker(t, selected),
                Span::raw("    "),
                Span::styled(pad(t, &row.name, name_w), text),
                Span::raw(" "),
                Span::styled(pad(t, &row.detail, detail_w), dim),
                Span::raw(" "),
                status_cell(t, row.status),
            ])
        }
    }
}

/// The full text of the focused item: what the list had to shorten, and what Enter will do.
fn detail_lines(t: &Theme, s: &State, width: usize) -> Vec<Line<'static>> {
    let (Some(view), Some(item)) = (&s.check.view, s.check.items().get(s.check.sel).copied())
    else {
        return vec![];
    };
    let room = width * 2;
    let head = |x: &str| {
        Line::styled(
            fit(t, x, room),
            t.style(Role::Text).add_modifier(Modifier::BOLD),
        )
    };
    let body = |x: &str| Line::styled(fit(t, x, room), t.style(Role::Dim));
    match item {
        Item::Header(g) => {
            let group = &view.groups[g];
            let mut v = vec![head(group.title), body(&group.summary)];
            v.extend(
                group
                    .facts
                    .iter()
                    .map(|f| body(&format!("{}: {}", f.label, f.value))),
            );
            v.extend(group.hint.iter().map(|h| body(h)));
            v
        }
        Item::Row(g, r) => {
            let group = &view.groups[g];
            let row = &group.rows[r];
            let mut v = vec![head(&row.name), body(&row.detail)];
            if !fixes_for(group.id, row).is_empty() {
                v.push(Line::styled(
                    "Press Enter to see how to fix it.",
                    t.style(Role::Key),
                ));
            }
            v
        }
    }
}

/// Draw into `area` (the inside of the panel). Each focusable line is a clickable area when `clickable`.
pub fn draw(f: &mut Frame, area: Rect, s: &State, t: &Theme, hits: &mut HitMap, clickable: bool) {
    let Some(view) = &s.check.view else {
        f.render_widget(Paragraph::new("Scanning this machine…"), area);
        return;
    };
    let width = area.width as usize;
    // The lead sentence (two rows at most), the counts and the scan indicator, then a gap.
    let lead = fit(t, &view.lead, width * 2);
    f.render_widget(
        Paragraph::new(Line::styled(lead, t.style(Role::Text))).wrap(Wrap { trim: true }),
        Rect::new(area.x, area.y, area.width, 2.min(area.height)),
    );
    let c = &view.counts;
    let mut chips = chip(t, Status::Ready, c.ready);
    chips.push(Span::raw("   "));
    chips.extend(chip(t, Status::NeedsStep, c.needs_step));
    chips.push(Span::raw("   "));
    chips.extend(chip(t, Status::Optional, c.optional));
    let chips_row = Rect::new(area.x, area.y + 2, area.width, 1);
    f.render_widget(Paragraph::new(Line::from(chips)), chips_row);
    if s.detecting {
        let mut scan = vec![Span::styled("scanning ", t.style(Role::Dim))];
        scan.extend(progress(t, 10, None, s.tick).spans);
        f.render_widget(
            Paragraph::new(Line::from(scan)).alignment(Alignment::Right),
            chips_row,
        );
    }

    let strip_h = if area.height >= 16 { 4 } else { 0 };
    let list = Rect::new(
        area.x,
        area.y + 4,
        area.width,
        area.height.saturating_sub(4 + strip_h),
    );
    let items = s.check.items();
    // Display lines; `Some(i)` marks the ones that are items.
    let mut lines: Vec<(Line<'static>, Option<usize>)> = vec![];
    for (i, item) in items.iter().enumerate() {
        lines.push((
            item_line(t, view, &s.check, *item, i == s.check.sel, width),
            Some(i),
        ));
        // Facts, hint and the folded "more" line belong to the group and follow its last row.
        let (g, ends) = match *item {
            Item::Header(g) => (g, view.groups[g].rows.is_empty()),
            Item::Row(g, r) => (g, r + 1 == view.groups[g].rows.len()),
        };
        if ends && s.check.open[g] {
            let group = &view.groups[g];
            let mut extra: Vec<String> = group
                .facts
                .iter()
                .map(|x| format!("{}: {}", x.label, x.value))
                .collect();
            extra.extend(group.hint.iter().cloned());
            extra.extend(group.more.iter().map(|m| format!("+ {}", m.title)));
            for text in extra {
                let shown = fit(t, &text, width.saturating_sub(5));
                lines.push((
                    Line::from(vec![
                        Span::raw("     "),
                        Span::styled(shown, t.style(Role::Dim)),
                    ]),
                    None,
                ));
            }
        }
    }
    let focus = lines
        .iter()
        .position(|(_, i)| *i == Some(s.check.sel))
        .unwrap_or(0);
    let first = (focus + 1).saturating_sub(list.height as usize);
    for (row, (line, item)) in lines
        .iter()
        .skip(first)
        .take(list.height as usize)
        .enumerate()
    {
        let r = Rect::new(list.x, list.y + row as u16, list.width, 1);
        f.render_widget(Paragraph::new(line.clone()), r);
        if let (true, Some(i)) = (clickable, item) {
            hits.add(r, Action::Select(*i));
        }
    }
    if strip_h > 0 {
        let strip = Rect::new(area.x, area.bottom() - strip_h, area.width, strip_h);
        f.render_widget(detail_strip(t, detail_lines(t, s, width)), strip);
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
    fn ascii_terminals_get_only_ascii_status_and_text() {
        let s = loaded(&real());
        let t = Theme {
            motion: false,
            depth: crate::tui::theme::ColorDepth::None,
            glyphs: Glyphs::Ascii,
        };
        let text = render_with(&s, &t, 100, 24).0;
        assert!(
            text.contains("[!] needs a step") && text.contains("[x] ready"),
            "{text}"
        );
        assert!(
            text.is_ascii(),
            "nothing a non-UTF-8 terminal would garble:\n{text}"
        );
    }

    fn render_with(s: &State, t: &Theme, w: u16, h: u16) -> (String, HitMap) {
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        let mut hits = HitMap::default();
        term.draw(|f| draw(f, f.area(), s, t, &mut hits, true))
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

    fn unicode_theme() -> Theme {
        Theme::detect(&crate::tui::theme::Env::default())
    }

    #[test]
    fn the_counts_are_chips_with_a_glyph_a_number_and_a_word() {
        let s = loaded(&real());
        let (text, _) = render_with(&s, &unicode_theme(), 100, 24);
        assert!(
            text.contains("✓ 4 ready")
                && text.contains("● 1 needs a step")
                && text.contains("○ 2 optional"),
            "{text}"
        );
    }

    #[test]
    fn every_status_sits_in_one_column_at_the_right_edge() {
        let s = loaded(&real());
        for w in [74u16, 98, 140] {
            let (text, _) = render_with(&s, &unicode_theme(), w, 24);
            let status_lines: Vec<&str> = text
                .lines()
                .filter(|l| {
                    ["✓ ready", "● needs a step", "○ optional", "· info"]
                        .iter()
                        .any(|k| l.trim_end().ends_with(k))
                })
                .collect();
            assert!(status_lines.len() >= 6, "{w}: too few status rows:\n{text}");
            for l in status_lines {
                assert_eq!(
                    l.chars().count(),
                    w as usize,
                    "{w}: the status column moved: {l:?}"
                );
            }
        }
    }

    #[test]
    fn long_hostile_and_wide_names_are_shortened_and_never_push_the_status_out() {
        let evil = "x\x1b]52;c;aGk=\x07\x1b[2J";
        let long = format!("{evil}{}", "very-long-server-name-".repeat(12));
        let wide = "日本語のサーバー".repeat(12);
        for name in [long, wide] {
            let mut snap = real();
            snap.local_llm[0].name = name.clone();
            snap.local_llm[0].up = true;
            snap.local_llm[0].models = vec![name];
            let mut s = loaded(&snap);
            s.check.open = vec![true; s.check.open.len()];
            for w in [74u16, 98, 140] {
                let (text, _) = render_with(&s, &unicode_theme(), w, 30);
                assert!(!text.chars().any(|c| c.is_control() && c != '\n'), "{w}");
                let row = text
                    .lines()
                    .find(|l| l.contains("Local model servers"))
                    .expect("the row");
                assert!(
                    row.contains('…'),
                    "{w}: shortened with an ellipsis: {row:?}"
                );
                assert!(
                    row.trim_end().ends_with("✓ ready") || row.trim_end().ends_with("○ optional"),
                    "{w}: the status stayed in its column: {row:?}"
                );
            }
        }
    }

    #[test]
    fn the_focused_row_shows_its_full_text_and_what_enter_does() {
        let mut s = loaded(&real());
        let hermes = s
            .check
            .items()
            .iter()
            .position(|i| matches!(i, Item::Row(g, r) if s.check.view.as_ref().unwrap().groups[*g].rows[*r].name == "Hermes"))
            .unwrap();
        s.check.sel = hermes;
        let (text, _) = render_with(&s, &unicode_theme(), 74, 24);
        let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(
            flat.contains("Takes your requests from a chat app. Not found on this computer."),
            "{flat}"
        );
        // A row with a fix says how to reach it.
        let mut s = loaded(&logged_out());
        let claude = s
            .check
            .items()
            .iter()
            .position(|i| matches!(i, Item::Row(g, r) if s.check.view.as_ref().unwrap().groups[*g].rows[*r].name == "Claude Code"))
            .unwrap();
        s.check.sel = claude;
        let (text, _) = render_with(&s, &unicode_theme(), 74, 24);
        assert!(text.contains("Press Enter to see how to fix it."), "{text}");
    }

    #[test]
    fn a_rescan_shows_a_sweeping_indicator_without_hiding_the_list() {
        let mut s = loaded(&real());
        s.detecting = true;
        let (a, _) = render_with(&s, &unicode_theme(), 100, 24);
        s.tick = 6;
        let (b, _) = render_with(&s, &unicode_theme(), 100, 24);
        assert!(a.contains("scanning") && a.contains("Your machine"), "{a}");
        assert_ne!(a, b, "the indicator moves");
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
