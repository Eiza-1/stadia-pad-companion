"""Builds the pad photos the app uses (app/ui/assets/pads/*.png) from the originals in art/pads/.

Each original is a product photo on a white background. For each one:
  1. the background is found by flood-filling from the image edges (so white highlights *inside* the
     pad, e.g. on Clearly White, are kept), then the edge is softened by 1 px for clean anti-aliasing;
  2. the image is cropped to the pad and scaled to a common width, then moved and uniformly scaled onto
     the reference photo's button layout (never warped), so colour wipes line up;
  3. each colour's measured button centres (POINTS below), carried through that move, are written to
     app/ui/js/pad-points.js.

Run: python scripts/make_pad_images.py   (needs Pillow)
"""
from pathlib import Path
from PIL import Image, ImageDraw, ImageFilter, ImageChops

ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "art" / "pads"
OUT = ROOT / "app" / "ui" / "assets" / "pads"
WIDTH = 880          # output width in px; height follows the pad's aspect ratio
MARGIN = 0.02        # transparent margin around the pad, as a fraction of the width
FILL_TOL = 14        # how far from the edge colour still counts as background

# Button centres measured on each normalised photo (880 px wide; zoomed 10 px grid, cross-checked with
# dark/light-pixel centroids where the contrast allows). The photos are product shots from different
# angles, so no single layout fits all four without distorting them: each colour keeps its own points.
# Order: D-pad centre, Y, X, B, A, left stick, right stick, Options, Menu, Assistant, Capture, Stadia.
REFERENCE = "Wasabi"  # the layout the other photos are moved onto
POINTS = {
    "Wasabi":       [(214, 145), (681, 92), (631, 142), (741, 145), (692, 195), (331, 240), (600, 240),
                     (348, 88), (541, 89), (388, 144), (510, 144), (459, 256)],
    "JustBlack":    [(205, 140), (679, 89), (624, 137), (736, 141), (681, 190), (307, 238), (577, 238),
                     (343, 83), (537, 83), (379, 139), (502, 139), (437, 250)],
    "NightBlue":    [(206, 127), (675, 80), (621, 126), (731, 129), (679, 175), (312, 212), (575, 212),
                     (344, 74), (532, 72), (381, 129), (500, 128), (437, 232)],
    "ClearlyWhite": [(201, 145), (672, 94), (614, 144), (725, 145), (666, 196), (282, 240), (548, 238),
                     (334, 89), (526, 89), (369, 144), (491, 144), (423, 255)],
}
# Highlight radius per control and the D-pad arm offset from its centre (same physical pad, ~same scale).
RADIUS = {"ABXY": 26, "DPAD": 19, "STICK": 46, "SMALL": 16, "PILL": 19, "STADIA": 28}
ARM = 40

PADS = {
    "JustBlack": "just-black.webp",
    "Wasabi": "wasabi.jpg",
    "NightBlue": "night-blue.webp",
    "ClearlyWhite": "clearly-white.jpg",
}

def cut_out(im: Image.Image) -> Image.Image:
    rgb = im.convert("RGB")
    w, h = rgb.size
    work = rgb.copy()
    key = (255, 0, 255)
    # Flood from every edge pixel that is still background-coloured.
    for x in range(0, w, 4):
        for y in (0, h - 1):
            if work.getpixel((x, y)) != key:
                ImageDraw.floodfill(work, (x, y), key, thresh=FILL_TOL)
    for y in range(0, h, 4):
        for x in (0, w - 1):
            if work.getpixel((x, y)) != key:
                ImageDraw.floodfill(work, (x, y), key, thresh=FILL_TOL)
    bg = Image.new("L", (w, h), 0)
    pixels = work.get_flattened_data() if hasattr(work, "get_flattened_data") else work.getdata()
    bg.putdata([255 if p == key else 0 for p in pixels])
    alpha = ImageChops.invert(bg)
    # Shrink past the light fringe the white background leaves on the outline, then soften for
    # anti-aliasing.
    alpha = alpha.filter(ImageFilter.MinFilter(5)).filter(ImageFilter.GaussianBlur(0.9))
    # Decontaminate the edge: semi-transparent pixels still hold some of the white background; remove
    # it (colour = (c - (1 - a) * white) / a) so the outline doesn't glow on the app's dark background.
    src = rgb.get_flattened_data() if hasattr(rgb, "get_flattened_data") else rgb.getdata()
    al = alpha.get_flattened_data() if hasattr(alpha, "get_flattened_data") else alpha.getdata()
    px = []
    for (r, g, b), a in zip(src, al):
        if 0 < a < 255:
            f = a / 255
            r, g, b = (max(0, min(255, round((c - (1 - f) * 255) / f))) for c in (r, g, b))
        px.append((r, g, b, a))
    out = Image.new("RGBA", rgb.size)
    out.putdata(px)
    return out

