# IGRIS

IGRIS runs a loop of AI tools for you. You send a request from any chat app; Hermes, your assistant, asks a decision model (Jev or Laya) which agent and model fit, hands the work to coding agents in Herdr (one builds, one reviews), runs the tests in the shell, and reports back. Anything risky waits for your yes.

**Status: early.** Pre-releases are on the [Releases](https://github.com/JlassiRafet/IGRIS/releases) page.

## Install

No Rust needed. Each command downloads the archive for your machine from the Release, checks its SHA-256 against `SHA256SUMS`, and puts `igris` in a folder of your own (no administrator rights). Then run `igris`.

```sh
# macOS and Linux (pre-release: put IGRIS_VERSION=v0.1.0-rc1 before sh)
curl -fsSL https://raw.githubusercontent.com/JlassiRafet/IGRIS/main/scripts/install.sh | sh
```

```powershell
# Windows (PowerShell; pre-release: first run  $env:IGRIS_VERSION = "v0.1.0-rc1")
irm https://raw.githubusercontent.com/JlassiRafet/IGRIS/main/scripts/install.ps1 | iex
```

With Rust installed, from source: `cargo install --git https://github.com/JlassiRafet/IGRIS igris`.

## Quick start

```sh
igris            # opens the terminal app: check this machine, fix gaps, set up the pipeline, test it
igris doctor     # the same checks, headless (add --json for a machine-readable report)
```

IGRIS itself sends nothing anywhere: no telemetry, no analytics, no network library in the build. It stores no logins or keys. The app works by keyboard and mouse at the same time. `?` shows every key; `q` quits. It never stores a login or a key, and it only runs fixed commands (a tool's own login, the documented Herdr installer after you say yes). A font with Braille and block characters looks best.

 The plan and the live demo are on the site: <https://jlassirafet.github.io/IGRIS/>

## What you bring

IGRIS does not resell or proxy any model. You use your own subscriptions or API keys for each tool (Claude Code, Codex, Hermes's model, Jev). IGRIS never stores those logins; each tool keeps its own.

## License

Apache-2.0. See [LICENSE](LICENSE) and [NOTICE](NOTICE).
