// The Stadia pad as inline SVG. Every control carries data-k (button flag name) so the live
// state from the bridge can light it; sticks move and triggers fill.

const SHELL = "M86 52 Q120 34 176 40 Q220 44 264 40 Q320 34 354 52 Q402 82 420 196 Q430 262 384 272 Q348 280 322 236 L298 196 Q220 186 142 196 L118 236 Q92 280 56 272 Q10 262 20 196 Q38 82 86 52Z";

const MARKUP = `
  <g class="trig" data-t="l2"><rect x="92" y="10" width="64" height="7" rx="3.5" class="track"/><rect x="92" y="10" width="64" height="7" rx="3.5" class="fill"/></g>
  <g class="trig" data-t="r2"><rect x="284" y="10" width="64" height="7" rx="3.5" class="track"/><rect x="284" y="10" width="64" height="7" rx="3.5" class="fill"/></g>
  <g filter="url(#padSoft)">
    <path data-k="LB" d="M78 58 Q92 30 140 30 L170 34 L160 52 Z" fill="var(--b2)"/>
    <path data-k="RB" d="M362 58 Q348 30 300 30 L270 34 L280 52 Z" fill="var(--b2)"/>
    <path d="${SHELL}" fill="var(--b)"/>
    <path d="${SHELL}" fill="url(#padShade)" style="opacity: var(--sh, 1)"/>
    <path d="${SHELL}" fill="none" stroke="var(--rim, transparent)" stroke-width="1.5"/>
  </g>
  <circle cx="104" cy="118" r="38" fill="var(--b2)" opacity=".55"/>
  <g fill="var(--k)" stroke="var(--ks, transparent)" stroke-width="1">
    <rect data-k="DPAD_UP" x="95" y="86" width="18" height="27" rx="5"/>
    <rect data-k="DPAD_DOWN" x="95" y="123" width="18" height="27" rx="5"/>
    <rect data-k="DPAD_LEFT" x="72" y="109" width="27" height="18" rx="5"/>
    <rect data-k="DPAD_RIGHT" x="109" y="109" width="27" height="18" rx="5"/>
    <rect x="95" y="109" width="18" height="18"/>
  </g>
  <circle cx="336" cy="118" r="40" fill="var(--b2)" opacity=".55"/>
  <g fill="var(--k)" stroke="var(--ks, transparent)" stroke-width="1">
    <circle data-k="A" cx="336" cy="144" r="13"/><circle data-k="B" cx="362" cy="118" r="13"/>
    <circle data-k="X" cx="310" cy="118" r="13"/><circle data-k="Y" cx="336" cy="92" r="13"/>
  </g>
  <g fill="url(#padCap)" pointer-events="none"><circle cx="336" cy="144" r="13"/><circle cx="362" cy="118" r="13"/><circle cx="310" cy="118" r="13"/><circle cx="336" cy="92" r="13"/></g>
  <g fill="var(--t)" font-family="Roboto Flex, sans-serif" font-size="12" font-weight="600" text-anchor="middle" pointer-events="none">
    <text x="336" y="148">A</text><text x="362" y="122">B</text><text x="310" y="122">X</text><text x="336" y="96">Y</text>
  </g>
  <circle cx="168" cy="170" r="30" fill="var(--b2)"/>
  <g class="stick" data-s="l"><circle data-k="L3" cx="168" cy="170" r="21" fill="var(--k)" stroke="var(--ks, transparent)"/><circle cx="168" cy="170" r="21" fill="url(#padCap)" pointer-events="none"/><circle cx="168" cy="170" r="13" fill="none" stroke="var(--b2)" stroke-width="1.5" opacity=".6"/></g>
  <circle cx="272" cy="170" r="30" fill="var(--b2)"/>
  <g class="stick" data-s="r"><circle data-k="R3" cx="272" cy="170" r="21" fill="var(--k)" stroke="var(--ks, transparent)"/><circle cx="272" cy="170" r="21" fill="url(#padCap)" pointer-events="none"/><circle cx="272" cy="170" r="13" fill="none" stroke="var(--b2)" stroke-width="1.5" opacity=".6"/></g>
  <circle data-k="STADIA" cx="220" cy="104" r="15" fill="var(--k)" stroke="var(--ks, transparent)"/>
  <image href="assets/stadia.svg" x="208" y="96" width="24" height="16" pointer-events="none"/>
  <circle data-k="ASSISTANT" cx="192" cy="134" r="7" fill="var(--k)" stroke="var(--ks, transparent)"/>
  <circle data-k="CAPTURE" cx="248" cy="134" r="7" fill="var(--k)" stroke="var(--ks, transparent)"/>
  <rect data-k="OPTIONS" x="180" y="78" width="16" height="7" rx="3.5" fill="var(--k)" stroke="var(--ks, transparent)"/>
  <rect data-k="MENU" x="244" y="78" width="16" height="7" rx="3.5" fill="var(--k)" stroke="var(--ks, transparent)"/>
  <rect x="208" y="60" width="24" height="3" rx="1.5" fill="var(--led)" class="led"/>`;

/** Centre of each control in viewBox units (440×290), for hotspots. */
export const PAD_POINTS = {
  A: [336, 144], B: [362, 118], X: [310, 118], Y: [336, 92],
  LB: [130, 40], RB: [310, 40], L2_CLICK: [124, 13], R2_CLICK: [316, 13],
  L3: [168, 170], R3: [272, 170], OPTIONS: [188, 81], MENU: [252, 81], STADIA: [220, 104],
  ASSISTANT: [192, 134], CAPTURE: [248, 134],
  DPAD_UP: [104, 96], DPAD_DOWN: [104, 140], DPAD_LEFT: [82, 118], DPAD_RIGHT: [126, 118],
};

/** Draws the pad into an empty <svg viewBox="0 0 440 290"> and colours it. */
export function renderPad(svgEl, colors) {
  svgEl.setAttribute("viewBox", "0 0 440 290");
  svgEl.classList.add("pad");
  svgEl.innerHTML = MARKUP;
  if (colors) colorPad(svgEl, colors);
}

export function colorPad(svgEl, c) {
  for (const k of ["b", "b2", "k", "t", "led", "ks", "rim", "sh", "hi"]) {
    if (c[k]) svgEl.style.setProperty(`--${k}`, c[k]);
    else svgEl.style.removeProperty(`--${k}`); // e.g. no "hi": pressed keys use the scheme primary
  }
}

const pressedOf = (buttons) => new Set(buttons ? String(buttons).split(" | ") : []);

/** Lights pressed buttons, moves sticks ±8 px and fills triggers from a bridge PadState. */
export function applyPadState(svgEl, s) {
  if (!svgEl || !s) return;
  const down = pressedOf(s.buttons);
  svgEl.querySelectorAll("[data-k]").forEach((el) => el.classList.toggle("on", down.has(el.dataset.k)));
  const move = (g, x, y) => {
    if (g) g.style.transform = `translate(${((x - 127.5) / 127.5) * 8}px, ${((y - 127.5) / 127.5) * 8}px)`;
  };
  move(svgEl.querySelector('[data-s="l"]'), s.lx, s.ly);
  move(svgEl.querySelector('[data-s="r"]'), s.rx, s.ry);
  for (const [t, v] of [["l2", s.l2], ["r2", s.r2]]) {
    const f = svgEl.querySelector(`[data-t="${t}"] .fill`);
    if (f) f.style.transform = `scaleX(${v / 255})`;
  }
}
