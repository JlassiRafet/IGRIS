#!/usr/bin/env python3
"""Rebuild the README screenshots in preview/ by running the real terminal app.

    cargo build && scripts/preview.py [--bin target/debug/igris] [--scrub FILE]

Needs tmux, chromium (or google-chrome) and Pillow. It drives `igris` in a detached 100x30 tmux
terminal against a throwaway git repo in /tmp/igris-demo, captures each screen with its colours,
and renders it to a PNG with the font in site/src/assets/fonts. Nothing outside /tmp/igris-demo, a
temporary HOME and preview/ is written.

--scrub FILE: lines `old=>new` applied to the captured text (the replacement is padded or cut to the
same width). Use it to hide facts about your own machine, such as the hardware shown on the Check screen.
"""
import argparse, html, os, re, shutil, subprocess, sys, tempfile, time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DEMO = Path("/tmp/igris-demo")
COLS, ROWS = 100, 30
BG, FG = "#0b1114", "#d7dbe0"
BASIC = ["#000000", "#cc3333", "#33aa55", "#ccaa33", "#4477dd", "#aa55cc", "#33aaaa", "#cccccc",
         "#666666", "#ff5555", "#55dd77", "#ffdd55", "#6699ff", "#cc77ee", "#55dddd", "#ffffff"]


def sh(*args, **kw):
    return subprocess.run(args, check=True, text=True, capture_output=True, **kw).stdout


class Term:
    def __init__(self, binary, home, repo, path):
        self.s = f"igris-preview-{os.getpid()}"
        env = f"env HOME={home} PATH={path} COLORTERM=truecolor IGRIS_NO_MOTION=1"
        sh("tmux", "new-session", "-d", "-s", self.s, "-x", str(COLS), "-y", str(ROWS),
           f"{env} '{binary}' --repo '{repo}'; sleep 120")

    def text(self, ansi=False):
        return sh("tmux", "capture-pane", "-t", self.s, *(["-e"] if ansi else []), "-p")

    def wait(self, pattern, seconds=60):
        for _ in range(seconds * 10):
            if pattern in self.text():
                return
            time.sleep(0.1)
        sys.exit(f"timed out waiting for {pattern!r}:\n{self.text()}")

    def keys(self, *keys, literal=False):
        sh("tmux", "send-keys", "-t", self.s, *(["-l"] if literal else []), *keys)
        time.sleep(0.3)

    def kill(self):
        subprocess.run(["tmux", "kill-session", "-t", self.s], capture_output=True)


