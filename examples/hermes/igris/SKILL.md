---
name: igris
description: "Use when the user asks for a coding task in a git repository: run it through IGRIS (a builder agent, a reviewer agent, then the user's checks)."
version: 0.1.0
license: Apache-2.0
platforms: [linux, macos, windows]
metadata:
  hermes:
    tags: [Coding, Orchestration, igris]
---

# IGRIS

IGRIS runs one coding task through a builder agent, a reviewer agent and the user's own check command. You start it with the terminal tool. You never write the code yourself.

## Run a task

1. Check the machine once: `igris doctor --repo <path>`. Exit code 0 means ready. If it is not, tell the user which line failed.
2. Start the task with one short sentence: `igris run --repo <path> --task "<what to do>"`. It prints one JSON document and ends with an exit code.

| Exit code | Meaning | What you do |
|---|---|---|
| 0 | Done: approved and the checks passed | Report `verdict`, `files_changed` and the check result. |
| 3 | Not done yet: `status` is `needs_approval` or `blocked` | Read `reason` and `next`, tell the user, wait. |
| 1 | Failed | Report `reason`. Do not retry on your own. |
| 2 | Wrong use | Fix the command. |

## Rules

- `needs_approval`: the task looks risky (a migration, auth, secrets, SQL). Say why and ask. Only after the user says yes, run the same command again with `--approved`. Never pass `--approved` unasked.
- `blocked`: an agent is asking something in Herdr (for example "trust this folder"). Tell the user to answer it in Herdr, then run `igris continue <id>`.
- After a done task, ask before merging. Merge only on a clear yes: `igris merge <id>` (fast-forward only).
- `igris status` lists tasks, `igris status <id>` shows one.
