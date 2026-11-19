#!/usr/bin/env python3
"""Rebuild the IGRIS logo set from the original artwork (source/IGRIS-logo-original.png).

Steps: split the raster into helmet, plume shade and plume layers; clean speckles; upscale and
smooth each layer; trace each with Inkscape's Trace Bitmap (the `object-trace` action, the same
engine as Path > Trace Bitmap); then assemble every variant and export the PNGs and the favicon.

Needs: python3 with Pillow and numpy, Inkscape 1.2+ and ImageMagick (`magick`) on PATH.
Run:   python3 assets/logo/tools/build.py          (rewrites svg/, png/ and favicon/)
"""
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

import numpy as np
from PIL import Image, ImageFilter

ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "source" / "IGRIS-logo-original.png"
SCALE = 3  # trace at 3x so the contours come out smooth
BLUR = 2.8  # blur radius (px at 3x) before thresholding: evens out the source's pixel steps
OPTIMIZE = 1.0  # potrace curve-optimisation tolerance: higher merges more segments

# Brand palette. The helmet changes with the background; the plume reds are matched to it.
DARK_BG = dict(helmet="#c9ced3", shade="#c9001a", plume="#ff3038")
LIGHT_BG = dict(helmet="#2b2f33", shade="#a90016", plume="#d00018")
WHITE = "#f0f2f3"
INK = "#252a30"
PLATE = "#0b1114"
OUTLINE = "#1b2026"


# ---------- raster work ----------

def label(mask):
    """4-connected components of a boolean array, as (flat indices, touches the border)."""
    h, w = mask.shape
    fl = mask.astype(np.uint8).tobytes()
    seen = bytearray(len(fl))
    out = []
    for start in np.flatnonzero(mask.reshape(-1)).tolist():
        if seen[start]:
            continue
        seen[start] = 1
        stack, comp, border = [start], [], False
        while stack:
            i = stack.pop()
            comp.append(i)
            y, x = divmod(i, w)
            if x == 0 or y == 0 or x == w - 1 or y == h - 1:
                border = True
            for j, ok in ((i - 1, x > 0), (i + 1, x < w - 1), (i - w, y > 0), (i + w, y < h - 1)):
                if ok and fl[j] and not seen[j]:
                    seen[j] = 1
                    stack.append(j)
        out.append((comp, border))
    return out


def to_img(mask):
    return Image.fromarray((mask * 255).astype(np.uint8))


def morph(mask, op, r):
    """Binary erode ('min') or dilate ('max') with a (2r+1) square."""
    f = ImageFilter.MinFilter if op == "min" else ImageFilter.MaxFilter
    return np.array(to_img(mask).filter(f(2 * r + 1))) > 127


def clean(mask, open_r, min_px, min_hole):
    """Drop thin slivers and specks, and fill tiny pinholes."""
    m = mask.copy()
    if open_r:
        m = morph(morph(m, "min", open_r), "max", open_r)
    flat = m.reshape(-1)
    for comp, _ in label(m):
        if len(comp) < min_px:
            flat[comp] = False
    for comp, border in label(~m):
        if not border and len(comp) < min_hole:
            flat[comp] = True
    return m


def split_layers(rgba):
    """Helmet, shade and plume masks from the original, plus the overall bounding box."""
    opaque = rgba[..., 3] >= 128
    red = rgba[..., 0].astype(int)
    helmet = clean(opaque & (red < 64), 0, 60, 40)
    shade = clean(opaque & (red >= 64) & (red < 154), 2, 200, 60)
    # Everything opaque that is not helmet is plume. It also runs under the helmet's edge, so the
    # helmet drawn on top leaves no hairline gap between the two.
    plume = opaque & ~helmet
    plume = clean(plume, 1, 200, 60)
    strip = morph(plume, "max", 4) & morph(helmet, "max", 4)
    plume = plume | strip
    shade = shade & plume
    ys, xs = np.where(helmet | plume)
    return helmet, shade, plume, (xs.min(), ys.min(), xs.max() + 1, ys.max() + 1)