def colour256(n):
    if n < 16:
        return BASIC[n]
    if n < 232:
        n -= 16
        v = [0, 95, 135, 175, 215, 255]
        return "#%02x%02x%02x" % (v[n // 36], v[n // 6 % 6], v[n % 6])
    g = 8 + (n - 232) * 10
    return "#%02x%02x%02x" % (g, g, g)


def line_html(line):
    fg = bg = None
    bold = dim = rev = False
    out, pos = [], 0

    def emit(text):
        if not text:
            return
        f, b = (fg or FG), bg
        if rev:
            f, b = (bg or BG), (fg or FG)
        style = f"color:{f};" + (f"background:{b};" if b else "")
        style += "font-weight:700;" * bold + "opacity:.6;" * dim
        out.append(f'<span style="{style}">{html.escape(text)}</span>')

    for m in re.finditer(r"\x1b\[([0-9;:]*)m", line):
        emit(line[pos:m.start()])
        pos = m.end()
        p = [int(x) if x else 0 for x in m.group(1).replace(":", ";").split(";")]
        i = 0
        while i < len(p):
            c = p[i]
            if c == 0: fg = bg = None; bold = dim = rev = False
            elif c == 1: bold = True
            elif c == 2: dim = True
            elif c == 7: rev = True
            elif c == 22: bold = dim = False
            elif c == 27: rev = False
            elif 30 <= c <= 37: fg = BASIC[c - 30]
            elif 90 <= c <= 97: fg = BASIC[c - 90 + 8]
            elif 40 <= c <= 47: bg = BASIC[c - 40]
            elif 100 <= c <= 107: bg = BASIC[c - 100 + 8]
            elif c == 39: fg = None
            elif c == 49: bg = None
            elif c in (38, 48):
                col = None
                if i + 4 < len(p) and p[i + 1] == 2:
                    col = "#%02x%02x%02x" % tuple(p[i + 2:i + 5]); i += 4
                elif i + 2 < len(p) and p[i + 1] == 5:
                    col = colour256(p[i + 2]); i += 2
                if c == 38: fg = col
                else: bg = col
            i += 1
    emit(line[pos:])
    return "".join(out) or " "


def render(ansi, png, scrub, chrome):
    for old, new in scrub:
        ansi = ansi.replace(old, new.ljust(len(old))[:len(old)])
    lines = [line_html(l) for l in ansi.rstrip("\n").split("\n")]
    lines += [" "] * (ROWS - len(lines))
    font = (ROOT / "site/src/assets/fonts/JetBrainsMono-Regular.woff2").as_uri()
    page = f"""<!doctype html><meta charset=utf-8><style>
@font-face{{font-family:JB;src:url({font})}}
html,body{{margin:0;background:transparent}}
#w{{display:inline-block;background:{BG};border:1px solid #2a3036;border-radius:14px;padding:0 0 18px}}
#t{{text-align:center;color:#7d858c;font:13px/38px JB,monospace}}
pre{{width:{COLS}ch;margin:0 22px;font:15px/1.28 JB,'DejaVu Sans Mono',monospace;color:{FG}}}
</style><div id=w><div id=t>igris</div><pre>{chr(10).join(lines)}</pre></div>"""
    with tempfile.TemporaryDirectory() as d:
        src = Path(d) / "page.html"
        src.write_text(page, encoding="utf-8")
        sh(chrome, "--headless", "--no-sandbox", "--disable-gpu", "--allow-file-access-from-files",
           "--hide-scrollbars", "--default-background-color=00000000", "--force-device-scale-factor=2",
           "--window-size=960,700", f"--screenshot={png}", src.as_uri())
    from PIL import Image
    im = Image.open(png).convert("RGBA")
    im.crop(im.getchannel("A").getbbox()).save(png, optimize=True)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--bin", default=str(ROOT / "target/debug/igris"))
    ap.add_argument("--scrub")
    ap.add_argument("--out", default=str(ROOT / "preview"))
    a = ap.parse_args()
    chrome = shutil.which("chromium") or shutil.which("google-chrome") or sys.exit("chromium not found")
    scrub = []
    if a.scrub:
        scrub = [tuple(l.split("=>", 1)) for l in Path(a.scrub).read_text().splitlines() if "=>" in l]
    out = Path(a.out)
    out.mkdir(exist_ok=True)

    # A tidy demo repo with one empty commit, and the agents found through a folder with a short path.
    repo, shim = DEMO / "my-app", DEMO / "bin"
    shim.mkdir(parents=True, exist_ok=True)
    git = ["git", "-C", str(repo), "-c", "user.email=demo@example.com", "-c", "user.name=demo"]
    if not (repo / ".git").exists():
        repo.mkdir(parents=True, exist_ok=True)
        sh(*git, "init", "-q", "-b", "main")
        sh(*git, "commit", "-q", "--allow-empty", "-m", "init")
    root = sh(*git, "rev-list", "--max-parents=0", "HEAD").strip()
    sh(*git, "reset", "-q", "--hard", root)
    (repo / "igris.toml").unlink(missing_ok=True)
    for tool in ("claude", "codex"):
        if found := shutil.which(tool):
            (shim / tool).unlink(missing_ok=True)
            (shim / tool).symlink_to(found)

    shots = {}
    home = tempfile.mkdtemp()
    t = Term(a.bin, home, repo, f"{shim}:{os.environ['PATH']}")
    try:
        t.wait("ʅ", 10); shots["splash"] = t.text(True)
        t.wait("optional"); time.sleep(0.5); shots["check"] = t.text(True)
        t.keys("2"); t.wait("Builder agent"); t.keys("Down", "Down", "Enter"); t.wait("e.g. cargo test")
        t.keys("cargo test", literal=True); shots["pipeline"] = t.text(True)
        t.keys("Enter"); t.keys("3"); t.wait("Write to"); time.sleep(0.5); shots["review"] = t.text(True)
        t.keys("Tab", "Enter"); time.sleep(1.5)
        sh(*git, "add", "igris.toml"); sh(*git, "commit", "-q", "-m", "Add igris.toml")
        t.keys("4"); t.wait("Run the checks again"); t.keys("Enter"); t.wait("All checks pass", 20)
        shots["test"] = t.text(True)
    finally:
        t.kill()
        shutil.rmtree(home, ignore_errors=True)
    for name, ansi in shots.items():
        render(ansi, out / f"{name}.png", scrub, chrome)
        print("wrote", out / f"{name}.png")


if __name__ == "__main__":
    main()
