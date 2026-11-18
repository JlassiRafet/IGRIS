#!/bin/sh
# End-to-end check with REAL agents on a throwaway repo. It runs the builder and the reviewer from
# your igris.toml, so it uses your own subscriptions or keys (a small task, but not free).
#
# Before the first run: Claude Code asks "trust this folder?" for every new folder, and a task runs
# in a fresh worktree under ~/.herdr/worktrees. Start `claude` once in that folder and trust it, and
# do the same for `codex` if it is your reviewer. If a run still stops with status "blocked", answer
# the prompt in Herdr and run `igris continue <id>`.
#
# Usage: scripts/smoke.sh      (needs `igris` on PATH, or IGRIS=/path/to/igris)
set -eu
IGRIS=${IGRIS:-igris}
dir=${TMPDIR:-/tmp}/igris-smoke
rm -rf "$dir"
mkdir -p "$dir"
cd "$dir"

git init -q -b main
cat > calc.py <<'PY'
# add(a, b) is missing on purpose.
PY
cat > test_calc.py <<'PY'
import unittest
from calc import add


class AddTest(unittest.TestCase):
    def test_add(self):
        self.assertEqual(add(2, 3), 5)

    def test_negative(self):
        self.assertEqual(add(-1, 1), 0)


if __name__ == "__main__":
    unittest.main()
PY
printf '[checks]\ncommand = "python3 -m unittest"\n' > igris.toml
git add -A
git -c user.name=smoke -c user.email=smoke@example.com commit -q -m "init"

echo "== igris doctor"
"$IGRIS" doctor --repo .
echo "== igris run"
"$IGRIS" run --repo . --task "implement add(a, b) in calc.py so the tests pass"
