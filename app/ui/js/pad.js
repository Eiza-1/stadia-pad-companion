// The Stadia pad: the real product photo for the chosen colour, with live overlays. Pressed controls
// light up over their spot on the photo, a dot follows each stick, and L2 / R2 fill bars sit above it.
// Every element that can light carries data-k (the button flag name), so mapping/live views find it.
// Photos and per-colour control positions come from scripts/make_pad_images.py.

import { PAD_W, PAD_H, POINTS } from "./pad-points.js";

/** Room above the photo (photo pixels) for the trigger bars. */
export const PAD_TOP = 60;
const TOP = PAD_TOP;
const KEYS = Object.keys(POINTS);
const keyOf = (k) => (POINTS[k] ? k : "JustBlack");

/** viewBox height for a colour (photo + trigger room). */
const viewH = (key) => PAD_H[key] + TOP;

/** Control centres in viewBox units (x, y from the photo's top-left; y < 0 is above the photo). */
export function padPoints(key) {
  const p = POINTS[keyOf(key)];
  return Object.fromEntries(Object.entries(p).map(([k, [x, y]]) => [k, [x, y]]));
}

/** Drawing width in viewBox units, and its aspect ratio (height / width) for a colour. */
export { PAD_W };
export const padAspect = (key) => viewH(keyOf(key)) / PAD_W;

function markup(key) {
  const p = POINTS[key];
  // Trigger bars are centred on the L2 / R2 points, so mapping leader lines land on them.
  const bar = (t, k) => {
    const [cx, cy] = p[k];
    return `
    <g class="trig" data-t="${t}">
      <rect data-k="${k}" class="track" x="${cx - 100}" y="${cy - 7}" width="200" height="14" rx="7"/>
      <rect class="fill" x="${cx - 100}" y="${cy - 7}" width="200" height="14" rx="7"/>
    </g>`;
  };
  const hot = (k) => {
    const [x, y, r] = p[k];
    return `<circle data-k="${k}" class="hot" cx="${x}" cy="${y}" r="${r}"/>`;
  };
  const bumper = (k) => {
    const [x, y] = p[k];
    return `<ellipse data-k="${k}" class="hot bumper" cx="${x}" cy="${y}" rx="78" ry="13"/>`;
  };
  const stick = (s, k) => {
    const [x, y, r] = p[k];
    return `<g class="stick" data-s="${s}">
      <circle data-k="${k}" class="hot" cx="${x}" cy="${y}" r="${r}"/>
      <circle class="thumb" cx="${x}" cy="${y}" r="9"/>
    </g>`;
  };
  return `
    ${bar("l2", "L2_CLICK")}${bar("r2", "R2_CLICK")}
    <image href="assets/pads/${key}.png" x="0" y="0" width="${PAD_W}" height="${PAD_H[key]}" preserveAspectRatio="none"/>
    ${bumper("LB")}${bumper("RB")}
    ${["DPAD_UP", "DPAD_DOWN", "DPAD_LEFT", "DPAD_RIGHT", "Y", "X", "B", "A", "OPTIONS", "MENU", "ASSISTANT", "CAPTURE", "STADIA"].map(hot).join("")}
    ${stick("l", "L3")}${stick("r", "R3")}`;
}

/** Draws the pad for a colour key (e.g. "Wasabi") into an <svg>. */
export function renderPad(svgEl, key) {
  svgEl.classList.add("pad");
  colorPad(svgEl, key, true);
}

/** Switches a drawn pad to another colour (rebuilt, so every part is fresh; live state is redrawn by
 *  the caller's "viewchange"). */
export function colorPad(svgEl, key, force = false) {
  const k = keyOf(key);
  if (!force && svgEl.dataset.pad === k) return;
  svgEl.dataset.pad = k;
  svgEl.setAttribute("viewBox", `0 ${-TOP} ${PAD_W} ${viewH(k)}`);
  svgEl.innerHTML = markup(k);
}

/** Warms the image cache so a colour switch shows the new photo at once. */
export function preloadPads() {
  for (const k of KEYS) new Image().src = `assets/pads/${k}.png`;
}

const pressedOf = (buttons) => new Set(buttons ? String(buttons).split(" | ") : []);
const STICK_TRAVEL = 20; // photo px at full deflection

/** Lights pressed controls, moves the stick dots and fills the trigger bars from a bridge PadState. */
export function applyPadState(svgEl, s) {
  if (!svgEl || !s) return;
  const down = pressedOf(s.buttons);
  svgEl.querySelectorAll("[data-k]").forEach((el) => el.classList.toggle("on", down.has(el.dataset.k)));
  for (const [side, x, y] of [["l", s.lx, s.ly], ["r", s.rx, s.ry]]) {
    const dot = svgEl.querySelector(`[data-s="${side}"] .thumb`);
    if (!dot) continue;
    const fx = (x - 127.5) / 127.5, fy = (y - 127.5) / 127.5;
    dot.style.transform = `translate(${fx * STICK_TRAVEL}px, ${fy * STICK_TRAVEL}px)`;
    dot.classList.toggle("show", Math.hypot(fx, fy) > 0.12);
  }
  for (const [t, v] of [["l2", s.l2], ["r2", s.r2]]) {
    const f = svgEl.querySelector(`[data-t="${t}"] .fill`);
    if (f) f.style.transform = `scaleX(${v / 255})`;
  }
}
