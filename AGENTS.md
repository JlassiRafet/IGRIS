# Guidance for AI agents working in this repository

- Read `README.md` and `CONTRIBUTING.md` first. Run the four checks in CONTRIBUTING before you say a change is done.
- Tests first: a change in behaviour starts with a failing test.
- Pin new dependencies exactly and keep `cargo deny check` green. Prefer the standard library.
- `site/pipeline-src/` is the only copy of the pipeline page; the site copies it in at build (`site/scripts/sync-ui.mjs`). Never fork it.
- Never write secrets, tokens or personal paths into the repository. This repository is public.
- The terminal app (`igris` with no arguments) stores no secrets and runs only fixed commands, never one taken from user text or a config file. Without a terminal on stdin and stdout it prints usage and exits 2.
