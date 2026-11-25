//! Draws the Tools screen. The state and keys are in `tools_screen`.
use super::action::Action;
use super::connect::{failure_lines, passed_line, running_word};
use super::hit::HitMap;
use super::state::State;
use super::style::Role;
use super::theme::{Glyphs, Theme};
use super::tools_screen::{
    CheckState, Edit, GROUPS, HEADER, Row, ToolStatus, index, rows, same_agent,
};
use super::widgets::{detail_strip, fit, input_line, pad, status_glyph};
use crate::catalog::{Group, tool};
use crate::check::view::Status;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

/// The width of a terminal that has room for the builder and the reviewer next to each other.
const SIDE_BY_SIDE: u16 = 100;
/// The tallest the focused row's detail strip needs, and the least height that shows it.
const STRIP_H: u16 = 4;
const STRIP_MIN: u16 = 16;

fn title(g: Group) -> &'static str {
    match g {
        Group::Workspace => "Workspace",
        Group::Builder => "Builder",
        Group::Reviewer => "Reviewer",
        Group::Decision => "Decision",
        Group::Assistant => "Assistant",
    }
}

fn noun(g: Group) -> &'static str {
    match g {
        Group::Workspace => "managers",
        Group::Builder | Group::Reviewer => "agents",
        Group::Decision => "options",
        Group::Assistant => "assistants",
    }
}

/// The glyph and colour of a status. The word beside it carries the meaning; colour only helps.
fn glyph(t: &Theme, st: ToolStatus) -> (&'static str, Role) {
    let (base, role) = match st {
        ToolStatus::Ready => (Status::Ready, Role::Ok),
        ToolStatus::NeedsStep => (Status::NeedsStep, Role::Bad),
        ToolStatus::InstalledLoginUnverified | ToolStatus::Skipped => {
            (Status::Optional, Role::Optional)
        }
        ToolStatus::Checking => (Status::Info, Role::Info),
        _ => (Status::Info, Role::Dim),
    };
    (status_glyph(t, base), role)
}

fn radio(t: &Theme, chosen: bool) -> &'static str {
    match (chosen, t.glyphs) {
        (true, Glyphs::Unicode) => "(•)",
        (true, Glyphs::Ascii) => "(*)",
        (false, _) => "( )",
    }
}

fn marker(t: &Theme, on: bool) -> Span<'static> {
    let m = match (on, t.glyphs) {
        (false, _) => " ",
        (true, Glyphs::Unicode) => "▌",
        (true, Glyphs::Ascii) => ">",
    };
    Span::styled(m, t.style(Role::Bad))
}

fn len(s: &str) -> usize {
    unicode_width::UnicodeWidthStr::width(s)
}

/// One row, exactly `width` columns: marker, radio, glyph, name with its tags, and the status word
/// at the right when it fits. The focused row also shows how the tool connects, in a bracket.
fn row_line(t: &Theme, r: &Row, focused: bool, width: usize) -> Line<'static> {
    let dim = r.status.dim();
    let base = if dim {
        t.style(Role::Dim)
    } else {
        t.style(Role::Text)
    };
    let name_style = if focused {
        base.add_modifier(Modifier::BOLD)
    } else {
        base
    };
    let (g, g_role) = glyph(t, r.status);
    let name = if r.default {
        format!("{} (default)", r.name)
    } else {
        r.name.to_string()
    };
    let word = r.status.word();
    let lead = 1 + 3 + 1 + len(g) + 1;
    // Two spaces and the word, kept only when the name still fits beside them.
    let right = if lead + len(&name) + len(word) + 2 <= width {
        len(word) + 2
    } else {
        0
    };
    let room = width.saturating_sub(lead + right);
    let name_cell = pad(t, &name, len(&name).min(room));
    let mut spans = vec![
        marker(t, focused),
        Span::styled(radio(t, r.chosen), base),
        Span::raw(" "),
        Span::styled(g, t.style(g_role)),
        Span::raw(" "),
        Span::styled(name_cell, name_style),
    ];
    let used = lead + len(&name).min(room);
    let bracket = format!("  ({})", r.connects);
    let mut bracket_w = 0;
    if focused && !r.connects.is_empty() && used + len(&bracket) + right <= width {
        bracket_w = len(&bracket);
        spans.push(Span::styled(bracket, t.style(Role::Dim)));
    }
    if right > 0 {
        let gap = width.saturating_sub(used + bracket_w + len(word));
        spans.push(Span::raw(" ".repeat(gap)));
        spans.push(Span::styled(
            word,
            t.style(if dim { Role::Dim } else { g_role }),
        ));
    }
    Line::from(spans)
}

