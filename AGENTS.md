# Guidance for AI agents working in this repository

- Read `README.md` and `CONTRIBUTING.md` first. Run the four checks in CONTRIBUTING before you say a change is done.
- Tests first: a change in behaviour starts with a failing test.
- Pin new dependencies exactly and keep `cargo deny check` green. Prefer the standard library.
- `crates/igris-setup/ui/` is the only copy of the shared screens. The site and the binary consume it; never fork it.
- Never write secrets, tokens or personal paths into the repository. This repository is public.
- The local setup app must stay bound to 127.0.0.1 and keep its Host, Origin and token checks.
