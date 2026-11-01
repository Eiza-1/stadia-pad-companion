"""Builds the pad photos the app uses (app/ui/assets/pads/*.png) from the originals in art/pads/.

Each original is a product photo on a white background. For each one:
  1. the background is found by flood-filling from the image edges (so white highlights *inside* the
     pad, e.g. on Clearly White, are kept), then the edge is softened by 1 px for clean anti-aliasing;
  2. the image is cropped to the pad and scaled to a common width, then moved and uniformly scaled onto
     the reference photo's button layout (never warped), so colour wipes line up;
  3. each colour's measured button centres (POINTS below), carried through that move, are written to
     app/ui/js/pad-points.js.

Run: python scripts/make_pad_images.py   (needs Pillow, numpy, opencv-python-headless)
"""
from pathlib import Path
import cv2
import numpy as np
from PIL import Image, ImageDraw, ImageFilter

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

SUPER = 4            # cut-out is computed at 4x the source resolution, then downsampled (true anti-aliasing)
SAVE = 1.5           # PNGs are saved at 1.5x the layout width (1320 px) so they stay sharp on HiDPI screens

def measure_box(im: Image.Image):
    """The pad's bounding box in source pixels, by the method the POINTS table was measured with
    (hard flood-fill mask, 2 px erosion, light blur, alpha > 24). Only used to place the crop."""
    rgb = im.convert("RGB")
    w, h = rgb.size
    work = rgb.copy()
    key = (255, 0, 255)
    for x in range(0, w, 4):
        for y in (0, h - 1):
            if work.getpixel((x, y)) != key:
                ImageDraw.floodfill(work, (x, y), key, thresh=FILL_TOL)
    for y in range(0, h, 4):
        for x in (0, w - 1):
            if work.getpixel((x, y)) != key:
                ImageDraw.floodfill(work, (x, y), key, thresh=FILL_TOL)
    a = np.array(work)
    fg = Image.fromarray(np.where((a == key).all(-1), 0, 255).astype(np.uint8), "L")
    fg = fg.filter(ImageFilter.MinFilter(5)).filter(ImageFilter.GaussianBlur(0.9))
    return fg.point(lambda v: 255 if v > 24 else 0).getbbox()