/// The header of one list: its name, how many rows, and the filter with its match count.
fn pane_header(t: &Theme, s: &State, g: Group, shown: usize, total: usize) -> Line<'static> {
    let bold = t.style(Role::Text).add_modifier(Modifier::BOLD);
    let focused = s.tools.group == g;
    let mut spans = vec![Span::styled(
        format!("{} · {total} {}", title(g), noun(g)),
        bold,
    )];
    let filter = s.tools.filter.value();
    if focused && (s.tools.edit == Some(Edit::Filter) || !filter.is_empty()) {
        let typing = s.tools.edit == Some(Edit::Filter);
        let caret = if typing { "▏" } else { "" };
        spans.push(Span::styled(
            format!("  / {}", input_line(t, &s.tools.filter, caret)),
            t.style(Role::Key),
        ));
        let word = match shown {
            0 => "(no match)".to_string(),
            1 => "(1 match)".to_string(),
            n => format!("({n} matches)"),
        };
        spans.push(Span::styled(format!("  {word}"), t.style(Role::Dim)));
    } else if focused {
        spans.push(Span::styled("  f filters · c connects", t.style(Role::Dim)));
    }
    Line::from(spans)
}

fn pane(
    f: &mut Frame,
    area: Rect,
    s: &State,
    t: &Theme,
    hits: &mut HitMap,
    clickable: bool,
    g: Group,
) {
    let Some(snap) = &s.snapshot else {
        return;
    };
    if area.height == 0 {
        return;
    }
    let list = rows(g, snap, &s.tools, s.pipeline.form.as_ref());
    let all = super::tools_screen::all_rows(g, snap, &s.tools, s.pipeline.form.as_ref()).len();
    let head = Rect::new(area.x, area.y, area.width, 1);
    f.render_widget(Paragraph::new(pane_header(t, s, g, list.len(), all)), head);
    if clickable {
        hits.add(head, Action::Select(index(g) * 1000 + HEADER));
    }
    let room = area.height.saturating_sub(1) as usize;
    let focused = s.tools.group == g;
    let at = s.tools.cursor[index(g)].min(list.len().saturating_sub(1));
    let first = (at + 1).saturating_sub(room);
    for (i, r) in list.iter().enumerate().skip(first).take(room) {
        let y = area.y + 1 + (i - first) as u16;
        let line = Rect::new(area.x, y, area.width, 1);
        f.render_widget(
            Paragraph::new(row_line(t, r, focused && i == at, area.width as usize)),
            line,
        );
        if clickable {
            hits.add(line, Action::Select(index(g) * 1000 + i));
        }
    }
    if list.is_empty() {
        let line = Rect::new(area.x, area.y + 1, area.width, 1.min(room as u16));
        f.render_widget(
            Paragraph::new(Line::styled("  nothing matches", t.style(Role::Dim))),
            line,
        );
    }
}

/// The sentence under the tabs: what the group is for, or what needs attention.
fn hint(s: &State) -> (String, Role) {
    let machine = s
        .pipeline
        .form
        .as_ref()
        .map(|f| f.herdr_machine.value().trim().to_string())
        .filter(|m| !m.is_empty());
    match s.tools.group {
        Group::Workspace => (
            format!(
                "Runs on: {} · Enter on Herdr changes where",
                machine.as_deref().unwrap_or("this computer")
            ),
            Role::Dim,
        ),
        Group::Builder | Group::Reviewer => match &s.pipeline.form {
            Some(f) if same_agent(f) => (
                "The same agent builds and reviews. A second agent catches more.".into(),
                Role::Bad,
            ),
            _ => (
                "The builder writes the code, the reviewer checks it. Enter picks a row.".into(),
                Role::Dim,
            ),
        },
        Group::Decision => (
            "Rules decide today. Jev and Laya arrive with their backend.".into(),
            Role::Dim,
        ),
        Group::Assistant => (
            "Takes your requests from chat and hands them to IGRIS. CLI only is an answer too."
                .into(),
            Role::Dim,
        ),
    }
}

