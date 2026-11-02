// The Stadia pad: the real product photo for the chosen colour, brought to life. Each control is a
// piece of the photo itself: pressed buttons sink into the shell, the D-pad arm and bumpers dip, the
// stick caps slide with the sticks, and L2 / R2 fill bars sit above it (triggers aren't in a front photo).
// Every element that reacts carries data-k (the button flag name), so mapping/live views find it.
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

let seq = 0; // keeps clip-path ids unique when several pads are on screen

/** Geometry of each moving part, in photo units: a clip shape around the real control in the photo. */
function parts(p) {
  const out = [];
  const circ = (k, r) => { const [x, y] = p[k]; out.push({ k, cx: x, cy: y, shape: { c: [x, y, r] } }); };
  for (const k of ["Y", "X", "B", "A"]) circ(k, p[k][2]);
  for (const k of ["ASSISTANT", "CAPTURE", "STADIA"]) circ(k, p[k][2]);
  // Options / Menu are pills, so they get an elliptical cut-out.
  for (const k of ["OPTIONS", "MENU"]) { const [x, y, r] = p[k]; out.push({ k, cx: x, cy: y, shape: { e: [x, y, r * 1.45, r * 0.68] } }); }
  // D-pad arms: a box from near the centre out to the arm's tip.
  const [dx, dy] = [p.DPAD_UP[0], p.DPAD_UP[1] + ARM_OF(p)];
  // Kept inside the arm (narrower than it), so pressing darkens the arm in place without a box edge.
  const W = 15, IN = 14, OUT = 58;
  const arm = (k, x, y, w, h) => out.push({ k, cx: x + w / 2, cy: y + h / 2, shape: { r: [x, y, w, h] }, cls: "dpad" });
  arm("DPAD_UP", dx - W, dy - OUT, 2 * W, OUT - IN);
  arm("DPAD_DOWN", dx - W, dy + IN, 2 * W, OUT - IN);
  arm("DPAD_LEFT", dx - OUT, dy - W, OUT - IN, 2 * W);
  arm("DPAD_RIGHT", dx + IN, dy - W, OUT - IN, 2 * W);
  // Bumpers: the dark band along the top edge.
  for (const k of ["LB", "RB"]) { const [x] = p[k]; out.push({ k, cx: x, cy: 18, shape: { e: [x, 18, 92, 22] }, cls: "bumper" }); }
  return out;
}
const ARM_OF = (p) => p.DPAD_DOWN[1] - p.DPAD_UP[1] >> 1;

function clipShape({ c, r, e }) {
  if (c) return `<circle cx="${c[0]}" cy="${c[1]}" r="${c[2]}"/>`;
  if (r) return `<rect x="${r[0]}" y="${r[1]}" width="${r[2]}" height="${r[3]}" rx="9"/>`;
  return `<ellipse cx="${e[0]}" cy="${e[1]}" rx="${e[2]}" ry="${e[3]}"/>`;
}

