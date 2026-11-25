#!/usr/bin/env bash
# End-to-end check of the terminal app in a real (detached) tmux terminal: keys, mouse, help, resize,
# quit. Nothing outside a temporary folder is written, and no real agent runs: the main session has
# fake `herdr`, `claude` and `codex` first on PATH, so the Verify step's live checks are a
# `sleep 60174` that quitting must kill.
# Usage: cargo build && scripts/tui-e2e.sh      (IGRIS=/path/to/igris to test another binary)
set -euo pipefail
BIN=${IGRIS:-target/debug/igris}
[ -x "$BIN" ] || { echo "no binary at $BIN: run cargo build first"; exit 2; }
BIN=$(cd "$(dirname "$BIN")" && pwd)/$(basename "$BIN")
S=igris-e2e-$$
REPO=$(mktemp -d)
HOME_DIR=$(mktemp -d)
FAKES=$(mktemp -d)
trap 'tmux kill-session -t "$S" 2>/dev/null || true; rm -rf "$REPO" "$HOME_DIR" "$FAKES"' EXIT
# Fakes: the scan's probes answer at once (not logged in); the live check of an agent sleeps; Herdr
# answers its version.
printf '#!/bin/sh\ncase "$1" in -p) exec sleep 60174 ;; *) echo claude; exit 1 ;; esac\n' > "$FAKES/claude"
printf '#!/bin/sh\ncase "$1" in exec) exec sleep 60174 ;; *) echo codex; exit 1 ;; esac\n' > "$FAKES/codex"
printf '#!/bin/sh\necho "herdr 0.9.1"\n' > "$FAKES/herdr"
chmod +x "$FAKES/claude" "$FAKES/codex" "$FAKES/herdr"
git -C "$REPO" init -q -b main

screen() { tmux capture-pane -t "$S" -p; }
fail() { echo "FAIL: $1"; echo "---- screen ----"; screen; exit 1; }
wait_for() { # pattern, seconds
  for _ in $(seq 1 $((${2:-10} * 10))); do screen | grep -q -- "$1" && return 0; sleep 0.1; done
  fail "waited for '$1'"
}
gone() { # pattern, seconds
  for _ in $(seq 1 $((${2:-5} * 10))); do screen | grep -q -- "$1" || return 0; sleep 0.1; done
  fail "'$1' should be gone"
}
click() { tmux send-keys -t "$S" -l "$(printf '\033[<0;%s;%sM\033[<0;%s;%sm' "$1" "$2" "$1" "$2")"; }
wheel_down() { tmux send-keys -t "$S" -l "$(printf '\033[<65;%s;%sM' "$1" "$2")"; }

# Without a terminal the app prints usage and exits 2: it never waits for input.
code=0
"$BIN" </dev/null >/dev/null 2>&1 || code=$?
[ "$code" = 2 ] || { echo "FAIL: no terminal: expected exit 2, got $code"; exit 1; }
echo "ok  no terminal: usage and exit 2"

tmux new-session -d -s "$S" -x 100 -y 30 \
  "env PATH=$FAKES:\$PATH HOME=$HOME_DIR COLORTERM=truecolor '$BIN' --repo '$REPO'; echo EXIT=\$?; sleep 60"

wait_for "▀█▀ █▀▀ █▀█" 10   # the wordmark
wait_for "Pipeline" 5       # the tab strip
wait_for "optional" 60          # the scan finished: the count chips are on screen
echo "ok  wordmark, tabs, scan"
wait_for "Runs on your machine" 5   # the second line of the top band
wait_for "Setup" 5          # the step rail and its progress
echo "ok  the privacy line and the step rail"

tmux send-keys -t "$S" 2;   wait_for "managers" 5
echo "ok  2 opens the Tools step"
tmux send-keys -t "$S" 3;   wait_for "Check command" 5
echo "ok  3 opens the Pipeline form"
tmux send-keys -t "$S" '?'; wait_for "How setup works" 5
tmux send-keys -t "$S" Escape; gone "How setup works" 5
echo "ok  ? opens help, Esc closes it"
tmux send-keys -t "$S" ']'; wait_for "Verify ·" 5
tmux send-keys -t "$S" '[';  wait_for "Check command" 5
echo "ok  ] and [ change screen (] from Pipeline opens Verify)"

# Mouse. At 100x30 the steps are a rail: rows 6 to 11 (the wordmark band takes rows 1 to 4, then a rule).
click 10 9; wait_for "Verify ·" 5                       # Verify
click 10 10; wait_for "Review is locked" 5              # Review stays locked until the tools pass
click 10 11; wait_for "Run the checks again" 5          # Test is not locked
click 10 6; wait_for "optional" 5                       # Scan
echo "ok  click on a step switches screen, and Review is locked until the tools pass"
tmux send-keys -t "$S" 3; wait_for "Check command" 5
wheel_down 20 12; wait_for "▌ Base branch" 5
echo "ok  the wheel moves the focus"

