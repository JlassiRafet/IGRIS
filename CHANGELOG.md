# Changelog

All notable changes are listed here. Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Versions: [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Changed
- `igris merge` names the files that block removing the worktree and says what to do. `scripts/smoke.sh` takes `SMOKE_AGENTS` to pick agents and models.

### Added
- The IGRIS logo set in `assets/logo/`: smooth SVGs (original colours, full colour for dark and light backgrounds, one colour, avatar), PNG exports, a favicon, and a script that rebuilds them from the original artwork. The app and the site now use the logo as their tab icon.
- `igris setup`: starts the local setup app on 127.0.0.1 and prints a one-time link. It opens the Check setup page: your machine, Herdr and agents with their logins, the decision layer, Hermes and local model servers, in plain words, with only the groups that need a step open. A Ctrl K (⌘ K on a Mac) quick-actions menu; a terminal look in the logo's red with self-hosted JetBrains Mono (dark only). Safety: Host and Origin checks, a one-time token turned into an HttpOnly cookie, a fixed list of actions (`detect`, `check`), only the page and its files are reachable without the cookie, idle stop after 30 minutes. The later steps (pipeline, keys and logins, review and apply, test run) are not built yet. New `igris-setup` crate.
- `igris doctor --json`: one JSON document with the machine (CPU, RAM, GPU, disk), Herdr, the 24 agent kinds and their login state, Hermes, Laya, whether a Jev key is set, and local model servers. New `igris-detect` crate. Secrets, emails and account names never appear in it.
- The orchestrator (`igris-core`): a Herdr wrapper, `igris.toml` loader, rules-based decisions with an append-only decision log, and the build, review and check loop with saved state and resume.
- Commands: `igris doctor`, `igris run`, `igris continue`, `igris status`, `igris merge` (JSON on stdout, exit codes 0/1/2/3).
- `scripts/smoke.sh`: an end-to-end check with real agents on a throwaway repo.
- Rust workspace with the `igris` binary (`--version`, `--help`).
- License, community files, CI (format, lint, tests, dependency policy, secret scan).
- Website with the pipeline demo as its Home page.
