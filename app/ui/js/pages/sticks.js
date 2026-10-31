// Sticks & triggers: radial dead zone, response curve and trigger range, with a live preview
// of the raw stick position (grey) and what games receive (coloured).

import { profile, save, onSettings } from "../ipc.js";
import { h, seg, slider, toggle, panel } from "../ui.js";

const CURVES = [
  { value: "Linear", label: "Linear" },
  { value: "Smooth", label: "Smooth" },
  { value: "Aggressive", label: "Aggressive" },
];
let ctl = {}, viz = {}, bars = {};

function stickPanel(side, title) {
  const p = panel(title, "joystick");
  const v = h(`<div class="stickviz"><svg viewBox="-70 -70 140 140"><circle class="ring" r="64"/><circle class="dz" r="5"/><circle class="raw" r="5"/><circle class="out" r="6"/></svg><div class="ctl"></div></div>`);
  const box = v.querySelector(".ctl");
  ctl[`${side}dz`] = slider({ label: "Dead zone", min: 0, max: 30, value: profile()[side].deadzone_pct,
    onInput: (x) => save(() => (profile()[side].deadzone_pct = x)) });
  ctl[`${side}curve`] = seg(CURVES, profile()[side].curve, (c) => save(() => (profile()[side].curve = c)));
  box.append(ctl[`${side}dz`], h(`<div class="slabel" style="margin-top:16px"><span>Response curve</span></div>`), ctl[`${side}curve`]);
  p.append(v);
  viz[side] = { dz: v.querySelector(".dz"), raw: v.querySelector(".raw"), out: v.querySelector(".out") };
  return p;
}

export function init(el) {
  el.innerHTML = `<h1>Sticks &amp; triggers</h1><p class="lede">The dead zone ignores small movements around the centre; the curve shapes how fast the stick ramps up. Smooth gives fine aim near the centre, Aggressive reaches full speed sooner.</p>`;
  const grid = h(`<div class="grid2"></div>`);
  grid.append(stickPanel("left", "Left stick"), stickPanel("right", "Right stick"));

  const t = panel("Triggers", "swap_vert", `<p>Start ignores light resting pressure; End reaches full press before the trigger bottoms out.</p>`);
  const tr = profile().triggers;
  ctl.start = slider({ label: "Start", min: 0, max: 40, value: tr.start_pct, onInput: (x) => save(() => (profile().triggers.start_pct = Math.min(x, profile().triggers.end_pct - 5))) });
  ctl.end = slider({ label: "End", min: 60, max: 100, value: tr.end_pct, onInput: (x) => save(() => (profile().triggers.end_pct = Math.max(x, profile().triggers.start_pct + 5))) });
  ctl.hair = toggle({ label: "Hair trigger", desc: "Full press as soon as the trigger passes the start point. Great for shooters.", checked: tr.hair,
    onChange: (v) => save(() => (profile().triggers.hair = v)) });
  const tb = h(`<div class="trigbars"><div class="trigbar"><small>L2</small><div><u></u><i></i></div></div><div class="trigbar"><small>R2</small><div><u></u><i></i></div></div></div>`);
  bars.l = tb.querySelectorAll("i")[0];
  bars.r = tb.querySelectorAll("i")[1];
  bars.zones = tb.querySelectorAll("u");
  t.append(h(`<div class="grid2"></div>`));
  t.lastChild.append(ctl.start, ctl.end);
  t.append(ctl.hair, tb);

  el.append(grid, h(`<div style="height:16px"></div>`), t);
  onSettings(refresh);
  refresh();

  window.addEventListener("padstate", (e) => {
    if (!el.classList.contains("on")) return;
    const { state, out } = e.detail;
    const place = (c, x, y) => { c.setAttribute("cx", x * 64); c.setAttribute("cy", y * 64); };
    place(viz.left.raw, (state.lx - 127.5) / 127.5, (state.ly - 127.5) / 127.5);
    place(viz.left.out, out.lx / 32767, -out.ly / 32767);
    place(viz.right.raw, (state.rx - 127.5) / 127.5, (state.ry - 127.5) / 127.5);
    place(viz.right.out, out.rx / 32767, -out.ry / 32767);
    bars.l.style.transform = `scaleX(${out.lt / 255})`;
    bars.r.style.transform = `scaleX(${out.rt / 255})`;
  });
}

function refresh() {
  const p = profile();
  for (const side of ["left", "right"]) {
    ctl[`${side}dz`].set(p[side].deadzone_pct);
    ctl[`${side}curve`].set(p[side].curve);
    viz[side].dz.setAttribute("r", Math.max(2, (p[side].deadzone_pct / 100) * 64));
  }
  ctl.start.set(p.triggers.start_pct);
  ctl.end.set(p.triggers.end_pct);
  ctl.hair.set(p.triggers.hair);
  bars.zones.forEach((u) => { u.style.left = "0"; u.style.width = `${p.triggers.start_pct}%`; });
}
