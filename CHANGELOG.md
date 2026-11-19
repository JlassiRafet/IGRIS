# Changelog

All notable changes are listed here. Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Versions: [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Changed
- `igris merge` names the files that block removing the worktree and says what to do. `scripts/smoke.sh` takes `SMOKE_AGENTS` to pick agents and models.

### Added
- `igris setup`: starts the local setup app on 127.0.0.1 and prints a one-time link. The first release has the security core only (Host and Origin checks, token to HttpOnly cookie, one fixed `detect` action, idle stop after 30 minutes) and a placeholder page; the setup screens follow. New `igris-setup` crate.
- `igris doctor --json`: one JSON document with the machine (CPU, RAM, GPU, disk), Herdr, the 24 agent kinds and their login state, Hermes, Laya, whether a Jev key is set, and local model servers. New `igris-detect` crate. Secrets, emails and account names never appear in it.
- The orchestrator (`igris-core`): a Herdr wrapper, `igris.toml` loader, rules-based decisions with an append-only decision log, and the build, review and check loop with saved state and resume.
- Commands: `igris doctor`, `igris run`, `igris continue`, `igris status`, `igris merge` (JSON on stdout, exit codes 0/1/2/3).
- `scripts/smoke.sh`: an end-to-end check with real agents on a throwaway repo.
- Rust workspace with the `igris` binary (`--version`, `--help`).
- License, community files, CI (format, lint, tests, dependency policy, secret scan).
- Website with the pipeline demo as its Home page.
