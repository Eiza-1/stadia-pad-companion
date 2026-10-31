// Performance: polling rate (honest about what the hardware allows), Basic / Competitive,
// Force Bluetooth Low-Latency Mode, and live measurements from the bridge.

import { settings, status, save, onSettings, onStatus } from "../ipc.js";
import { h, seg, toggle, panel } from "../ui.js";

const RATES = [125, 250, 500, 1000];
let rateSeg, rateNote, modes, ll, statEls = {}, spark, history = [];

export function init(el) {
  el.innerHTML = `<h1>Performance</h1><p class="lede">Pad Companion forwards every report the moment it arrives, with several reads always queued so bursts are never dropped. These settings tune the rest.</p>`;
  const grid = h(`<div class="grid2"></div>`);

  const rate = panel("Polling rate", "sensors");
  rateSeg = seg([], settings.polling_hz, () => {});
  rateSeg.classList.add("tall");
  rate.append(rateSeg, h(`<div class="scale"><span>Daily use</span><span>Competitive</span></div>`));
  rateNote = h(`<div class="note-box info"><span class="ms">info</span><span></span></div>`);
  rate.append(rateNote);

  const mode = panel("Pad mode", "bolt");
  modes = h(`<div class="modes"></div>`);
  const modeCard = (value, title, icon, text) => {
    const b = h(`<button class="mode" data-m="${value}"><b>${title}<span class="ms">${icon}</span></b><span class="d">${text}</span></button>`);
    b.onclick = () => save((s) => (s.pad_mode = value));
    modes.append(b);
  };
  modeCard("Basic", "Basic", "eco", "Low latency tuned for everyday play. Light smoothing hides tiny stick jitter; Windows power management is left alone.");
  modeCard("Competitive", "Competitive", "rocket_launch", "No smoothing, input thread at game priority with a 1 ms system timer while a pad is connected. Uses a little more PC power; pad battery use stays about the same.");
  ll = toggle({
    label: "Force Bluetooth Low-Latency Mode",
    desc: "Asks Windows for the shortest Bluetooth connection interval with no skipped slots, held while you play.",
    checked: settings.ble_low_latency,
    onChange: (v) => save((s) => (s.ble_low_latency = v)),
  });
  mode.append(modes, ll);
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
  const bt = status?.connection === "Bluetooth";
  const driver = !!status?.rate_override_available;
  const options = RATES.map((r) => ({
    value: r, label: `${r} Hz`, exp: r >= 500,
    disabled: r > 125 && (bt || !driver),
    title: r > 125 && bt ? "Bluetooth is limited by the radio link (≈7.5 ms)" : r > 125 && !driver ? "Needs a USB polling-rate driver" : "",
  }));
  const fresh = seg(options, settings.polling_hz, (v) => save((s) => (s.polling_hz = v)));
  fresh.classList.add("tall");
  rateSeg.replaceWith(fresh);
  rateSeg = fresh;

  const text = rateNote.querySelector("span:last-child");
  rateNote.classList.toggle("info", settings.polling_hz < 500);
  if (bt) text.textContent = "Over Bluetooth the pad reports at its native rate (about 125 Hz); faster rates need the USB cable.";
  else if (!driver) text.textContent = "125 Hz is the pad's native USB rate. Faster rates need a USB polling-rate override driver (hidusbf); when it's installed these options unlock and the live meter shows what you actually get.";
  else if (settings.polling_hz >= 500) text.textContent = "Experimental: 500 and 1000 Hz can cause stutter or input lag on some PCs. Set the rate in hidusbf; the live meter below shows the rate actually reached.";
  else text.textContent = "Set the rate in hidusbf; the live meter below shows the rate actually reached.";

  modes.querySelectorAll(".mode").forEach((m) => m.classList.toggle("on", m.dataset.m === settings.pad_mode));
  const state = status?.ble_low_latency;
  const usable = settings.pad_mode === "Competitive";
  ll.set(settings.ble_low_latency, state === "Unsupported" || !usable,
    state === "Unsupported" ? "Needs Windows 11." : !usable ? "Part of Competitive mode." : !bt ? "Applies when the pad is connected over Bluetooth." : state === "Active" ? "" : "Windows didn't grant the request this time; it is retried on reconnect.");
  ll.querySelector("small").textContent = state === "Active"
    ? "Active: Windows is holding the shortest connection interval for the pad."
    : "Asks Windows for the shortest Bluetooth connection interval with no skipped slots, held while you play.";
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
