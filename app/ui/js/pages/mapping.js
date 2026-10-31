// Button mapping: the live pad with a labelled chip per control (leader lines to the button).
// Clicking a chip opens a menu of Xbox targets.

import { renderPad, PAD_POINTS } from "../pad.js";
import { SCHEMES, current } from "../theme.js";
import { settings, profile, save, onSettings } from "../ipc.js";
import { h } from "../ui.js";

const NAMES = {
  A: "A", B: "B", X: "X", Y: "Y", LB: "L1", RB: "R1", L2_CLICK: "L2 click", R2_CLICK: "R2 click",
  L3: "L3", R3: "R3", OPTIONS: "Options", MENU: "Menu", STADIA: "Stadia button",
  DPAD_UP: "D-pad up", DPAD_DOWN: "D-pad down", DPAD_LEFT: "D-pad left", DPAD_RIGHT: "D-pad right",
};
const XB = { A: "A", B: "B", X: "X", Y: "Y", LB: "LB", RB: "RB", L3: "Left stick click", R3: "Right stick click",
  Back: "Back / View", Start: "Start / Menu", Guide: "Xbox Guide",
  DpadUp: "D-pad up", DpadDown: "D-pad down", DpadLeft: "D-pad left", DpadRight: "D-pad right" };
// Default physical → Xbox mapping (mirrors pad-core mapping::default_target).
const DEFAULT = { A: "A", B: "B", X: "X", Y: "Y", LB: "LB", RB: "RB", L3: "L3", R3: "R3", OPTIONS: "Back",
  MENU: "Start", STADIA: "Guide", DPAD_UP: "DpadUp", DPAD_DOWN: "DpadDown", DPAD_LEFT: "DpadLeft", DPAD_RIGHT: "DpadRight",
  L2_CLICK: null, R2_CLICK: null };
const LEFT = ["L2_CLICK", "LB", "OPTIONS", "DPAD_UP", "DPAD_LEFT", "DPAD_RIGHT", "DPAD_DOWN", "L3"];
const RIGHT = ["R2_CLICK", "RB", "MENU", "STADIA", "Y", "X", "B", "A", "R3"];

let root, stage, svg, lines, chips = {};

const targetOf = (k) => {
  const t = profile().remap[k];
  if (t === undefined) return DEFAULT[k];
  return t === "Disabled" ? null : t.Xbox;
};
const isChanged = (k) => profile().remap[k] !== undefined;

export function init(el) {
  root = el;
  el.innerHTML = `
    <div style="display:flex;justify-content:space-between;align-items:flex-start">
      <div><h1>Button mapping</h1><p class="lede">Choose what each button does in games. Press a button on the pad to find it.</p></div>
      <div class="maptools"><button class="btn text" id="mapReset"><span class="ms">restart_alt</span>Restore default</button></div>
    </div>
    <div class="mapstage"><svg class="bigpad"></svg><svg class="lines"></svg></div>`;
  stage = el.querySelector(".mapstage");
  svg = el.querySelector(".bigpad");
  lines = el.querySelector(".lines");
  renderPad(svg, SCHEMES[current].pad);
  for (const k of [...LEFT, ...RIGHT]) {
    const c = h(`<button class="chip"><small>${NAMES[k]}</small><span></span></button>`);
    c.onclick = (e) => openMenu(k, c, e);
    stage.append(c);
    chips[k] = c;
  }
  el.querySelector("#mapReset").onclick = () => save(() => (profile().remap = {}));
  onSettings(refresh);
  new ResizeObserver(layout).observe(stage);
  window.addEventListener("padstate", (e) => {
    if (!root.classList.contains("on")) return;
    const down = new Set(String(e.detail.state.buttons || "").split(" | "));
    for (const [k, c] of Object.entries(chips)) c.classList.toggle("lit", down.has(k));
  });
  refresh();
}

export const shown = () => requestAnimationFrame(layout);

function refresh() {
  for (const [k, c] of Object.entries(chips)) {
    const t = targetOf(k);
    c.querySelector("span").textContent = t ? XB[t] : "Not used";
    c.classList.toggle("changed", isChanged(k));
    c.classList.toggle("off", !t && isChanged(k));
  }
}

function layout() {
  const W = stage.clientWidth, H = stage.clientHeight;
  if (!W || !H) return;
  const padW = Math.min(560, W - 2 * 190), padH = padW * (290 / 440);
  const left = (W - padW) / 2, top = Math.max(20, (H - padH) / 2 - 10);
  Object.assign(svg.style, { left: `${left}px`, top: `${top}px`, width: `${padW}px`, height: `${padH}px` });
  const s = padW / 440;
  let d = "", dots = "";
  const place = (keys, side) => {
    const span = Math.min(H - 40, padH + 120), y0 = top + padH / 2 - span / 2;
    keys.forEach((k, i) => {
      const y = y0 + (span * (i + 0.5)) / keys.length;
      const c = chips[k];
      const cw = c.offsetWidth || 130;
      const x = side === "l" ? Math.max(0, left - cw - 36) : Math.min(W - cw, left + padW + 36);
      c.style.left = `${x}px`;
      c.style.top = `${y}px`;
      const [px, py] = PAD_POINTS[k];
      const bx = left + px * s, by = top + py * s;
      const ex = side === "l" ? x + cw : x;
      const mx = side === "l" ? ex + 16 : ex - 16;
      d += `M${ex} ${y} L${mx} ${y} L${bx} ${by} `;
      dots += `<circle cx="${bx}" cy="${by}" r="2.5"/>`;
    });
  };
  place(LEFT, "l");
  place(RIGHT, "r");
  lines.innerHTML = `<path d="${d}"/>${dots}`;
}

let menu;
function closeMenu() {
  menu?.remove();
  menu = null;
  document.removeEventListener("pointerdown", outside, true);
}
const outside = (e) => menu && !menu.contains(e.target) && closeMenu();

function openMenu(k, chip) {
  closeMenu();
  const cur = targetOf(k);
  menu = h(`<div class="menu" role="menu"></div>`);
  const add = (label, value, on) => {
    const b = h(`<button role="menuitem" class="${on ? "on" : ""}"><span class="ms">${on ? "check" : ""}</span><span></span></button>`);
    b.querySelector("span:last-child").textContent = label;
    b.onclick = () => {
      save(() => {
        const remap = profile().remap;
        if (value === "default") delete remap[k];
        else if (value === null) remap[k] = "Disabled";
        else remap[k] = { Xbox: value };
      });
      closeMenu();
    };
    menu.append(b);
  };
  add(`Default (${DEFAULT[k] ? XB[DEFAULT[k]] : "not used"})`, "default", !isChanged(k));
  menu.append(h("<hr>"));
  for (const [v, label] of Object.entries(XB)) add(label, v, isChanged(k) && cur === v);
  menu.append(h("<hr>"));
  add("Disabled", null, isChanged(k) && cur === null);
  document.body.append(menu);
  const r = chip.getBoundingClientRect();
  const mh = Math.min(360, menu.scrollHeight);
  menu.style.left = `${Math.min(innerWidth - 220, r.left)}px`;
  menu.style.top = `${Math.min(innerHeight - mh - 10, r.bottom + 6)}px`;
  menu.querySelector("button")?.focus();
  setTimeout(() => document.addEventListener("pointerdown", outside, true));
  menu.onkeydown = (e) => e.key === "Escape" && closeMenu();
}
