// Performance: what the pad's link really delivers (measured, per connection), Basic / Competitive,
// and live measurements from the bridge.

import { settings, status, save, onSettings, onStatus } from "../ipc.js";
import { h, panel } from "../ui.js";

let links, linkNote, modes, statEls = {}, spark, history = [];

export function init(el) {
  el.innerHTML = `<h1>Performance</h1><p class="lede">Pad Companion forwards every report the moment it arrives, with several reads always queued so bursts are never dropped. These settings tune the rest.</p>`;
  const grid = h(`<div class="grid2"></div>`);

  // Measured on the pad (bridge_cli --bench): its firmware sends 125 reports/s on both links. Windows
  // already polls USB every 4 ms, so no polling driver can raise it; Bluetooth bunches reports per link.
  const rate = panel("Report rate", "sensors");
  links = h(`<div class="links">
    <div class="link" data-c="Usb"><span class="ms">usb</span><span class="t"><b>USB cable</b><small>A report every 8 ms, steady</small></span><b class="v">125<em>Hz</em></b></div>
    <div class="link" data-c="Bluetooth"><span class="ms">bluetooth</span><span class="t"><b>Bluetooth</b><small></small></span><b class="v">125<em>Hz</em></b></div>
  </div>`);
  linkNote = h(`<div class="note-box info"><span class="ms">info</span><span></span></div>`);
  rate.append(links, linkNote);

  const mode = panel("Pad mode", "bolt");
  modes = h(`<div class="modes"></div>`);
  const modeCard = (value, title, icon, text) => {
    const b = h(`<button class="mode" data-m="${value}"><b>${title}<span class="ms">${icon}</span></b><span class="d">${text}</span></button>`);
    b.onclick = () => save((s) => (s.pad_mode = value));
    modes.append(b);
  };
  modeCard("Basic", "Basic", "eco", "Low latency tuned for everyday play. Light smoothing hides tiny stick jitter; Windows power management is left alone.");
  modeCard("Competitive", "Competitive", "rocket_launch", "No smoothing, input thread at game priority with a 1 ms system timer while a pad is connected. Uses a little more PC power; pad battery use stays about the same.");
  mode.append(modes);
  grid.append(rate, mode);

  const live = panel("Live input", "monitoring", `<p>Measured on the last 1024 reports. Move the sticks to see it update.</p>`);
  const stats = h(`<div class="stats"></div>`);
  for (const [k, label, unit] of [["rate", "Report rate", "Hz"], ["p50", "Typical interval", "ms"], ["p99", "Worst 1%", "ms"], ["proc", "Added by app (p99)", "µs"]]) {
    const s = h(`<div class="stat"><small>${label}</small><b>–<em>${unit}</em></b></div>`);
    statEls[k] = s.querySelector("b");
    statEls[k].unit = unit;
    stats.append(s);
  }
  spark = h(`<svg class="spark" viewBox="0 0 300 56" preserveAspectRatio="none"><path class="area"/><path class="line"/></svg>`);
  live.append(stats, spark);
  el.append(grid, h(`<div style="height:16px"></div>`), live);

  window.addEventListener("padstats", (e) => showStats(e.detail));
  onSettings(refresh);
  onStatus(refresh);
  refresh();
}

function refresh() {
  const conn = status?.connection;
  const link = status?.link_ms;
  links.querySelectorAll(".link").forEach((l) => l.classList.toggle("on", l.dataset.c === conn));
  links.querySelector('[data-c="Bluetooth"] small').textContent = conn === "Bluetooth" && link
    ? `Arrives in small bursts over a ${+link.toFixed(2)} ms link`
    : "Arrives in small bursts over a 10 ms link";
  const text = linkNote.querySelector("span:last-child");
  text.textContent = conn === "Bluetooth"
    ? "For the lowest latency, plug in the cable. Over Bluetooth reports come in bursts, so an input can wait a few slots. Windows’ own link is already its fastest for this pad, so Pad Companion leaves it alone."
    : conn === "Usb"
      ? "This is the pad's own limit: Windows already asks for a report every 4 ms, so a faster polling setting or driver can't add any."
      : "Connect the pad to see the live numbers below.";

  modes.querySelectorAll(".mode").forEach((m) => m.classList.toggle("on", m.dataset.m === settings.pad_mode));
}

function showStats(s) {
  const fmt = (v, d = 1) => (s.samples > 1 ? v.toFixed(d) : "–");
  const set = (k, v) => { statEls[k].firstChild.textContent = v; };
  set("rate", fmt(s.rate_hz, 0));
  set("p50", fmt(s.interval_p50_ms, 1));
  set("p99", fmt(s.interval_p99_ms, 1));
  set("proc", s.samples > 1 ? String(s.proc_p99_us) : "–");
  history.push(s.samples > 1 ? s.rate_hz : 0);
  if (history.length > 60) history.shift();
  const max = Math.max(150, ...history);
  const last = Math.max(1, history.length - 1);
  const pts = history.map((v, i) => `${(i / last) * 300},${56 - (v / max) * 50}`);
  spark.querySelector(".line").setAttribute("d", pts.length ? `M${pts.join(" L")}` : "");
  spark.querySelector(".area").setAttribute("d", pts.length ? `M0,56 L${pts.join(" L")} L300,56 Z` : "");
}