function markup(key) {
  const p = POINTS[key];
  const id = `pd${++seq}`;
  const img = `assets/pads/${key}.png`;
  const photo = (clip, cls = "") =>
    `<image class="${cls}" href="${img}" x="0" y="0" width="${PAD_W}" height="${PAD_H[key]}" preserveAspectRatio="none" clip-path="url(#${clip})"/>`;
  // Trigger bars are centred on the L2 / R2 points, so mapping leader lines land on them.
  const bar = (t, k) => {
    const [cx, cy] = p[k];
    return `
    <g class="trig" data-t="${t}">
      <rect data-k="${k}" class="track" x="${cx - 100}" y="${cy - 7}" width="200" height="14" rx="7"/>
      <rect class="fill" x="${cx - 100}" y="${cy - 7}" width="200" height="14" rx="7"/>
    </g>`;
  };
  const ps = parts(p);
  // Each press-able part is a copy of the photo clipped to that control. Pressed, it sinks (scales
  // in / moves down) and darkens over a soft shadow that hides the untouched photo underneath.
  const clips = ps.map((q, i) => `<clipPath id="${id}c${i}">${clipShape(q.shape)}</clipPath>`).join("");
  const pressable = ps.map((q, i) => `
    <g class="part${q.cls ? " " + q.cls : ""}" data-k="${q.k}">
      <g class="well" clip-path="url(#${id}c${i})"><rect x="${q.cx - 120}" y="${q.cy - 120}" width="240" height="240"/></g>
      <g class="cap" style="transform-origin:${q.cx}px ${q.cy}px">${photo(`${id}c${i}`)}</g>
      ${q.cls === "dpad" ? `<g class="tint" clip-path="url(#${id}c${i})"><rect x="${q.cx - 80}" y="${q.cy - 80}" width="160" height="160"/></g>` : ""}
    </g>`).join("");
  // Sticks: the cap is cut out of the photo and slides over a dark socket as the stick moves; a
  // stick click sinks it.
  const stick = (s, k) => {
    const [x, y, r] = p[k];
    const cr = Math.round(r * 0.8);
    return `<clipPath id="${id}${s}"><circle cx="${x}" cy="${y}" r="${cr}"/></clipPath>
    <g class="stick" data-s="${s}">
      <circle class="socket" cx="${x}" cy="${y}" r="${cr + 1}" fill="url(#${id}sock)"/>
      <g class="move">
        <g class="part" data-k="${k}">
          <g class="cap" style="transform-origin:${x}px ${y}px">${photo(`${id}${s}`)}
            <circle class="sheen" cx="${x}" cy="${y}" r="${cr}" fill="url(#${id}sheen)"/></g>
        </g>
      </g>
    </g>`;
  };
  return `
    <defs>
      ${clips}
      <radialGradient id="${id}sock"><stop offset="0" stop-color="#050506"/><stop offset=".75" stop-color="#121215"/><stop offset="1" stop-color="#26262b"/></radialGradient>
      <radialGradient id="${id}sheen" cx=".5" cy=".5" r=".5"><stop offset=".55" stop-color="#000" stop-opacity="0"/><stop offset="1" stop-color="#000" stop-opacity=".35"/></radialGradient>
    </defs>
    ${bar("l2", "L2_CLICK")}${bar("r2", "R2_CLICK")}
    <image class="base" href="${img}" x="0" y="0" width="${PAD_W}" height="${PAD_H[key]}" preserveAspectRatio="none"/>
    ${pressable}
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
const STICK_TRAVEL = 15; // photo px the cap slides at full deflection

/** Presses the controls in the photo, slides the stick caps and fills the trigger bars from a bridge
 *  PadState. */
export function applyPadState(svgEl, s) {
  if (!svgEl || !s) return;
  const down = pressedOf(s.buttons);
  svgEl.querySelectorAll("[data-k]").forEach((el) => el.classList.toggle("on", down.has(el.dataset.k)));
  for (const [side, x, y] of [["l", s.lx, s.ly], ["r", s.rx, s.ry]]) {
    const g = svgEl.querySelector(`[data-s="${side}"]`);
    if (!g) continue;
    let fx = (x - 127.5) / 127.5, fy = (y - 127.5) / 127.5;
    const m = Math.hypot(fx, fy);
    if (m > 1) { fx /= m; fy /= m; }  // square stick gates: keep the cap on a round path
    g.querySelector(".move").style.transform = `translate(${fx * STICK_TRAVEL}px, ${fy * STICK_TRAVEL}px)`;
    g.classList.toggle("tilt", m > 0.08);
  }
  for (const [t, v] of [["l2", s.l2], ["r2", s.r2]]) {
    const f = svgEl.querySelector(`[data-t="${t}"] .fill`);
    if (f) f.style.transform = `scaleX(${v / 255})`;
  }
}
