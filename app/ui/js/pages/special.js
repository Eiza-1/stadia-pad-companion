// Special buttons: Assistant → Gemini, Capture → screenshot / recording, Stadia → Guide (+ hold).

import { renderPad } from "../pad.js";
import { SCHEMES, current } from "../theme.js";
import { settings, save, invoke, onSettings } from "../ipc.js";
import { h, seg } from "../ui.js";

let segs = {};

export function init(el) {
  el.innerHTML = `<h1>Special buttons</h1><p class="lede">The Assistant and Capture buttons have real jobs here. They never reach the game, so they can't trigger anything by accident.</p>`;
  const pad = h(`<svg class="minipad"></svg>`);
  el.append(pad);
  renderPad(pad, SCHEMES[current].pad);

  const grid = h(`<div class="grid3"></div>`);
  const card = (icon, title, text, key, options) => {
    const p = h(`<section class="panel special"><div class="icon">${icon}</div><h3>${title}</h3><p>${text}</p></section>`);
    segs[key] = seg(options, settings[key], (v) => save((s) => (s[key] = v)));
    p.append(segs[key]);
    grid.append(p);
    return p;
  };
  card(`<img src="assets/gemini-light.svg" alt="Gemini">`, "Assistant button",
    "Opens Gemini in a compact window, or brings it to the front if it's already open.",
    "assistant", [{ value: "Gemini", label: "Gemini" }, { value: "Off", label: "Off" }]);
  const capture = card(`<span class="ms">screenshot_monitor</span>`, "Capture button",
    "Tap for an instant screenshot. Hold for half a second to start recording; hold again to stop.",
    "capture", [{ value: "TapHold", label: "Tap + hold" }, { value: "Off", label: "Off" }]);
  card(`<img src="assets/stadia.svg" alt="Stadia" style="height:30px">`, "Stadia button",
    "Works as the Xbox Guide button in games. Hold it for 1 s to open Pad Companion over any game.",
    "stadia", [{ value: "GuideHold", label: "Guide + hold" }, { value: "GuideOnly", label: "Guide only" }]);
  el.append(grid);

  invoke("app_info").then((info) => {
    const where = info.game_bar_capture
      ? "Screenshots and recordings are saved by Xbox Game Bar to Videos\\Captures."
      : "Game Bar captures are turned off, so taps save a full-screen screenshot to Pictures\\Screenshots and recording won't work. Turn on Settings › Gaming › Captures to capture just the game and record.";
    capture.append(h(`<div class="note-box ${info.game_bar_capture ? "info" : ""}"><span class="ms">${info.game_bar_capture ? "info" : "warning"}</span><span>${where}</span></div>`));
  });

  onSettings((s) => { for (const k in segs) segs[k].set(s[k]); });
}