/// The full text of the focused row: what a narrow list had to shorten, and what Enter does.
fn detail_lines(t: &Theme, s: &State, width: usize) -> Vec<Line<'static>> {
    let Some(snap) = &s.snapshot else {
        return vec![];
    };
    let g = s.tools.group;
    let list = rows(g, snap, &s.tools, s.pipeline.form.as_ref());
    let Some(r) = list.get(s.tools.cursor[index(g)].min(list.len().saturating_sub(1))) else {
        return vec![];
    };
    let mut head = r.name.to_string();
    if r.default {
        head.push_str(" (default)");
    }
    let mut v = vec![Line::styled(
        fit(t, &head, width),
        t.style(Role::Text).add_modifier(Modifier::BOLD),
    )];
    let word = match (r.status, tool(r.id)) {
        (ToolStatus::Checking, Some(tl)) => running_word(tl, snap),
        _ => r.status.word(),
    };
    let how = if r.connects.is_empty() {
        word.to_string()
    } else {
        format!("{} · {word}", r.connects)
    };
    v.push(Line::styled(fit(t, &how, width * 2), t.style(Role::Dim)));
    // The last live check of this tool: how fast it answered, or why it did not.
    if let Some(tl) = tool(r.id) {
        match s.tools.checks.get(r.id) {
            Some(CheckState::Passed { ms }) => {
                v.push(Line::styled(passed_line(tl, *ms), t.style(Role::Ok)));
            }
            Some(CheckState::Failed(f)) => {
                let first = failure_lines(tl, f).swap_remove(0);
                v.push(Line::styled(fit(t, &first, width * 2), t.style(Role::Bad)));
            }
            _ => {}
        }
    }
    if r.id == "herdr" {
        v.push(Line::styled(
            fit(
                t,
                "Recommended: it connects several devices and workspaces, with view and control.",
                width * 2,
            ),
            t.style(Role::Dim),
        ));
    } else if r.status == ToolStatus::NotInstalled {
        v.push(Line::styled(
            "Not installed: install it, then press r to scan again.",
            t.style(Role::Dim),
        ));
    }
    v
}

