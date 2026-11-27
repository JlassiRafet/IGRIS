#!/usr/bin/env python3
"""Rebuild the README media in preview/ from the real thing.

    cargo build && scripts/preview.py [--bin target/debug/igris] [--scrub FILE] [--only NAME]

Needs tmux, chromium (or google-chrome) and Pillow. It runs `igris` in a detached tmux terminal against a
throwaway git repo in /tmp/igris-demo, captures the screens with their colours and renders them with the
font in site/src/assets/fonts:

    wordmark.png                                                 the IGRIS wordmark (README header)
    check.png, tools.png, pipeline.png, verify.png, review.png, test.png   one screenshot per screen
    igris.gif                                                    the app in use, recorded live
    how-it-runs.gif                                              the pipeline page, one task start to finish

Nothing outside /tmp/igris-demo, a temporary HOME and preview/ is written.

--scrub FILE  lines `old=>new` applied to the captured text (the replacement is padded or cut to the same
              width). Use it to hide facts about your own machine, such as the hardware on the Check screen.
--only NAME   build one of: wordmark, screens, igris, pipeline, chain (chain is never part of a full rebuild).
"""
import argparse, html, os, re, shutil, subprocess, sys, tempfile, threading, time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DEMO = Path("/tmp/igris-demo")
COLS, ROWS = 110, 32
BG, FG = "#0b1114", "#d7dbe0"
BASIC = ["#000000", "#cc3333", "#33aa55", "#ccaa33", "#4477dd", "#aa55cc", "#33aaaa", "#cccccc",
         "#666666", "#ff5555", "#55dd77", "#ffdd55", "#6699ff", "#cc77ee", "#55dddd", "#ffffff"]
CHROME = ["--headless", "--no-sandbox", "--disable-gpu", "--hide-scrollbars", "--allow-file-access-from-files"]


def sh(*args, **kw):
    return subprocess.run(args, check=True, text=True, capture_output=True, **kw).stdout


class Term:
    """igris in a detached tmux terminal."""

    def __init__(self, binary, home, repo, path, motion=False, extra_env=""):
        self.s = f"igris-preview-{os.getpid()}"
        env = f"env HOME={home} PATH={path} COLORTERM=truecolor" + ("" if motion else " IGRIS_NO_MOTION=1") + (f" {extra_env}" if extra_env else "")
        sh("tmux", "new-session", "-d", "-s", self.s, "-x", str(COLS), "-y", str(ROWS),
           f"{env} '{binary}' --repo '{repo}'; sleep 180")

    def text(self, ansi=False):
        return sh("tmux", "capture-pane", "-t", self.s, *(["-e"] if ansi else []), "-p")

    def wait(self, pattern, seconds=60):
        for _ in range(seconds * 10):
            if pattern in self.text():
                return
            time.sleep(0.1)
        sys.exit(f"timed out waiting for {pattern!r}:\n{self.text()}")

    def keys(self, *keys, literal=False, pause=0.3):
        sh("tmux", "send-keys", "-t", self.s, *(["-l"] if literal else []), *keys)
        time.sleep(pause)

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


def render(ansi, png, scrub, chrome, scale=2):
    """One captured screen -> a PNG: a rounded terminal window with a small `igris` title, like a terminal's."""
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
        sh(chrome, *CHROME, f"--user-data-dir={d}/profile", "--default-background-color=00000000",
           f"--force-device-scale-factor={scale}", "--window-size=1100,720", f"--screenshot={png}", src.as_uri())
    from PIL import Image
    im = Image.open(png).convert("RGBA")
    im.crop(im.getchannel("A").getbbox()).save(png, optimize=True)