tmux resize-window -t "$S" -x 70 -y 20; wait_for "at least 80x24" 5
tmux resize-window -t "$S" -x 100 -y 30; wait_for "Check command" 5
echo "ok  too small a window says so, and recovers"
tmux resize-window -t "$S" -x 90 -y 30; wait_for "/6 done" 5   # under 100 columns the rail is a strip
tmux resize-window -t "$S" -x 100 -y 30; wait_for "Setup" 5
echo "ok  under 100 columns the rail becomes a strip, and back"

# Verify started the live checks of the chosen agents (the fakes sleep): they are running now.
started=0
for _ in $(seq 1 100); do pgrep -f "sleep 60174" >/dev/null && { started=1; break; }; sleep 0.1; done
[ "$started" = 1 ] || fail "Verify never started its live checks"
echo "ok  Verify starts the live checks of the chosen tools"
tmux send-keys -t "$S" q;   wait_for "EXIT=0" 5
echo "ok  q exits with code 0"
# The live checks that Verify started are still running when q is pressed: quitting must kill them.
gone_process() { # pattern, seconds
  for _ in $(seq 1 $((${2:-5} * 10))); do pgrep -f "$1" >/dev/null || return 0; sleep 0.1; done
  return 1
}
gone_process "sleep 60174" 5 || { pkill -f "sleep 60174" || true; echo "FAIL: a live check outlived q"; exit 1; }
echo "ok  quitting kills the live checks Verify started"

# Closing the terminal must end igris. It once kept running at about 45% CPU on a dead terminal.
HUP_REPO=$(mktemp -d)
git -C "$HUP_REPO" init -q -b main
tmux new-session -d -s "$S-hup" -x 100 -y 30 "env HOME=$HOME_DIR COLORTERM=truecolor '$BIN' --repo '$HUP_REPO'; sleep 30"
for _ in $(seq 1 100); do tmux capture-pane -t "$S-hup" -p | grep -q "Pipeline" && break; sleep 0.1; done
tmux kill-session -t "$S-hup"
ended=0
for _ in $(seq 1 30); do pgrep -f -- "--repo $HUP_REPO" >/dev/null || { ended=1; break; }; sleep 0.1; done
if [ "$ended" != 1 ]; then pkill -f -- "--repo $HUP_REPO" || true; rm -rf "$HUP_REPO"; echo "FAIL: igris kept running after its terminal closed"; exit 1; fi
rm -rf "$HUP_REPO"
echo "ok  closing the terminal ends igris"

# Closing the terminal must also kill a running live check. The fake `claude` answers the scan's probes
# at once and sleeps for its one check call, so a left-over `sleep 60173` means the job outlived igris.
FAKE=$(mktemp -d)
printf '#!/bin/sh\ncase "$1" in -p) exec sleep 60173 ;; *) echo claude; exit 1 ;; esac\n' > "$FAKE/claude"
chmod +x "$FAKE/claude"
JOB_REPO=$(mktemp -d)
git -C "$JOB_REPO" init -q -b main
tmux new-session -d -s "$S-job" -x 100 -y 30 "env PATH=$FAKE:\$PATH HOME=$HOME_DIR COLORTERM=truecolor '$BIN' --repo '$JOB_REPO'; sleep 30"
for _ in $(seq 1 600); do tmux capture-pane -t "$S-job" -p | grep -q "optional" && break; sleep 0.1; done
tmux send-keys -t "$S-job" 2 Tab c End Enter
for _ in $(seq 1 100); do pgrep -f "sleep 60173" >/dev/null && break; sleep 0.1; done
pgrep -f "sleep 60173" >/dev/null || { tmux capture-pane -t "$S-job" -p; rm -rf "$FAKE" "$JOB_REPO"; echo "FAIL: the live check never started"; exit 1; }
tmux kill-session -t "$S-job"
killed=0
for _ in $(seq 1 50); do pgrep -f "sleep 60173" >/dev/null || { killed=1; break; }; sleep 0.1; done
if [ "$killed" != 1 ]; then pkill -f "sleep 60173" || true; rm -rf "$FAKE" "$JOB_REPO"; echo "FAIL: a running check outlived its terminal"; exit 1; fi
rm -rf "$FAKE" "$JOB_REPO"
echo "ok  closing the terminal kills a running check"