def cut_out(im: Image.Image) -> Image.Image:
    """Cuts a product photo off its white background with a smooth, anti-aliased outline.

    The small JPEG/WebP originals have noisy, blocky edges; a per-pixel mask follows that noise and
    looks jagged. Instead: denoise lightly, upscale 4x (Lanczos), flood-fill the background at that
    scale, smooth the outline as a curve (blur + re-threshold), pull it inside the white fringe, and
    let the later downsample produce the anti-aliased edge. Edge colours are taken from the pad's own
    interior, so no white halo shows on the dark UI."""
    rgb = np.array(im.convert("RGB"))
    rgb = cv2.fastNlMeansDenoisingColored(rgb, None, 4, 4, 5, 15)   # remove JPEG blocking, keep detail
    h, w = rgb.shape[:2]
    big = cv2.resize(rgb, (w * SUPER, h * SUPER), interpolation=cv2.INTER_LANCZOS4)
    H, W = big.shape[:2]
    # Background = everything connected to the border that is close to the border's white.
    ff = np.zeros((H + 2, W + 2), np.uint8)
    tol = (FILL_TOL,) * 3
    flags = 4 | cv2.FLOODFILL_MASK_ONLY | cv2.FLOODFILL_FIXED_RANGE | (255 << 8)
    work = big.copy()
    for x in range(W):
        for y in (0, H - 1):
            if ff[y + 1, x + 1] == 0 and big[y, x].min() > 200:
                cv2.floodFill(work, ff, (x, y), 0, tol, tol, flags)
    for y in range(H):
        for x in (0, W - 1):
            if ff[y + 1, x + 1] == 0 and big[y, x].min() > 200:
                cv2.floodFill(work, ff, (x, y), 0, tol, tol, flags)
    fg = (ff[1:-1, 1:-1] == 0).astype(np.float32)
    # Some photos crop the grips at the frame; the outer 3 px there are backdrop or rim light.
    b = 3 * SUPER
    fg[:b], fg[-b:], fg[:, :b], fg[:, -b:] = 0, 0, 0, 0
    # Work on a zero-padded copy: OpenCV's morphology treats outside-the-image as foreground, which
    # would let the pad grow back out to the frame.
    P = 12 * SUPER
    fg = cv2.copyMakeBorder(fg, P, P, P, P, cv2.BORDER_CONSTANT, value=0)
    big = cv2.copyMakeBorder(big, P, P, P, P, cv2.BORDER_REPLICATE)
    # Keep only the pad itself (largest blob) and fill pin-holes inside it.
    n, lab, stats, _ = cv2.connectedComponentsWithStats(fg.astype(np.uint8), 8)
    if n > 1:
        fg = (lab == 1 + np.argmax(stats[1:, cv2.CC_STAT_AREA])).astype(np.float32)
    # Close dents where a light pad edge melts into the white backdrop, then smooth the outline as a
    # curve (blur ~3 source px, re-threshold): removes stair-steps and compression bumps.
    close = cv2.getStructuringElement(cv2.MORPH_ELLIPSE, (int(6 * SUPER) | 1,) * 2)
    fg = cv2.morphologyEx(fg, cv2.MORPH_CLOSE, close)
    sm = cv2.GaussianBlur(fg, (0, 0), 3 * SUPER)
    fg = (sm > 0.5).astype(np.float32)
    # Pull the edge ~1.5 source px inward, past the light fringe the white backdrop leaves.
    k = cv2.getStructuringElement(cv2.MORPH_ELLIPSE, (int(3 * SUPER) | 1,) * 2)
    fg = cv2.erode(fg, k)
    # A hair of softness at 4x becomes a clean ~1 px anti-aliased edge after the downsample.
    alpha = cv2.GaussianBlur(fg, (0, 0), 0.6 * SUPER)
    # Edge colour: pixels near the outline still mix in the white backdrop; replace them with the
    # average colour of the pad just inside (colour bleed), weighted by how deep inside they are.
    inner = cv2.erode(fg, cv2.getStructuringElement(cv2.MORPH_ELLIPSE, (int(3 * SUPER) | 1,) * 2))
    f = big.astype(np.float32)
    acc = cv2.GaussianBlur(f * inner[..., None], (0, 0), 2.5 * SUPER)
    wgt = cv2.GaussianBlur(inner, (0, 0), 2.5 * SUPER)[..., None]
    bleed = acc / np.maximum(wgt, 1e-4)
    depth = cv2.GaussianBlur(inner, (0, 0), 0.8 * SUPER)[..., None]
    col = f * depth + bleed * (1 - depth)
    out = np.dstack([np.clip(col, 0, 255), alpha * 255]).astype(np.uint8)
    out = out[P:-P, P:-P]
    res = Image.fromarray(out, "RGBA")
    # Crop box for normalise(): measured exactly as when POINTS were measured (see measure_box), so
    # the button positions stay valid whatever the cut-out does.
    res.info["box"] = tuple(v * SUPER for v in measure_box(im))
    return res

def normalise(im: Image.Image) -> Image.Image:
    """Crops to the pad and scales it to the output width (SAVE x layout width) with a margin."""
    width = round(WIDTH * SAVE)
    box = im.info.get("box") or im.getchannel("A").point(lambda a: 255 if a > 24 else 0).getbbox()
    pad = np.array(im.crop(box)).astype(np.float32)
    # The edge pixels already carry the pad's own colour (see cut_out), so colour and alpha can be
    # resized straight without a dark or white fringe.
    inner = round(width * (1 - 2 * MARGIN))
    size = (inner, round(pad.shape[0] * inner / pad.shape[1]))
    sc = cv2.resize(pad, size, interpolation=cv2.INTER_AREA)
    # Gentle unsharp mask restores the crispness lost in the source's compression.
    blur = cv2.GaussianBlur(sc[..., :3], (0, 0), 1.2)
    sc[..., :3] = sc[..., :3] + 0.2 * (sc[..., :3] - blur)
    scaled = Image.fromarray(np.clip(sc, 0, 255).astype(np.uint8), "RGBA")
    m = round(width * MARGIN)
    canvas = Image.new("RGBA", (width, scaled.height + 2 * m), (0, 0, 0, 0))
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
    tx2, ty2 = tx * SAVE, ty * SAVE
    out = img.transform(size, Image.AFFINE, (1 / s, 0, -tx2 / s, 0, 1 / s, -ty2 / s), resample=Image.BICUBIC)
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
        heights[name] = round(out.size[1] / SAVE)
        print(f"{name}: {PADS[name]} -> {out.size[0]}x{out.size[1]}")
    (ROOT / "app" / "ui" / "js" / "pad-points.js").write_text(points_js(heights), encoding="utf-8", newline="\n")
    print("wrote app/ui/js/pad-points.js")

if __name__ == "__main__":
    main()