def save_gif(frames, durations, gif):
    """Frames (PIL images) -> one GIF with a single shared palette, so colours stay steady and the file small."""
    from PIL import Image
    w, h = frames[0].size
    picks = [frames[i] for i in range(0, len(frames), max(1, len(frames) // 6))][:6]
    sheet = Image.new("RGB", (w, h * len(picks)))
    for k, f in enumerate(picks):
        sheet.paste(f, (0, k * h))
    palette = sheet.quantize(colors=255, method=Image.Quantize.MEDIANCUT, dither=Image.Dither.NONE)
    quant = [f.convert("RGB").quantize(palette=palette, dither=Image.Dither.NONE) for f in frames]
    quant[0].save(gif, save_all=True, append_images=quant[1:], duration=durations, loop=0, optimize=True, disposal=1)
    print("wrote", gif, f"{Path(gif).stat().st_size / 1e6:.2f} MB, {len(frames)} frames")


def demo_repo():
    """A tidy demo repo with one empty commit, and the agents found through a folder with a short path."""
    repo, shim = DEMO / "my-app", DEMO / "bin"
    shim.mkdir(parents=True, exist_ok=True)
    git = ["git", "-C", str(repo), "-c", "user.email=demo@example.com", "-c", "user.name=demo"]
    if not (repo / ".git").exists():
        repo.mkdir(parents=True, exist_ok=True)
        sh(*git, "init", "-q", "-b", "main")
        sh(*git, "commit", "-q", "--allow-empty", "-m", "init")
    sh(*git, "reset", "-q", "--hard", sh(*git, "rev-list", "--max-parents=0", "HEAD").strip())
    (repo / "igris.toml").unlink(missing_ok=True)
    # Each tool is found through a short wrapper. Verify's live check (`claude -p`, `codex exec`) answers at once
    # instead of spending a real model call; every other command goes to the real tool when there is one.
    quick = {"claude": "-p", "codex": "exec", "herdr": "--version"}
    for tool, arg in quick.items():
        (shim / tool).unlink(missing_ok=True)  # never write through an old symlink to the real tool
        if not (real := shutil.which(tool)):
            continue
        (shim / tool).write_text(f'#!/bin/sh\nif [ "$1" = {arg} ]; then echo {"herdr 0.9.1" if tool == "herdr" else "ok"}; exit 0; fi\n'
                                 f'exec {real} "$@"\n')
        (shim / tool).chmod(0o755)
    return repo, shim, git


def walk_through(t, git, repo, record=None):
    """The same session for the screenshots and the GIF: Scan, Tools (CLI only), Pipeline, Verify, Review, Test."""
    t.wait("optional"); time.sleep(1.2)
    if record:
        record("check")
    t.keys("2"); t.wait("managers"); time.sleep(0.8)
    t.keys("Tab", "Tab", "Tab", "Tab", pause=0.4); t.wait("Assistant ·")
    t.keys("Down", "Down", pause=0.5); t.keys("Enter"); t.wait("CLI only"); time.sleep(0.8)
    if record:
        record("tools")
    t.keys("3"); t.wait("Check command"); time.sleep(0.8)
    t.keys("Enter"); t.wait("e.g. cargo test"); time.sleep(0.6)
    for ch in "cargo test":
        t.keys(ch, literal=True, pause=0.09)
    time.sleep(0.5)
    if record:
        record("pipeline")
    t.keys("Enter", pause=1.0)
    t.keys("4"); t.wait("replied in", 30); time.sleep(1.5)
    if record:
        record("verify")
    t.keys("5"); t.wait("Write to"); time.sleep(1.8)
    if record:
        record("review")
    t.keys("Tab", pause=0.6); t.keys("Enter", pause=1.8)
    sh(*git, "add", "igris.toml"); sh(*git, "commit", "-q", "-m", "Add igris.toml")
    t.keys("6"); t.wait("Run the checks again"); time.sleep(1.0); t.keys("Enter", pause=0.2)
    t.wait("All checks pass", 20); time.sleep(2.2)
    if record:
        record("test")


def wordmark(out, chrome):
    """The block-pixel wordmark with its plume, on a transparent background: the README's logo."""
    rows = ["▀█▀ █▀▀ █▀█ ▀█▀ █▀▀", "▄█▄ █▄█ █▀▄ ▄█▄ ▄▄█"]
    glow, acc = (0xbc, 0xfa, 0xf1), (0x08, 0x79, 0xf3)
    cells = []
    for r in range(4):
        for c in range(19):
            on = rows[r // 2][c] in ("▀█" if r % 2 == 0 else "▄█")
            col = "rgb(%d,%d,%d)" % tuple(round(g + (a - g) * ((c / 18 + r / 3) / 2)) for g, a in zip(glow, acc))
            cells.append(f'<i style="background:{col if on else "none"}"></i>')
    page = f"""<!doctype html><meta charset=utf-8><style>
html,body{{margin:0;background:transparent}}
#w{{--px:24px;position:relative;display:inline-grid;grid-template-columns:repeat(19,var(--px));grid-auto-rows:var(--px);margin:60px 30px 30px}}
#w i{{display:block;transform:scale(1.04)}}
#w b{{position:absolute;left:calc(5.5*var(--px));top:calc(-2.6*var(--px));transform:translateX(-50%);color:#ff3038;font:700 calc(2*var(--px))/1 'DejaVu Sans Mono',monospace}}
</style><div id=w>{''.join(cells)}<b>ʅ</b></div>"""
    png = out / "wordmark.png"
    with tempfile.TemporaryDirectory() as d:
        src = Path(d) / "page.html"
        src.write_text(page, encoding="utf-8")
        sh(chrome, *CHROME, f"--user-data-dir={d}/profile", "--default-background-color=00000000",
           "--force-device-scale-factor=2", "--window-size=700,330", f"--screenshot={png}", src.as_uri())
    from PIL import Image
    im = Image.open(png).convert("RGBA")
    pad = 20
    l, t, r, b = im.getchannel("A").getbbox()
    im.crop((max(0, l - pad), max(0, t - pad), min(im.width, r + pad), min(im.height, b + pad))).save(png, optimize=True)
    print("wrote", png)


def screens(binary, out, scrub, chrome):
    repo, shim, git = demo_repo()
    home = tempfile.mkdtemp()
    t = Term(binary, home, repo, f"{shim}:{os.environ['PATH']}")
    shots = {}
    try:
        walk_through(t, git, repo, lambda name: shots.__setitem__(name, t.text(True)))
    finally:
        t.kill(); shutil.rmtree(home, ignore_errors=True)
    for name, ansi in shots.items():
        render(ansi, out / f"{name}.png", scrub, chrome)
        print("wrote", out / f"{name}.png")


def login_shim():
    """Fake `claude` and `codex` that answer "logged in", so the chain shows its best case."""
    shim = DEMO / "bin-logged-in"
    shim.mkdir(parents=True, exist_ok=True)
    (shim / "claude").write_text('#!/bin/sh\necho \'{"loggedIn":true,"authMethod":"claude.ai"}\'\n')
    (shim / "codex").write_text('#!/bin/sh\necho "Logged in using ChatGPT" >&2\n')
    for tool in ("claude", "codex"):
        (shim / tool).chmod(0o755)
    return shim


def chain_screens(binary, out, scrub, chrome):
    """The guided chain (IGRIS_CHAIN=1, slice 1): the Builder, Reviewer and end screens in the usual
    terminal, on a machine where the agents are logged in, in 16 colours and in an ASCII terminal."""
    repo, shim, git = demo_repo()
    runs = [("", "", shim), ("-logged-in", "", login_shim()), ("-16-colour", "COLORTERM=", shim), ("-ascii", "LC_ALL=C", shim)]
    shots = {}
    for name, env, bin_dir in runs:
        home = tempfile.mkdtemp()
        extra = f"IGRIS_CHAIN=1 {env}".strip()
        t = Term(binary, home, repo, f"{bin_dir}:{shim}:{os.environ['PATH']}", extra_env=extra)
        try:
            t.wait("Builder: which agent"); time.sleep(0.8)
            shots[f"chain-builder{name}"] = t.text(True)
            t.keys("Enter"); t.wait("Reviewer: who checks"); time.sleep(0.8)
            shots[f"chain-reviewer{name}"] = t.text(True)
            if not name:
                t.keys("Enter"); t.wait("Slice 1 ends here"); time.sleep(0.8)
                shots["chain-end"] = t.text(True)
        finally:
            t.kill(); shutil.rmtree(home, ignore_errors=True)
    for name, ansi in shots.items():
        render(ansi, out / f"{name}.png", scrub, chrome)
        print("wrote", out / f"{name}.png")


def igris_gif(binary, out, scrub, chrome):
    """Record the real session with the wordmark's sheen moving, then render each distinct frame."""
    repo, shim, git = demo_repo()
    home = tempfile.mkdtemp()
    t = Term(binary, home, repo, f"{shim}:{os.environ['PATH']}", motion=True)
    frames, stop = [], threading.Event()

    def sample():
        t0 = time.time()
        while not stop.is_set():
            frames.append((time.time() - t0, t.text(True)))
            time.sleep(0.1)

    thread = threading.Thread(target=sample, daemon=True)
    thread.start()
    try:
        walk_through(t, git, repo)
    finally:
        stop.set(); thread.join(); t.kill(); shutil.rmtree(home, ignore_errors=True)
    # Merge frames that look the same; each distinct frame lasts until the next one appears.
    uniq = []
    for at, ansi in frames:
        if not uniq or uniq[-1][1] != ansi:
            uniq.append((at, ansi))
    end = frames[-1][0]
    durations = [max(40, min(2500, round((b - a) * 1000 / 10) * 10)) for (a, _), (b, _) in zip(uniq, uniq[1:] + [(end, "")])]
    durations[-1] = 3000
    work = Path(tempfile.mkdtemp())
    pngs = [work / f"f{i:04d}.png" for i in range(len(uniq))]
    with ThreadPoolExecutor(4) as pool:
        list(pool.map(lambda i: render(uniq[i][1], pngs[i], scrub, chrome, scale=1), range(len(uniq))))
    from PIL import Image
    imgs = [Image.open(p).convert("RGB") for p in pngs]
    w, h = max(i.width for i in imgs), max(i.height for i in imgs)
    imgs = [i if i.size == (w, h) else i.resize((w, h)) for i in imgs]
    save_gif(imgs, durations, out / "igris.gif")
    shutil.rmtree(work, ignore_errors=True)


def pipeline_gif(out, chrome, task=1, frame_ms=100):
    """Record the pipeline page. Its clock is virtual: ?at=MS draws exactly that moment, so every frame is exact."""
    base = (ROOT / "site/pipeline-src/index.html").as_uri() + f"?task={task}&at="
    args = [chrome, *CHROME]
    dom = sh(*args, "--virtual-time-budget=5000", "--dump-dom", base + "99999999")
    end = int(re.search(r'data-end="(\d+)"', dom).group(1))
    times = list(range(0, end + frame_ms, frame_ms))
    work = Path(tempfile.mkdtemp())

    def shoot(i):
        png = work / f"f{i:04d}.png"
        with tempfile.TemporaryDirectory() as profile:
            sh(*args, f"--user-data-dir={profile}", "--window-size=848,1130", f"--screenshot={png}", base + str(times[i]))
        return png

    with ThreadPoolExecutor(4) as pool:
        pngs = list(pool.map(shoot, range(len(times))))
    from PIL import Image, ImageChops
    imgs = [Image.open(p).convert("RGB") for p in pngs]
    bg = Image.new("RGB", imgs[0].size, imgs[0].getpixel((2, 2)))
    boxes = [b for b in (ImageChops.difference(i, bg).getbbox() for i in imgs) if b]
    pad = 16
    box = (max(0, min(b[0] for b in boxes) - pad), max(0, min(b[1] for b in boxes) - pad),
           min(imgs[0].width, max(b[2] for b in boxes) + pad), min(imgs[0].height, max(b[3] for b in boxes) + pad))
    imgs = [i.crop(box) for i in imgs]
    save_gif(imgs, [frame_ms] * (len(imgs) - 1) + [3000], out / "how-it-runs.gif")
    shutil.rmtree(work, ignore_errors=True)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--bin", default=str(ROOT / "target/debug/igris"))
    ap.add_argument("--scrub")
    ap.add_argument("--out", default=str(ROOT / "preview"))
    ap.add_argument("--only", choices=["wordmark", "screens", "igris", "pipeline", "chain"])
    a = ap.parse_args()
    chrome = shutil.which("chromium") or shutil.which("google-chrome") or sys.exit("chromium not found")
    scrub = []
    if a.scrub:
        scrub = [tuple(l.split("=>", 1)) for l in Path(a.scrub).read_text().splitlines() if "=>" in l]
    out = Path(a.out)
    out.mkdir(exist_ok=True)
    if a.only in (None, "wordmark"):
        wordmark(out, chrome)
    if a.only in (None, "screens"):
        screens(a.bin, out, scrub, chrome)
    if a.only in (None, "igris"):
        igris_gif(a.bin, out, scrub, chrome)
    if a.only in (None, "pipeline"):
        pipeline_gif(out, chrome)
    if a.only == "chain":
        chain_screens(a.bin, out, scrub, chrome)


if __name__ == "__main__":
    main()
