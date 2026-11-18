# Changelog

All notable changes are listed here. Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Versions: [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
- The orchestrator (`igris-core`): a Herdr wrapper, `igris.toml` loader, rules-based decisions with an append-only decision log, and the build, review and check loop with saved state and resume.
- Commands: `igris doctor`, `igris run`, `igris continue`, `igris status`, `igris merge` (JSON on stdout, exit codes 0/1/2/3).
- `scripts/smoke.sh`: an end-to-end check with real agents on a throwaway repo.
- Rust workspace with the `igris` binary (`--version`, `--help`).
- License, community files, CI (format, lint, tests, dependency policy, secret scan).
- Website with the pipeline demo as its Home page.
