#!/usr/bin/env bash
# End-to-end check of the terminal app in a real (detached) tmux terminal: keys, mouse, help, resize,
# quit. No agent is started and nothing outside a temporary folder is written.
# Usage: cargo build && scripts/tui-e2e.sh      (IGRIS=/path/to/igris to test another binary)
set -euo pipefail
BIN=${IGRIS:-target/debug/igris}
[ -x "$BIN" ] || { echo "no binary at $BIN: run cargo build first"; exit 2; }
BIN=$(cd "$(dirname "$BIN")" && pwd)/$(basename "$BIN")
S=igris-e2e-$$
REPO=$(mktemp -d)
HOME_DIR=$(mktemp -d)
trap 'tmux kill-session -t "$S" 2>/dev/null || true; rm -rf "$REPO" "$HOME_DIR"' EXIT
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
  "env HOME=$HOME_DIR COLORTERM=truecolor '$BIN' --repo '$REPO'; echo EXIT=\$?; sleep 60"

wait_for "▀█▀ █▀▀ █▀█" 10   # the wordmark
wait_for "Pipeline" 5       # the tab strip
wait_for "optional" 60          # the scan finished: the count chips are on screen
echo "ok  wordmark, tabs, scan"

tmux send-keys -t "$S" 2;   wait_for "Builder agent" 5
echo "ok  2 opens the Pipeline form"
tmux send-keys -t "$S" '?'; wait_for "How setup works" 5
tmux send-keys -t "$S" Escape; gone "How setup works" 5
echo "ok  ? opens help, Esc closes it"
tmux send-keys -t "$S" ']'; wait_for "Write to" 5
tmux send-keys -t "$S" '[';  wait_for "Builder agent" 5
echo "ok  ] and [ change screen"

# Mouse. At 100x30 the steps are a rail: rows 6 to 9 (the wordmark band takes rows 1 to 4, then a rule).
click 10 8; wait_for "Write to" 5              # Review
click 10 9; wait_for "Run the checks again" 5   # Test
click 10 6; wait_for "optional" 5                   # Check
echo "ok  click on a tab switches screen"
tmux send-keys -t "$S" 2; wait_for "Builder agent" 5
wheel_down 20 12; wait_for "▌ Reviewer agent" 5
echo "ok  the wheel moves the focus"

tmux resize-window -t "$S" -x 70 -y 20; wait_for "at least 80x24" 5
tmux resize-window -t "$S" -x 100 -y 30; wait_for "Builder agent" 5
echo "ok  too small a window says so, and recovers"

tmux send-keys -t "$S" q;   wait_for "EXIT=0" 5
echo "ok  q exits with code 0"
echo "tui-e2e: all checks passed"