def colours(rgba, layers):
    """Median colour of each layer in the original, as hex (for the faithful 'original' variant)."""
    out = {}
    for name, m in zip(("helmet", "shade", "plume"), layers):
        core = morph(m, "min", 3) & (rgba[..., 3] >= 250)
        px = rgba[core][:, :3]
        out[name] = "#%02x%02x%02x" % tuple(int(v) for v in np.median(px, axis=0))
    return out


# ---------- tracing with Inkscape ----------

def trace(mask, tmp, name):
    """Upscale, smooth, then trace one layer with Inkscape. Returns (path d, transform or '')."""
    h, w = mask.shape
    big = to_img(mask).resize((w * SCALE, h * SCALE), Image.BICUBIC).filter(ImageFilter.GaussianBlur(BLUR))
    bw = Image.fromarray(np.where(np.array(big) >= 128, 0, 255).astype(np.uint8))  # layer = black
    png, svg, out = tmp / f"{name}.png", tmp / f"{name}.svg", tmp / f"{name}.traced.svg"
    bw.save(png)
    svg.write_text(
        f'<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" '
        f'width="{w}" height="{h}" viewBox="0 0 {w} {h}"><image x="0" y="0" width="{w}" height="{h}" '
        f'xlink:href="{png.name}"/></svg>'
    )
    # scans=2 (black and white), no pre-blur, remove background, speckles, corner and optimize
    actions = (
        f"select-all;object-trace:2,false,false,true,8,1.0,{OPTIMIZE};"
        f"export-type:svg;export-plain-svg;export-filename:{out};export-do"
    )
    subprocess.run(["inkscape", str(svg), "--batch-process", f"--actions={actions}"],
                   check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    # Read Inkscape's output as text (no XML parser needed): take every path's `d`, and refuse
    # anything unexpected, such as a transformed group, instead of guessing.
    text = out.read_text()
    tags = re.findall(r"<path\b[^>]*>", text, re.S)
    if not tags:
        sys.exit(f"trace of {name} produced no path")
    if re.search(r"<g\b[^>]*\btransform=", text):
        sys.exit(f"trace of {name}: unexpected group transform, extend build.py before using it")
    ds = [re.search(r'\bd="([^"]*)"', t).group(1) for t in tags]
    tf = re.search(r'\btransform="([^"]*)"', tags[0])
    if any(re.search(r"\btransform=", t) for t in tags[1:]):
        sys.exit(f"trace of {name}: several transformed paths, extend build.py before using it")
    # Two decimals are far finer than a pixel at any size we export, and keep the files small.
    short = lambda m: f"{float(m.group()):.2f}".rstrip("0").rstrip(".")
    return re.sub(r"-?\d+\.\d+", short, " ".join(ds)), tf.group(1) if tf else ""


# ---------- assembling ----------

def doc(size, body, title, bg=None):
    plate = f'<rect width="{size}" height="{size}" fill="{bg}"/>' if bg else ""
    return (f'<svg xmlns="http://www.w3.org/2000/svg" width="{size}" height="{size}" '
            f'viewBox="0 0 {size} {size}"><title>{title}</title>{plate}{body}</svg>\n')


def mark(g, pal, outline=None):
    """The three layers, bottom to top: plume, shade, helmet. `outline` adds a dark rim behind."""
    tf = lambda t: f' transform="{t}"' if t else ""
    layers = [("plume", pal["plume"]), ("shade", pal["shade"]), ("helmet", pal["helmet"])]
    # The rim is a stroke painted behind each fill (paint-order), so each path appears only once.
    rim = (f' stroke="{outline[0]}" stroke-width="{outline[1]}" stroke-linejoin="round" paint-order="stroke"'
           if outline else "")
    body = "".join(
        f'<path d="{g["paths"][n][0]}"{tf(g["paths"][n][1])} fill="{c}" fill-rule="evenodd"'
        f'{rim if n != "shade" else ""}/>'
        for n, c in layers)
    return f'<g transform="{g["fit"]}">{body}</g>'


def fit(bbox, size, frac):
    """Transform that centres the art in a square canvas, art height = frac of the canvas."""
    x0, y0, x1, y1 = bbox
    s = frac * size / (y1 - y0)
    tx, ty = size / 2 - s * (x0 + x1) / 2, size / 2 - s * (y0 + y1) / 2
    return f"translate({tx:.3f} {ty:.3f}) scale({s:.6f})"


def render(svg, png, px):
    subprocess.run(["inkscape", str(svg), "-o", str(png), "-w", str(px), "-h", str(px)],
                   check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def main():
    rgba = np.array(Image.open(SRC).convert("RGBA"))
    helmet, shade, plume, bbox = split_layers(rgba)
    original = colours(rgba, (helmet, shade, plume))
    print("original colours:", original)

    with tempfile.TemporaryDirectory() as t:
        tmp = Path(t)
        paths = {n: trace(m, tmp, n) for n, m in (("helmet", helmet), ("shade", shade), ("plume", plume))}

        out = {d: ROOT / d for d in ("svg", "png", "favicon")}
        for d in out.values():
            shutil.rmtree(d, ignore_errors=True)
            d.mkdir()

        g = dict(paths=paths, fit=fit(bbox, 1024, 0.82))
        one = lambda c: dict(helmet=c, shade=c, plume=c)
        variants = {
            "IGRIS_original": (original, "IGRIS (original colours)"),
            "IGRIS_full_colour_dark_background": (DARK_BG, "IGRIS (for dark backgrounds)"),
            "IGRIS_full_colour_light_background": (LIGHT_BG, "IGRIS (for light backgrounds)"),
            "IGRIS_one_colour_white": (one(WHITE), "IGRIS (white)"),
            "IGRIS_one_colour_dark": (one(INK), "IGRIS (dark)"),
        }
        for name, (pal, title) in variants.items():
            (out["svg"] / f"{name}.svg").write_text(doc(1024, mark(g, pal), title))
        shutil.copy(out["svg"] / "IGRIS_full_colour_dark_background.svg", out["svg"] / "IGRIS_master.svg")

        # Avatar: own dark plate, art at 82% of its normal size so a round crop keeps the plume tip.
        inner = f'<g transform="translate(512 512) scale(0.82) translate(-512 -512)">{mark(g, DARK_BG)}</g>'
        (out["svg"] / "IGRIS_square_avatar.svg").write_text(doc(1024, inner, "IGRIS avatar", bg=PLATE))
        variants["IGRIS_square_avatar"] = (DARK_BG, "")

        # Favicon: tighter margins and a thin dark rim so it reads on any tab colour.
        gf = dict(paths=paths, fit=fit(bbox, 256, 0.90))
        (out["svg"] / "IGRIS_favicon.svg").write_text(
            doc(256, mark(gf, DARK_BG, outline=(OUTLINE, 34)), "IGRIS favicon"))

        for name in variants:
            sizes = (512, 1024, 2048) if "full_colour" in name else (512, 1024) if "avatar" in name else (1024,)
            for px in sizes:
                render(out["svg"] / f"{name}.svg", out["png"] / f"{name}_{px}.png", px)
        frames = []
        for px in (16, 24, 32, 48, 64):
            p = tmp / f"fav_{px}.png"
            render(out["svg"] / "IGRIS_favicon.svg", p, px)
            frames.append(str(p))
        subprocess.run(["magick", *frames, str(out["favicon"] / "IGRIS_favicon.ico")], check=True)
        for px in (32, 180, 512):
            render(out["svg"] / "IGRIS_favicon.svg", out["favicon"] / f"IGRIS_favicon_{px}.png", px)
        shutil.copy(out["svg"] / "IGRIS_favicon.svg", out["favicon"] / "IGRIS_favicon.svg")
    print("done")


if __name__ == "__main__":
    main()
