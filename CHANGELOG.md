# Changelog

All notable changes are listed here. Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Versions: [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Fixed
- A task that fails with "the builder left uncommitted changes" now names the files (up to ten, then `and N more`), and so does the one reminder sent to the builder, which also offers `.gitignore` for generated files. A check that leaves `__pycache__/` no longer fails without saying why.
- A blocked report names the agent that is waiting (from Herdr, else the configured one) and quotes the last lines of its screen, cleaned of escape sequences. It no longer says "Claude Code" for a Codex update prompt, and it says the answer is a key press in the Herdr pane (#82).
- The hints in `igris doctor` and the blocked report no longer promise that trusting a parent folder covers new worktrees: that was never verified.
- The next-step line of a blocked or risky task repeats the `--config` file the run was given, quoted for the shell, so the printed command runs as it stands (#83).

## [0.1.0-rc2] - 2026-11-25

The guided setup: six steps from a scan to a tested pipeline. Pre-release: it has passed CI and a scripted walk with fake agents, not yet a live run with real accounts.

### Added
- A guided setup in six steps: Scan, Tools, Pipeline, Verify, Review, Test (keys `1` to `6`). Tools lists the workspace, builder, reviewer, decision layer and assistant, with the install and connect methods each tool documents (a login, or an API key typed into a masked box that goes to the tool's own command by stdin, never argv or disk). Installs and model pulls run in the background with a timeout and a clean tail, and quitting or closing the terminal stops them.
- Local-model suggestions that fit the machine, judged by `llmfit` when it is installed (IGRIS's own list; no command is built from llmfit's output).
- Verify runs one live check per chosen tool and shows its time. Review stays locked until the chosen tools pass; the builder, reviewer and Herdr cannot be skipped.
- A **Next steps** box on the Test screen after a green sample run, and a tmux test that walks all six steps with fake agents (`scripts/tui-e2e.sh`).
- `preview/tools.png` and `preview/verify.png`; the other screenshots and the GIF are rebuilt for the six steps.

### Changed
- The Check screen is titled Scan. Pipeline no longer lists the builder and the reviewer (Tools chooses them) but still validates them.
- The assistant is a required answer; "CLI only" is one of them. The README's "Chat with it" section is no longer marked optional.

### Fixed
- A background job (install, pull, check) no longer outlives a closed terminal.

## [0.1.0-rc1] - 2026-11-23

The first pre-release: the terminal app, the orchestrator and `igris doctor`, with installers for Linux, macOS and Windows.

### Removed
- The web setup app (`igris setup`, its 127.0.0.1 server and pages). It never shipped in a release. A terminal app replaces it (see Added).

### Changed
- The README follows the torlink layout: the wordmark logo, a GIF of IGRIS in use, one section per screen with a screenshot, and a GIF of a task through the pipeline, all in `preview/` and rebuilt from the real app by `scripts/preview.py`.
- The GitHub Pages site is frozen until it is ready: Pages is off, the deploy workflow runs only by hand, and the crate homepage is the repository. The site source stays in `site/`.
- The top band says what IGRIS does and where it runs in a few words: `Build · Review · Verify` and `Runs on your machine`. The version is no longer in the band; `igris --version` has it.
- The pipeline page (`/pipeline/`) is redesigned and keeps its terminal look, top to bottom: You and Hermes (optional), then an IGRIS group holding the decision (agent, tier, risk, needs_approval) and the three Herdr panes (Claude Code builds, Codex reviews, your shell checks), then the report. Fixed-size boxes, no window frame, the real Claude Code, Codex, Hermes, Jev (TypeSafe AI), Telegram and MiniMax logos in one solid colour each, a soft glow in each tool's own colour while it works, and a tail of pixels carrying the task along the wires. Four example tasks follow the real rules (exit 3 for a risky task, 2 review rounds, 2 fix rounds). With reduced motion it shows one finished task. No hover boxes.
- The terminal app has a new look: the wordmark with a moving sheen while the first scan runs, then a step rail (or a strip in a narrow window), framed panels, aligned status columns that never clip, a detail strip, a framed input box, progress bars, hover highlight and a `Next` row above calm key hints. `IGRIS_NO_MOTION=1` stops the animation.
- The Pages Home is rebuilt in the style of the owner's reference: a white grid page, a big bold headline, numbered sections and a floating Index pill with the scroll percentage. The pipeline animation moves to `/pipeline/`. The docs take the same red and fonts. Fonts are self-hosted (Inter, JetBrains Mono); the site loads nothing from outside.
- `igris merge` names the files that block removing the worktree and says what to do. `scripts/smoke.sh` takes `SMOKE_AGENTS` to pick agents and models.

### Fixed
- `igris` now ends when its terminal is closed (SIGHUP). Before, it kept running at about 45% CPU on a dead terminal, and `scripts/tui-e2e.sh` checks it.

### Added
- `examples/hermes/igris/SKILL.md`: a Hermes skill that tells Hermes how to run a task through IGRIS (`igris run`), what the exit codes mean, and to ask you before `--approved` and before `igris merge`.
- `igris` with no arguments opens a terminal app (keyboard and mouse): Check what this machine has and fix gaps (a tool's own login, the documented Herdr install after a yes), edit `igris.toml` in a Pipeline form with live validation, Review the exact change and Apply it (the old file is kept as `.bak`), then Test with the doctor's checks and an optional sample task. Without a terminal it prints the usage and exits 2.
- The IGRIS logo set in `assets/logo/`: smooth SVGs (original colours, full colour for dark and light backgrounds, one colour, avatar), PNG exports, a favicon, and a script that rebuilds them from the original artwork. The app and the site now use the logo as their tab icon.
- `igris doctor --json`: one JSON document with the machine (CPU, RAM, GPU, disk), Herdr, the 24 agent kinds and their login state, Hermes, Laya, whether a Jev key is set, and local model servers. New `igris-detect` crate. Secrets, emails and account names never appear in it.
- The orchestrator (`igris-core`): a Herdr wrapper, `igris.toml` loader, rules-based decisions with an append-only decision log, and the build, review and check loop with saved state and resume.
- Commands: `igris doctor`, `igris run`, `igris continue`, `igris status`, `igris merge` (JSON on stdout, exit codes 0/1/2/3).
- `scripts/smoke.sh`: an end-to-end check with real agents on a throwaway repo.
- Rust workspace with the `igris` binary (`--version`, `--help`).
- License, community files, CI (format, lint, tests, dependency policy, secret scan).
- Website with the pipeline demo as its Home page.
