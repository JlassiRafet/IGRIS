# IGRIS logo

A knight's helm with a red plume. Everything here is built from `source/IGRIS-logo-original.png`.

| Folder | What |
|---|---|
| `source/` | The original artwork (raster). The only hand-made file. |
| `svg/` | Vector files. `IGRIS_master.svg` is the dark-background full-colour mark. |
| `png/` | Exports: full colour 512/1024/2048, original and one-colour 1024, avatar 512/1024. |
| `favicon/` | `IGRIS_favicon.svg` (source), `.ico` (16, 24, 32, 48, 64) and PNGs at 32, 180, 512. |
| `tools/build.py` | Rebuilds `svg/`, `png/` and `favicon/` from `source/`. |

## Which file where
- `IGRIS_original`: the supplied colours (black helmet, two reds). For light backgrounds.
- `IGRIS_full_colour_dark_background`: light-grey helmet, bright red plume.
- `IGRIS_full_colour_light_background`: dark-grey helmet, deep red plume.
- `IGRIS_one_colour_white` / `IGRIS_one_colour_dark`: single colour, for dark / light backgrounds.
- `IGRIS_square_avatar`: dark plate, art at 82% so a round crop keeps the plume tip.
- `IGRIS_favicon`: tighter margins and a thin dark rim so it reads on any tab colour.

## Palette
Helmet `#C9CED3` (on dark) or `#2B2F33` (on light). Plume `#FF3038` / `#D00018`, shade `#C9001A` / `#A90016`. Plate `#0B1114`. One colour `#F0F2F3` / `#252A30`.

## Rebuild
Needs Python 3 with Pillow and numpy, Inkscape 1.2+ and ImageMagick.

    python3 assets/logo/tools/build.py

The script splits the raster into plume, shade and helmet, removes speckles, upscales and smooths
each layer, and traces it with Inkscape's Trace Bitmap (`object-trace`). The plume runs under the
helmet's edge, so no hairline gap shows between them.
