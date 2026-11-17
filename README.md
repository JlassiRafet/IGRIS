# IGRIS

IGRIS runs a loop of AI tools for you. You send a request from any chat app; Hermes, your assistant, asks a decision model (Jev or Laya) which agent and model fit, hands the work to coding agents in Herdr (one builds, one reviews), runs the tests in the shell, and reports back. Anything risky waits for your yes.

**Status: early.** Nothing to install yet. The plan and the live demo are on the site: <https://jlassirafet.github.io/IGRIS/>

## What you bring

IGRIS does not resell or proxy any model. You use your own subscriptions or API keys for each tool (Claude Code, Codex, Hermes's model, Jev). IGRIS never stores those logins; each tool keeps its own.

## License

Apache-2.0. See [LICENSE](LICENSE) and [NOTICE](NOTICE).
