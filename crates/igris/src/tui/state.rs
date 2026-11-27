//! The whole app state, and the one function that changes it.
use super::action::Action;
use super::apply::Change;
use super::check::CheckUi;
use super::connect::Connect;
use super::detect_worker::Msg;
use super::fix::{Fix, INSTALL_HERDR_ARGV};
use super::pipeline_screen::PipelineUi;
use super::review::ReviewUi;
use super::test_screen::TestUi;
use super::tools_screen::ToolsUi;
use super::verify_screen::VerifyUi;
use igris_detect::Snapshot;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    /// Titled "Scan": the variant keeps its first name.
    Check,
    Tools,
    Pipeline,
    Verify,
    Review,
    Test,
}

impl Screen {
    pub const ALL: [Screen; 6] = [
        Screen::Check,
        Screen::Tools,
        Screen::Pipeline,
        Screen::Verify,
        Screen::Review,
        Screen::Test,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Screen::Check => "Scan",
            Screen::Tools => "Tools",
            Screen::Pipeline => "Pipeline",
            Screen::Verify => "Verify",
            Screen::Review => "Review",
            Screen::Test => "Test",
        }
    }

    fn step(self, by: usize) -> Screen {
        let i = Screen::ALL.iter().position(|s| *s == self).unwrap_or(0);
        Screen::ALL[(i + by) % Screen::ALL.len()]
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Confirm {
    Discard,
    Install,
    SampleTask,
    /// Install this tool from the Tools step, in the background.
    InstallTool(&'static str),
    /// Pull this model (a place in the fit table) with Ollama, in the background.
    Pull(usize),
}

/// What a row that needs a step offers, kept whole so a rescan cannot change it under the dialog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FixDialog {
    pub title: String,
    pub detail: String,
    pub fixes: Vec<Fix>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Overlay {
    Help,
    Palette,
    Fix(FixDialog),
    Confirm(Confirm),
    /// The methods of one tool, then the hint or the key entry of the chosen one.
    Connect(Connect),
    /// A failed live check of this tool: why, what was left alone, the command that fixes it.
    Failure(&'static str),
}

#[derive(Clone, Debug, PartialEq)]
pub struct State {
    pub screen: Screen,
    pub overlay: Option<Overlay>,
    /// Unsaved changes: quitting asks first.
    pub dirty: bool,
    pub detecting: bool,
    pub size: (u16, u16),
    /// What blocks running tasks, in words, from the Check screen (None when nothing does).
    pub blocker: Option<String>,
    /// `checks.command` is set in the form.
    pub command_set: bool,
    /// An `igris.toml` that matches the form exists on disk.
    pub applied: bool,
    pub tested: bool,
    /// What is typed in the help filter or the palette.
    pub query: String,
    /// The palette's chosen row, or the help's scroll offset.
    pub cursor: usize,
    pub check: CheckUi,
    pub tools: ToolsUi,
    pub verify: VerifyUi,
    pub pipeline: PipelineUi,
    pub review: ReviewUi,
    pub test: TestUi,
    /// The user's home directory, for the user-level config target.
    pub home: Option<PathBuf>,
    /// The last scan, kept for screens that need it (Pipeline's agent list).
    pub snapshot: Option<Snapshot>,
    /// The repository the app was opened on.
    pub repo: PathBuf,
    /// The first scan, while it runs: probes done, total, and the one that is next.
    pub scan: Option<(usize, usize, &'static str)>,
    /// Where the pointer last was (a mouse-move event), for the hover highlight.
    pub hover: Option<(u16, u16)>,
    /// The result of the last action, in words.
    pub toast: Option<String>,
    /// Counts loop turns, for the spinner.
    pub tick: usize,
    /// A background install or download finished: scan again as soon as no scan is running.
    pub rescan_wanted: bool,
    /// The guided chain (`IGRIS_CHAIN=1`); `None` is the dashboard.
    pub chain: Option<super::chain::model::ChainUi>,
}

impl State {
    /// Take in a message from a background worker.
    pub fn apply(&mut self, m: Msg) {
        let snap = match m {
            Msg::Progress(done, total, what) => {
                self.scan = Some((done, total, what));
                return;
            }
            Msg::Detected(snap) => snap,
            Msg::Checked(list) => {
                self.test.checks = Some(list);
                self.test.running = false;
                return;
            }
            Msg::Found(ids) => {
                self.tools.found = ids.into_iter().collect();
                return;
            }
            Msg::Sampled(r) => {
                self.test.sample_running = false;
                self.tested = r.is_ok();
                self.test.sample = Some(r);
                return;
            }
            Msg::Fit(report) => {
                self.tools.fit = Some(*report);
                self.show_fit();
                return;
            }
            Msg::Job(id, end) => {
                super::install::dispatch(self, id, end);
                return;
            }
        };
        self.scan = None;
        self.check.set(&snap);
        self.show_fit();
        if self.pipeline.form.is_none() && self.pipeline.broken.is_none() {
            // The form starts once, from the first scan; later scans never overwrite edits.
            self.applied = self.pipeline.load(&self.repo, &snap);
            super::pipeline_screen::refresh(self);
        }
        self.snapshot = Some(*snap);
        if let (Some(c), Some(s)) = (self.chain.as_mut(), self.snapshot.as_ref()) {
            c.settle(s);
        }
        super::install::after_scan(self);
        super::verify_screen::after_scan(self);
        self.blocker = self.check.view.as_ref().and_then(super::check::blocker);
        self.detecting = false;
    }

    /// Put the fit line under "Your machine" (the view is rebuilt by every scan).
    fn show_fit(&mut self) {
        let line = self.tools.fit.as_ref().map(super::install::fit_line);
        self.check.set_fit(line);
    }

    pub fn new(size: (u16, u16)) -> State {
        State {
            screen: Screen::Check,
            overlay: None,
            dirty: false,
            detecting: false,
            size,
            blocker: None,
            command_set: false,
            applied: false,
            tested: false,
            query: String::new(),
            cursor: 0,
            check: CheckUi::default(),
            tools: ToolsUi::default(),
            verify: VerifyUi::default(),
            pipeline: PipelineUi::default(),
            review: ReviewUi::default(),
            test: TestUi::default(),
            home: None,
            snapshot: None,
            repo: PathBuf::from("."),
            scan: None,
            hover: None,
            toast: None,
            tick: 0,
            rescan_wanted: false,
            chain: None,
        }
    }
}

/// What the loop must do after an update.
#[derive(Debug, PartialEq, Eq)]
pub enum Effect {
    None,
    Quit,
    Rescan,
    /// Run this fixed command on the user's terminal, then rescan.
    RunAttached(&'static [&'static str]),
    /// Put this text on the clipboard.
    Copy(String),
    /// Write this change to disk (the loop does the I/O and reports back).
    Write(Change),
    /// Run the doctor's checks on a worker thread.
    RunChecks,
    /// Run the sample task on a worker thread.
    RunSample,
    /// Run the live check of this tool as a background job.
    Check(&'static str),
    /// Hand the typed key to the stdin of this fixed command (the loop takes it from the input).
    StoreKey(&'static [&'static str]),
    /// Run the install command of this tool as a background job.
    Install(&'static str),
    /// Pull this model (a place in the fit table) as a background job.
    Pull(usize),
}

pub fn update(s: &mut State, a: Action) -> Effect {
    let before = s.screen;
    let mut effect = update_inner(s, a);
    // Review opens only when every chosen tool is clear. One place, so a digit, `]`, `[` (Test
    // wraps back to Review), a click and the palette are all covered. Only Review is locked.
    if s.screen == Screen::Review && before != Screen::Review {
        let pending = super::verify_screen::pending_names(s);
        if !pending.is_empty() {
            s.screen = before;
            s.toast = Some(format!(
                "Review is locked until these pass or are skipped (key 4, Verify): {}.",
                pending.join(", ")
            ));
        }
    }
    if s.screen == Screen::Review {
        super::review::refresh(s);
    }
    // Opening the Test screen runs the checks, so what it shows is always fresh.
    if s.screen == Screen::Test && before != Screen::Test && effect == Effect::None {
        effect = super::test_screen::rerun(s);
    }
    // Opening Verify scans again, then checks the tools that have no result yet.
    if s.screen == Screen::Verify && before != Screen::Verify && effect == Effect::None {
        effect = super::verify_screen::enter(s);
    }
    effect
}

fn update_inner(s: &mut State, a: Action) -> Effect {
    let discard = Some(Overlay::Confirm(Confirm::Discard));
    match a {
        Action::Quit if s.overlay == discard => Effect::Quit,
        Action::Quit if s.dirty => {
            s.overlay = discard;
            Effect::None
        }
        Action::Quit => Effect::Quit,
        Action::Yes if s.overlay == discard => Effect::Quit,
        Action::Yes if s.overlay == Some(Overlay::Confirm(Confirm::SampleTask)) => {
            s.overlay = None;
            s.test.sample_running = true;
            s.test.sample = None;
            Effect::RunSample
        }
        Action::Yes if matches!(s.overlay, Some(Overlay::Confirm(Confirm::InstallTool(_)))) => {
            super::install::start_install(s)
        }
        Action::Yes if matches!(s.overlay, Some(Overlay::Confirm(Confirm::Pull(_)))) => {
            super::install::start_pull(s)
        }
        Action::Yes if s.overlay == Some(Overlay::Confirm(Confirm::Install)) => {
            s.overlay = None;
            Effect::RunAttached(INSTALL_HERDR_ARGV)
        }
        // Before the generic close below: Esc here goes back a step, and r and s are answers.
        _ if matches!(s.overlay, Some(Overlay::Connect(_) | Overlay::Failure(_))) => {
            super::connect::update(s, a)
        }
        Action::No | Action::Cancel if s.overlay.is_some() => {
            s.overlay = None;
            Effect::None
        }
        Action::Up | Action::Down if matches!(s.overlay, Some(Overlay::Fix(_))) => {
            let Some(Overlay::Fix(d)) = &s.overlay else {
                return Effect::None;
            };
            let last = d.fixes.len().saturating_sub(1);
            s.cursor = if a == Action::Up {
                s.cursor.saturating_sub(1)
            } else {
                (s.cursor + 1).min(last)
            };
            Effect::None
        }
        Action::Activate if matches!(s.overlay, Some(Overlay::Fix(_))) => choose(s, s.cursor),
        Action::Select(i) if matches!(s.overlay, Some(Overlay::Fix(_))) => choose(s, i),
        // An open overlay is modal: only the answers above and Quit get through.
        _ if s.overlay.is_some() => Effect::None,
        Action::Help => open(s, Overlay::Help),
        Action::Palette => open(s, Overlay::Palette),
        Action::Goto(screen) => {
            s.screen = screen;
            Effect::None
        }
        Action::NextScreen => {
            s.screen = s.screen.step(1);
            Effect::None
        }
        Action::PrevScreen => {
            s.screen = s.screen.step(Screen::ALL.len() - 1);
            Effect::None
        }
        // On Verify, `r` also checks every tool again once the scan lands.
        Action::Rescan if s.screen == Screen::Verify => super::verify_screen::recheck_all(s),
        Action::Rescan if !s.detecting => {
            s.detecting = true;
            Effect::Rescan
        }
        Action::Up
        | Action::Down
        | Action::PageUp
        | Action::PageDown
        | Action::Home
        | Action::End
        | Action::Toggle
        | Action::Activate
        | Action::Select(_)
            if s.screen == Screen::Check =>
        {
            super::check::update(s, a)
        }
        Action::Up
        | Action::Down
        | Action::PageUp
        | Action::PageDown
        | Action::Home
        | Action::End
        | Action::NextField
        | Action::PrevField
        | Action::Toggle
        | Action::Activate
        | Action::Cancel
        | Action::Filter
        | Action::Connect
        | Action::Select(_)
            if s.screen == Screen::Tools =>
        {
            super::tools_screen::update(s, a)
        }
        Action::Up
        | Action::Down
        | Action::PageUp
        | Action::PageDown
        | Action::Home
        | Action::End
        | Action::NextField
        | Action::PrevField
        | Action::Toggle
        | Action::Activate
        | Action::Cancel
        | Action::Select(_)
            if s.screen == Screen::Pipeline =>
        {
            super::pipeline_screen::update(s, a)
        }
        Action::Up
        | Action::Down
        | Action::PageUp
        | Action::PageDown
        | Action::Home
        | Action::End
        | Action::NextField
        | Action::PrevField
        | Action::Toggle
        | Action::Activate
        | Action::Select(_)
            if s.screen == Screen::Review =>
        {
            super::review::update(s, a)
        }
        Action::Up
        | Action::Down
        | Action::Home
        | Action::End
        | Action::Toggle
        | Action::Activate
        | Action::Skip
        | Action::Select(_)
            if s.screen == Screen::Verify =>
        {
            super::verify_screen::update(s, a)
        }
        Action::Up
        | Action::Down
        | Action::Home
        | Action::End
        | Action::NextField
        | Action::PrevField
        | Action::Toggle
        | Action::Activate
        | Action::Select(_)
            if s.screen == Screen::Test =>
        {
            super::test_screen::update(s, a)
        }
        _ => Effect::None,
    }
}

/// Do the chosen fix of the open fix dialog.
fn choose(s: &mut State, i: usize) -> Effect {
    let Some(Overlay::Fix(d)) = s.overlay.clone() else {
        return Effect::None;
    };
    let Some(fix) = d.fixes.get(i).cloned() else {
        return Effect::None;
    };
    match fix {
        Fix::Login(argv) => {
            s.overlay = None;
            Effect::RunAttached(argv)
        }
        // Never run on the first press: show the command and ask.
        Fix::Install { .. } => {
            s.overlay = Some(Overlay::Confirm(Confirm::Install));
            Effect::None
        }
        Fix::Docs(url) => {
            s.overlay = None;
            s.toast = Some(format!("Docs: {url}"));
            Effect::None
        }
        Fix::Copy(text) => {
            s.overlay = None;
            Effect::Copy(text)
        }
    }
}

fn open(s: &mut State, o: Overlay) -> Effect {
    s.overlay = Some(o);
    s.query.clear();
    s.cursor = 0;
    Effect::None
}

/// Keys typed into the help filter or the palette. Returns an action when one is chosen.
pub fn type_key(s: &mut State, key: KeyEvent) -> Option<Action> {
    if super::connect::entering(s) {
        return super::connect::entry_key(s, key);
    }
    let palette = match s.overlay {
        Some(Overlay::Palette) => true,
        Some(Overlay::Help) => false,
        _ => return None,
    };
    let last = if palette {
        super::palette::entries(&s.query).len().saturating_sub(1)
    } else {
        super::help::lines(&s.query).len().saturating_sub(1)
    };
    match key.code {
        KeyCode::Esc => return Some(Action::Cancel),
        KeyCode::Backspace => {
            s.query.pop();
            s.cursor = 0;
        }
        KeyCode::Up => s.cursor = s.cursor.saturating_sub(1),
        KeyCode::Down => s.cursor = (s.cursor + 1).min(last),
        KeyCode::PageUp => s.cursor = s.cursor.saturating_sub(10),
        KeyCode::PageDown => s.cursor = (s.cursor + 10).min(last),
        KeyCode::Home => s.cursor = 0,
        KeyCode::End => s.cursor = last,
        KeyCode::Enter if palette => {
            let chosen = super::palette::entries(&s.query).get(s.cursor).map(|e| e.0);
            if chosen.is_some() {
                s.overlay = None;
            }
            return chosen;
        }
        KeyCode::Char(c)
            if !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
        {
            s.query.push(c);
            s.cursor = 0;
        }
        _ => {}
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> State {
        State::new((100, 30))
    }

    #[test]
    fn quit_asks_first_when_there_are_unsaved_changes() {
        let mut s = state();
        s.dirty = true;
        assert_eq!(update(&mut s, Action::Quit), Effect::None);
        assert_eq!(s.overlay, Some(Overlay::Confirm(Confirm::Discard)));
        assert_eq!(
            update(&mut s, Action::Quit),
            Effect::Quit,
            "a second press quits"
        );
    }

    #[test]
    fn quit_is_immediate_when_clean() {
        let mut s = state();
        assert_eq!(update(&mut s, Action::Quit), Effect::Quit);
    }

    #[test]
    fn the_discard_dialog_answers() {
        let mut s = state();
        s.dirty = true;
        update(&mut s, Action::Quit);
        assert_eq!(update(&mut s, Action::No), Effect::None);
        assert_eq!(s.overlay, None);
        update(&mut s, Action::Quit);
        assert_eq!(update(&mut s, Action::Yes), Effect::Quit);
    }

    #[test]
    fn cancel_closes_an_overlay() {
        let mut s = state();
        update(&mut s, Action::Help);
        assert_eq!(s.overlay, Some(Overlay::Help));
        update(&mut s, Action::Cancel);
        assert_eq!(s.overlay, None);
        update(&mut s, Action::Palette);
        assert_eq!(s.overlay, Some(Overlay::Palette));
    }

    #[test]
    fn screens_wrap_and_jump() {
        let mut s = state();
        update(&mut s, Action::Goto(Screen::Test));
        assert_eq!(s.screen, Screen::Test);
        update(&mut s, Action::NextScreen);
        assert_eq!(s.screen, Screen::Check, "Test wraps to Check");
        update(&mut s, Action::PrevScreen);
        assert_eq!(s.screen, Screen::Test, "Check wraps back to Test");
    }

    #[test]
    fn an_open_overlay_blocks_navigation_but_not_closing() {
        let mut s = state();
        update(&mut s, Action::Help);
        assert_eq!(update(&mut s, Action::Goto(Screen::Review)), Effect::None);
        assert_eq!(s.screen, Screen::Check);
        assert_eq!(update(&mut s, Action::Rescan), Effect::None);
        assert_eq!(update(&mut s, Action::Quit), Effect::Quit);
    }

    #[test]
    fn rescan_does_not_stack() {
        let mut s = state();
        assert_eq!(update(&mut s, Action::Rescan), Effect::Rescan);
        assert!(s.detecting);
        assert_eq!(update(&mut s, Action::Rescan), Effect::None);
    }

    #[test]
    fn opening_help_or_the_palette_starts_with_an_empty_filter() {
        let mut s = state();
        update(&mut s, Action::Palette);
        type_key(
            &mut s,
            KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE),
        );
        assert_eq!(s.query, "r");
        update(&mut s, Action::Cancel);
        update(&mut s, Action::Help);
        assert_eq!((s.query.as_str(), s.cursor), ("", 0));
    }

    #[test]
    fn a_click_inside_an_overlay_keeps_it_open() {
        let mut s = state();
        update(&mut s, Action::Help);
        assert_eq!(update(&mut s, Action::Stay), Effect::None);
        assert_eq!(s.overlay, Some(Overlay::Help));
    }

    #[test]
    fn the_palette_runs_the_chosen_action() {
        let mut s = state();
        update(&mut s, Action::Palette);
        for c in "rescan".chars() {
            assert_eq!(
                type_key(&mut s, KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)),
                None
            );
        }
        let chosen = type_key(&mut s, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(chosen, Some(Action::Rescan));
        assert_eq!(s.overlay, None, "closed before the action runs");
        assert_eq!(update(&mut s, Action::Rescan), Effect::Rescan);
    }

    #[test]
    fn enter_with_no_match_keeps_the_palette_open_and_esc_closes_it() {
        let mut s = state();
        update(&mut s, Action::Palette);
        type_key(
            &mut s,
            KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE),
        );
        assert_eq!(
            type_key(&mut s, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            None
        );
        assert_eq!(s.overlay, Some(Overlay::Palette));
        assert_eq!(
            type_key(&mut s, KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            Some(Action::Cancel)
        );
    }

    #[test]
    fn the_help_scroll_stays_inside_the_text() {
        let mut s = state();
        update(&mut s, Action::Help);
        for _ in 0..500 {
            type_key(&mut s, KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        }
        assert_eq!(s.cursor, crate::tui::help::lines("").len() - 1);
    }
}
