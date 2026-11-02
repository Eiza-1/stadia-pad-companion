// Special buttons: Assistant → Gemini (app, else default browser), Capture → screenshot / recording
// with Game Bar, Steam, NVIDIA or AMD, Stadia → Guide (+ hold).

import { renderPad } from "../pad.js";
import { current } from "../theme.js";
import { settings, save, invoke, onSettings, appInfo } from "../ipc.js";
import { h, seg } from "../ui.js";

let segs = {};

const TOOLS = [
  { value: "Auto", label: "Auto" },
  { value: "GameBar", label: "Game Bar" },
  { value: "Steam", label: "Steam" },
  { value: "Nvidia", label: "NVIDIA" },
  { value: "Amd", label: "AMD" },
];

const USES = {
  GameBar: ["info", "Xbox Game Bar: screenshots and recordings go to Videos\\Captures."],
  Steam: ["info", "Steam, in games launched from Steam: F12 takes a screenshot. Recording needs Steam › Settings › Game Recording set to “Record on demand” (Ctrl+F11)."],
  Nvidia: ["info", "NVIDIA overlay: Alt+F1 takes a screenshot, Alt+F9 starts and stops recording. Keep the in-game overlay on with these default shortcuts."],
  Amd: ["info", "AMD Adrenalin: Ctrl+Shift+S takes a screenshot, Ctrl+Shift+E starts and stops recording."],
  Windows: ["warning", "No recorder found, so taps save a full-screen screenshot to Pictures\\Screenshots and recording won't work. Turn on Settings › Gaming › Captures, or start Steam, the NVIDIA overlay or AMD Adrenalin."],
};
const NAMES = { GameBar: "Game Bar", Steam: "Steam", Nvidia: "NVIDIA", Amd: "AMD" };
let toolNote, asked = 0;

export function init(el) {
  el.innerHTML = `<h1>Special buttons</h1><p class="lede">The Assistant and Capture buttons have real jobs here. They never reach the game, so they can't trigger anything by accident.</p>`;
  const pad = h(`<svg class="minipad"></svg>`);
  el.append(pad);
  renderPad(pad, current);

  const grid = h(`<div class="grid3"></div>`);
  const card = (icon, title, text, key, options) => {
    const p = h(`<section class="panel special"><div class="icon">${icon}</div><h3>${title}</h3><p>${text}</p></section>`);
    segs[key] = seg(options, settings[key], (v) => save((s) => (s[key] = v)));
    p.append(segs[key]);
    grid.append(p);
    return p;
  };
  const gemini = card(`<img src="assets/gemini-light.svg" alt="Gemini">`, "Assistant button",
    "Opens your Gemini app, or Gemini in your default browser if the app isn't installed. Brings it to the front if it's already open.",
    "assistant", [{ value: "Gemini", label: "Gemini" }, { value: "Off", label: "Off" }]);
  const capture = card(`<span class="ms">screenshot_monitor</span>`, "Capture button",
    "Tap for an instant screenshot. Hold for half a second to start recording; hold again to stop.",
    "capture", [{ value: "TapHold", label: "Tap + hold" }, { value: "Off", label: "Off" }]);
  card(`<img src="assets/stadia.svg" alt="Stadia" style="height:30px">`, "Stadia button",
    "Works as the Xbox Guide button in games. Hold it for 1 s to open Pad Companion over any game.",
    "stadia", [{ value: "GuideHold", label: "Guide + hold" }, { value: "GuideOnly", label: "Guide only" }]);
  el.append(grid);

  appInfo().then((info) => {
    const text = info.gemini_app ? "Uses your installed Gemini app." : "No Gemini app found, so Gemini opens in your default browser.";
    gemini.append(h(`<div class="note-box info"><span class="ms">${info.gemini_app ? "apps" : "public"}</span><span>${text}</span></div>`));
  });

  // Capture with: Automatic follows Game Bar, then Steam (in Steam games), NVIDIA, then AMD.
  segs.capture_tool = seg(TOOLS, settings.capture_tool, (v) => save((s) => (s.capture_tool = v)));
  segs.capture_tool.classList.add("compact");
  toolNote = h(`<div class="note-box info"><span class="ms">info</span><span></span></div>`);
  capture.append(h(`<div class="sublabel">Capture with</div>`), segs.capture_tool, toolNote);
  refreshCapture();
  window.addEventListener("focus", refreshCapture); // a recorder may have been started meanwhile

  onSettings((s) => {
    for (const k in segs) segs[k].set(s[k]);
    refreshCapture();
  });
}

/** Says which recorder the button will drive now, and what it needs. */
function refreshCapture() {
  if (!toolNote) return;
  const want = settings.capture_tool, n = ++asked;
  invoke("capture_info", { tool: want }).then((c) => {
    if (n !== asked || !c) return;
    const [icon, text] = USES[c.uses];
    const fell = want !== "Auto" && c.uses !== want
      ? `${NAMES[want]} isn't available right now, so it uses ${c.uses === "Windows" ? "a Windows screenshot" : NAMES[c.uses]} instead. `
      : "";
    toolNote.className = `note-box${icon === "info" ? " info" : ""}`;
    toolNote.querySelector(".ms").textContent = icon;
    toolNote.querySelector("span:last-child").textContent = fell + text;
  });
}