/// Draw into `area` (the inside of the panel).
pub fn draw(f: &mut Frame, area: Rect, s: &State, t: &Theme, hits: &mut HitMap, clickable: bool) {
    if s.snapshot.is_none() {
        f.render_widget(Paragraph::new("Scanning this machine…"), area);
        return;
    }
    // The tabs: the five groups in order, the focused one bold and marked.
    let mut x = area.x;
    let mut tabs: Vec<Span<'static>> = vec![];
    for (i, g) in GROUPS.iter().enumerate() {
        let on = *g == s.tools.group;
        let label = format!("{}{}", if on { "▸ " } else { "" }, title(*g));
        let label = if t.glyphs == Glyphs::Ascii {
            label.replace('▸', ">")
        } else {
            label
        };
        let w = len(&label) as u16;
        if clickable && x + w <= area.right() {
            hits.add(
                Rect::new(x, area.y, w, 1),
                Action::Select(i * 1000 + HEADER),
            );
        }
        let style = if on {
            t.style(Role::Text).add_modifier(Modifier::BOLD)
        } else {
            t.style(Role::Dim)
        };
        tabs.push(Span::styled(label, style));
        tabs.push(Span::raw("  "));
        x += w + 2;
    }
    f.render_widget(
        Paragraph::new(Line::from(tabs)),
        Rect::new(area.x, area.y, area.width, 1),
    );
    let (text, role) = hint(s);
    f.render_widget(
        Paragraph::new(Line::styled(
            fit(t, &text, area.width as usize),
            t.style(role),
        )),
        Rect::new(
            area.x,
            area.y + 1,
            area.width,
            1.min(area.height.saturating_sub(1)),
        ),
    );
    let strip = if area.height >= STRIP_MIN { STRIP_H } else { 0 };
    let top = 3.min(area.height);
    let body = Rect::new(
        area.x,
        area.y + top,
        area.width,
        area.height.saturating_sub(top + strip),
    );
    let both = matches!(s.tools.group, Group::Builder | Group::Reviewer);
    if !both {
        pane(f, body, s, t, hits, clickable, s.tools.group);
    } else if s.size.0 >= SIDE_BY_SIDE {
        let w = (body.width.saturating_sub(2)) / 2;
        let left = Rect::new(body.x, body.y, w, body.height);
        let right = Rect::new(
            body.x + w + 2,
            body.y,
            body.width.saturating_sub(w + 2),
            body.height,
        );
        pane(f, left, s, t, hits, clickable, Group::Builder);
        pane(f, right, s, t, hits, clickable, Group::Reviewer);
    } else {
        let h = body.height / 2;
        let top = Rect::new(body.x, body.y, body.width, h);
        let bottom = Rect::new(body.x, body.y + h, body.width, body.height - h);
        pane(f, top, s, t, hits, clickable, Group::Builder);
        pane(f, bottom, s, t, hits, clickable, Group::Reviewer);
    }
    if strip > 0 {
        let r = Rect::new(area.x, area.bottom() - strip, area.width, strip);
        f.render_widget(detail_strip(t, detail_lines(t, s, area.width as usize)), r);
    }
    // Typing Herdr's machine label: the prompt takes the bottom row of the list.
    if s.tools.edit == Some(Edit::Machine) && area.height > 4 {
        let y = area.bottom() - strip - 1;
        let prompt = format!(
            "Where does Herdr run? {}  (empty means this computer · Enter saves · Esc cancels)",
            input_line(t, &s.tools.machine, "▏")
        );
        f.render_widget(
            Paragraph::new(Line::styled(
                fit(t, &prompt, area.width as usize),
                t.style(Role::Key),
            )),
            Rect::new(area.x, y, area.width, 1),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::Group;
    use crate::tui::action::Action;
    use crate::tui::theme::Env;
    use crate::tui::tools_screen::tests::{app, both, snap};
    use crate::tui::tools_screen::{HEADER, ToolsUi, edit_key, index, rows};
    use igris_detect::Login;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn render(w: u16, h: u16, s: &State) -> (Vec<String>, HitMap) {
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        let mut hits = HitMap::default();
        let t = Theme::detect(&Env::default());
        term.draw(|f| draw(f, f.area(), s, &t, &mut hits, true))
            .unwrap();
        let buf = term.backend().buffer().clone();
        let lines = (0..h)
            .map(|y| {
                (0..w)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect();
        (lines, hits)
    }

    fn on(g: Group, w: u16, h: u16) -> (State, Vec<String>, HitMap) {
        let mut s = app(both(), None);
        s.size = (w, h);
        s.tools.group = g;
        let (lines, hits) = render(w, h, &s);
        (s, lines, hits)
    }

    fn row_of(lines: &[String], needle: &str) -> Option<usize> {
        lines.iter().position(|l| l.contains(needle))
    }

    #[test]
    fn the_focused_row_shows_its_methods_in_a_bracket() {
        let (_, lines, _) = on(Group::Builder, 100, 30);
        let text = lines.join("\n");
        assert!(text.contains("Claude Code"), "{text}");
        assert!(text.contains("(default)"), "{text}");
        assert!(text.contains("subscription login or API key"), "{text}");
    }

    #[test]
    fn builder_and_reviewer_sit_side_by_side_at_100_columns_and_stacked_at_80() {
        let (_, wide, _) = on(Group::Builder, 100, 30);
        let (b, r) = (
            row_of(&wide, "Builder · ").unwrap(),
            row_of(&wide, "Reviewer · ").unwrap(),
        );
        assert_eq!(b, r, "one row holds both headers: {wide:#?}");
        let (_, narrow, _) = on(Group::Builder, 80, 24);
        let (b, r) = (
            row_of(&narrow, "Builder · ").unwrap(),
            row_of(&narrow, "Reviewer · ").unwrap(),
        );
        assert!(r > b, "the reviewer sits below the builder: {narrow:#?}");
    }

    #[test]
    fn the_focused_row_is_visible_at_80x20() {
        for g in [Group::Builder, Group::Reviewer, Group::Workspace] {
            let mut s = app(both(), None);
            s.size = (80, 20);
            s.tools.group = g;
            let last = rows(
                g,
                s.snapshot.as_ref().unwrap(),
                &s.tools,
                s.pipeline.form.as_ref(),
            )
            .len()
                - 1;
            s.tools.cursor[index(g)] = last;
            let name = rows(
                g,
                s.snapshot.as_ref().unwrap(),
                &s.tools,
                s.pipeline.form.as_ref(),
            )[last]
                .name;
            let (lines, _) = render(80, 20, &s);
            assert!(
                row_of(&lines, name).is_some(),
                "{g:?}: {name} is off screen: {lines:#?}"
            );
        }
    }

    #[test]
    fn workspace_lists_herdr_and_dims_the_rest_not_supported_yet() {
        let (_, lines, _) = on(Group::Workspace, 100, 30);
        let text = lines.join("\n");
        assert!(text.contains("Herdr") && text.contains("Orca"), "{text}");
        assert!(text.contains("not supported yet"), "{text}");
        assert!(text.contains("Runs on: this computer"), "{text}");
    }

    #[test]
    fn workspace_shows_a_saved_machine_label_and_the_prompt_while_typing() {
        let mut s = app(both(), None);
        s.size = (100, 30);
        s.pipeline.form.as_mut().unwrap().herdr_machine = crate::tui::input::Input::new("devbox");
        let (lines, _) = render(100, 30, &s);
        assert!(lines.join("\n").contains("Runs on: devbox"));
        crate::tui::tools_screen::update(&mut s, Action::Activate);
        for c in "-2".chars() {
            edit_key(&mut s, KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        }
        let (lines, _) = render(100, 30, &s);
        let text = lines.join("\n");
        assert!(text.contains("devbox-2"), "{text}");
        assert!(text.contains("empty means this computer"), "{text}");
    }

    #[test]
    fn decision_shows_rules_ready_and_jev_laya_not_selectable_yet() {
        let (_, lines, _) = on(Group::Decision, 100, 30);
        let text = lines.join("\n");
        assert!(text.contains("Rules") && text.contains("ready"), "{text}");
        assert!(text.contains("Jev") && text.contains("Laya"), "{text}");
        assert_eq!(text.matches("not selectable yet").count(), 2, "{text}");
    }

    #[test]
    fn typing_filters_with_a_match_count() {
        let mut s = app(both(), None);
        s.size = (100, 30);
        s.tools.group = Group::Builder;
        crate::tui::tools_screen::update(&mut s, Action::Filter);
        for c in "cla".chars() {
            edit_key(&mut s, KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        }
        let (lines, _) = render(100, 30, &s);
        let text = lines.join("\n");
        assert!(text.contains("(1 match)"), "{text}");
        assert!(text.contains("cla"), "the typed filter is shown: {text}");
    }

    #[test]
    fn skip_for_now_is_absent_for_workspace_builder_and_reviewer() {
        for g in [Group::Workspace, Group::Builder, Group::Reviewer] {
            let (_, lines, _) = on(g, 100, 30);
            assert!(!lines.join("\n").contains("Skip for now"), "{g:?}");
        }
    }

    #[test]
    fn the_same_agent_in_both_roles_is_called_out() {
        let mut s = app(snap(&[("claude", true, Login::LoggedIn)]), None);
        s.size = (100, 30);
        s.tools.group = Group::Reviewer;
        let (lines, _) = render(100, 30, &s);
        assert!(lines.join("\n").contains("same agent"), "{lines:#?}");
        let (_, ok, _) = on(Group::Reviewer, 100, 30);
        assert!(!ok.join("\n").contains("same agent"));
    }

    #[test]
    fn every_row_is_clickable_and_has_a_key() {
        for g in [Group::Workspace, Group::Decision, Group::Assistant] {
            let (s, lines, hits) = on(g, 100, 30);
            let found: Vec<Action> = (0..30u16)
                .flat_map(|y| (0..100u16).map(move |x| (x, y)))
                .filter_map(|(x, y)| hits.at(x, y))
                .collect();
            let n = rows(
                g,
                s.snapshot.as_ref().unwrap(),
                &s.tools,
                s.pipeline.form.as_ref(),
            )
            .len();
            for r in 0..n {
                assert!(
                    found.contains(&Action::Select(index(g) * 1000 + r)),
                    "{g:?} row {r} has no click area: {lines:#?}"
                );
            }
            for other in crate::tui::tools_screen::GROUPS {
                assert!(
                    found.contains(&Action::Select(index(other) * 1000 + HEADER)),
                    "the {other:?} tab is not clickable"
                );
            }
        }
        // And a key reaches each row: Down moves one row at a time (see the logic tests).
        let _ = ToolsUi::default();
    }
}
