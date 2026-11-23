# Contributing to IGRIS

Thanks for helping. A few rules keep the project easy to trust.

## Set up

- Rust: `rustup` installs the pinned toolchain from `rust-toolchain.toml` (1.99.1) on first build.
- Website (optional): Node 20+ in `site/`.

## Before you open a pull request

Run the same checks CI runs:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo deny check
```

- The terminal app (`crates/igris/src/tui/`) is tested at three levels: pure state and key tests, render tests on ratatui's `TestBackend`, and `scripts/tui-e2e.sh`, which drives the real binary in a detached tmux session (`cargo build && scripts/tui-e2e.sh`; needs `tmux`).
- A bug fix starts with a test that fails without the fix.
- New dependencies are pinned to an exact version (`=x.y.z`) and must pass `cargo deny`.
- Never commit secrets, `.env` files, or personal paths.

## Branches and commits

- Branch: `<type>/<area>-<task>`, type one of `feat`, `fix`, `docs`, `chore`.
- Commit subject, one line, past tense: `<Action>: <what changed> (<metrics>). <key details>.`
- Pull requests are squash-merged once CI is green.

## License

By contributing you agree your work is licensed under Apache-2.0 (section 5 of the license). No CLA.

Please follow the [Code of Conduct](CODE_OF_CONDUCT.md).