def normalise(im: Image.Image) -> Image.Image:
    box = im.getchannel("A").point(lambda a: 255 if a > 24 else 0).getbbox()
    pad = im.crop(box)
    inner = round(WIDTH * (1 - 2 * MARGIN))
    scaled = pad.resize((inner, round(pad.height * inner / pad.width)), Image.LANCZOS)
    m = round(WIDTH * MARGIN)
    canvas = Image.new("RGBA", (WIDTH, scaled.height + 2 * m), (0, 0, 0, 0))
    canvas.alpha_composite(scaled, (m, m))
    return canvas

def fit_similarity(src, dst):
    """Least-squares uniform scale s and shift (tx, ty) with dst ~= s * src + t (no rotation/shear)."""
    n = len(src)
    mx, my = sum(p[0] for p in src) / n, sum(p[1] for p in src) / n
    nx, ny = sum(p[0] for p in dst) / n, sum(p[1] for p in dst) / n
    num = sum((sx - mx) * (dx - nx) + (sy - my) * (dy - ny) for (sx, sy), (dx, dy) in zip(src, dst))
    den = sum((sx - mx) ** 2 + (sy - my) ** 2 for sx, sy in src)
    s = num / den
    return s, nx - s * mx, ny - s * my

def align(img: Image.Image, name: str, size):
    """Moves and uniformly scales a photo onto the reference's button layout (never distorts it), so a
    colour wipe between two pads lines up as well as the photos allow. Returns the image and its points
    mapped into the new frame."""
    s, tx, ty = fit_similarity(POINTS[name], POINTS[REFERENCE])
    moved = [(round(s * x + tx), round(s * y + ty)) for x, y in POINTS[name]]
    miss = max(((a - c) ** 2 + (b - d) ** 2) ** 0.5 for (a, b), (c, d) in zip(moved, POINTS[REFERENCE]))
    print(f"  {name}: scale {s:.3f}, shift ({tx:+.1f}, {ty:+.1f}); buttons now within {miss:.0f} px of {REFERENCE}")
    out = img.transform(size, Image.AFFINE, (1 / s, 0, -tx / s, 0, 1 / s, -ty / s), resample=Image.BICUBIC)
    return out, moved

def points_js(heights) -> str:
    """The per-colour control map for app/ui/js/pad-points.js: KEY -> [x, y, r] in image pixels."""
    lines = ["// Generated by scripts/make_pad_images.py from the measured POINTS table; do not edit by hand.",
             "// Control centres and highlight radii in each pad photo's own pixels (880 px wide).", "",
             "export const PAD_W = %d;" % WIDTH, "export const PAD_H = { %s };" % ", ".join(f"{k}: {v}" for k, v in heights.items()),
             "", "export const POINTS = {"]
    for name, p in POINTS_OUT.items():
        d, y, x, b, a, ls, rs, opt, menu, ast, cap, st = p
        r = RADIUS
        ctl = {
            "DPAD_UP": (d[0], d[1] - ARM, r["DPAD"]), "DPAD_DOWN": (d[0], d[1] + ARM, r["DPAD"]),
            "DPAD_LEFT": (d[0] - ARM, d[1], r["DPAD"]), "DPAD_RIGHT": (d[0] + ARM, d[1], r["DPAD"]),
            "Y": (*y, r["ABXY"]), "X": (*x, r["ABXY"]), "B": (*b, r["ABXY"]), "A": (*a, r["ABXY"]),
            "L3": (*ls, r["STICK"]), "R3": (*rs, r["STICK"]),
            "OPTIONS": (*opt, r["PILL"]), "MENU": (*menu, r["PILL"]),
            "ASSISTANT": (*ast, r["SMALL"]), "CAPTURE": (*cap, r["SMALL"]), "STADIA": (*st, r["STADIA"]),
            # Bumpers sit on the top edge above the D-pad / face buttons; triggers are behind them.
            "LB": (d[0] - 20, 24, 0), "RB": (x[0] + 50, 24, 0), "L2_CLICK": (d[0] - 20, -30, 0), "R2_CLICK": (x[0] + 50, -30, 0),
        }
        body = ", ".join(f"{k}: [{v[0]}, {v[1]}, {v[2]}]" for k, v in ctl.items())
        lines.append(f"  {name}: {{ {body} }},")
    lines.append("};")
    return "\n".join(lines) + "\n"

POINTS_OUT = {}

def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    imgs = {name: normalise(cut_out(Image.open(SRC / file))) for name, file in PADS.items()}
    size = imgs[REFERENCE].size
    heights = {}
    for name, img in imgs.items():
        if name == REFERENCE:
            out, POINTS_OUT[name] = img, list(POINTS[name])
        else:
            out, POINTS_OUT[name] = align(img, name, size)
        out.save(OUT / f"{name}.png", optimize=True)
        heights[name] = out.size[1]
        print(f"{name}: {PADS[name]} -> {out.size[0]}x{out.size[1]}")
    (ROOT / "app" / "ui" / "js" / "pad-points.js").write_text(points_js(heights), encoding="utf-8", newline="\n")
    print("wrote app/ui/js/pad-points.js")

if __name__ == "__main__":
    main()