# The whole walk, with fakes that answer at once: Scan, Tools (CLI only), Pipeline (a command), Verify
# (every check passes and shows its latency), Review (unlocked: it writes igris.toml) and Test.
PASS=$(mktemp -d)
WALK=$(mktemp -d)
WHOME=$(mktemp -d)
git -C "$WALK" init -q -b main
printf '#!/bin/sh\ncase "$1" in -p) echo ok ;; *) echo claude; exit 1 ;; esac\n' > "$PASS/claude"
printf '#!/bin/sh\ncase "$1" in exec) echo ok ;; *) echo codex; exit 1 ;; esac\n' > "$PASS/codex"
printf '#!/bin/sh\necho "herdr 0.9.1"\n' > "$PASS/herdr"
chmod +x "$PASS/claude" "$PASS/codex" "$PASS/herdr"
W="$S-walk"
tmux new-session -d -s "$W" -x 100 -y 30 "env PATH=$PASS:\$PATH HOME=$WHOME COLORTERM=truecolor '$BIN' --repo '$WALK'; sleep 30"
walk_fail() { echo "FAIL: walk: $1"; tmux capture-pane -t "$W" -p; tmux kill-session -t "$W" 2>/dev/null || true; rm -rf "$PASS" "$WALK" "$WHOME"; exit 1; }
walk_wait() { # pattern, seconds
  for _ in $(seq 1 $((${2:-10} * 10))); do tmux capture-pane -t "$W" -p | grep -q -- "$1" && return 0; sleep 0.1; done
  walk_fail "waited for '$1'"
}
walk_wait "optional" 60
tmux send-keys -t "$W" 2;                 walk_wait "managers" 5
tmux send-keys -t "$W" Tab Tab Tab Tab;   walk_wait "Assistant ·" 5
tmux send-keys -t "$W" Down Down Enter;   walk_wait "(•) ✓ CLI only" 5
tmux send-keys -t "$W" 3;                 walk_wait "Check command" 5
tmux send-keys -t "$W" Enter;             sleep 0.3
tmux send-keys -t "$W" -l "true";         tmux send-keys -t "$W" Enter
walk_wait "true" 5
tmux send-keys -t "$W" 4;                 walk_wait "Claude Code replied in" 20
walk_wait "Codex replied in" 5
tmux send-keys -t "$W" 5;                 walk_wait "Write to" 5
tmux send-keys -t "$W" Tab; sleep 0.3;    tmux send-keys -t "$W" Enter
for _ in $(seq 1 50); do [ -f "$WALK/igris.toml" ] && break; sleep 0.1; done
[ -f "$WALK/igris.toml" ] || walk_fail "Apply did not write igris.toml"
grep -q 'command = "true"' "$WALK/igris.toml" || walk_fail "igris.toml lacks the command"
tmux send-keys -t "$W" 6;                 walk_wait "Run the checks again" 10
tmux kill-session -t "$W"
ended=0
for _ in $(seq 1 30); do pgrep -f -- "--repo $WALK" >/dev/null || { ended=1; break; }; sleep 0.1; done
[ "$ended" = 1 ] || { pkill -f -- "--repo $WALK" || true; rm -rf "$PASS" "$WALK" "$WHOME"; echo "FAIL: igris kept running after the walk"; exit 1; }
rm -rf "$PASS" "$WALK" "$WHOME"
echo "ok  the whole walk: Tools, Pipeline, Verify (latency), Review unlocked and written, Test; no orphan"

# Motion: the wordmark sheen moves in a truecolor terminal and stands still with IGRIS_NO_MOTION=1.
distinct_frames() { # extra environment, for example IGRIS_NO_MOTION=1
  local sess="igris-e2e-m$$"
  tmux new-session -d -s "$sess" -x 100 -y 30 "env HOME=$HOME_DIR COLORTERM=truecolor $1 '$BIN' --repo '$REPO'; sleep 30"
  for _ in $(seq 1 600); do tmux capture-pane -t "$sess" -p | grep -q "optional" && break; sleep 0.1; done
  local n
  n=$(for _ in 1 2 3 4; do tmux capture-pane -t "$sess" -e -p | head -4 | md5sum; sleep 0.7; done | sort -u | wc -l)
  tmux kill-session -t "$sess" 2>/dev/null || true
  echo "$n"
}
moving=$(distinct_frames "")
still=$(distinct_frames "IGRIS_NO_MOTION=1")
[ "$moving" -gt 1 ] || { echo "FAIL: the sheen did not move ($moving distinct frames)"; exit 1; }
[ "$still" = 1 ] || { echo "FAIL: IGRIS_NO_MOTION=1 still moved ($still distinct frames)"; exit 1; }
echo "ok  the sheen moves, and IGRIS_NO_MOTION=1 stops it"
echo "tui-e2e: all checks passed"
