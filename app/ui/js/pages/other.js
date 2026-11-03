// Other settings: start with Windows, hide pad from games, Gemini (key, answer voice, overlay position),
// logs, version, drivers.

import { settings, status, save, onSettings, onStatus, invoke, appInfo } from "../ipc.js";
import { h, toggle, panel, seg, slider } from "../ui.js";

let start, hide, kv, voiceSeg, voicePick, sizeSlider, sizeSeg, animSeg, listenSeg, glassSeg, dimSeg;
const SIZES = [{ value: 82, label: "Compact" }, { value: 100, label: "Standard" }, { value: 120, label: "Large" }];
const pct = () => Math.round((settings.overlay_scale ?? 1) * 100);
const quick = (p) => (SIZES.some((o) => o.value === p) ? p : null);

export function init(el) {
  el.innerHTML = `<h1>Other settings</h1><p class="lede">Pad Companion keeps running in the tray when you close this window, so the controller keeps working. To quit, right-click the tray icon and choose Quit.</p>`;
  const p = panel("General", "tune");
  start = toggle({ label: "Start with Windows", desc: "Starts quietly in the tray when you sign in.", checked: settings.start_with_windows,
    onChange: (v) => save((s) => (s.start_with_windows = v)) });
  hide = toggle({ label: "Hide the real pad from games", desc: "Games and launchers only see the Xbox controller, so you never get double input. Turn off to let apps see the Stadia pad directly.", checked: settings.hide_from_games,
    onChange: (v) => save((s) => (s.hide_from_games = v)) });
  p.append(start, hide);

  const about = panel("About", "info");
  kv = h(`<dl class="kv"></dl>`);
  const logs = h(`<button class="btn tonal" style="margin-top:16px"><span class="ms">folder_open</span>Open log folder</button>`);
  logs.onclick = () => invoke("open_log_folder");
  about.append(kv, logs);

  const grid = h(`<div class="grid2"></div>`);
  grid.append(p, about, geminiPanel());
  el.append(grid);

  appInfo().then((i) => { kv.dataset.version = i.version; renderAbout(); });
  onSettings(() => {
    start.set(settings.start_with_windows);
    hide.set(settings.hide_from_games);
    voiceSeg.set(settings.answer_voice);
    voicePick?.set(settings.voice);
    sizeSlider.set(pct());
    sizeSeg.set(quick(pct()));
    animSeg.set(settings.overlay_anim || "Glow");
    listenSeg.set(settings.live_listen || "Speakers");
    glassSeg.set(settings.live_glass || "Frosted");
    dimSeg.set(settings.live_dim || "EasesOff");
  });
  onStatus(renderAbout);
  renderAbout();
}

/** Your Gemini key (saved only on this PC), how answers are given, the pill's size, entrance and position. */
function geminiPanel() {
  const g = panel("Gemini", "auto_awesome");
  const keyRow = h(`<div class="srow"><span class="stext"><b>Your Gemini key</b><small></small></span><span class="keybtns"></span></div>`);
  const form = h(`<div class="keyform" hidden><input type="password" placeholder="Paste your key" autocomplete="off" spellcheck="false"><button class="btn tonal">Save</button><button class="btn text">Get a free key</button></div>`);
  const msg = h(`<div class="note-box info" hidden><span class="ms">info</span><span></span></div>`);
  const say = (text, err = false) => {
    msg.hidden = !text;
    msg.className = `note-box${err ? "" : " info"}`;
    msg.querySelector(".ms").textContent = err ? "error" : "info";
    msg.querySelector("span:last-child").textContent = text;
  };
  const render = async () => {
    const ok = await invoke("gemini_key_status");
    keyRow.querySelector("small").textContent = ok
      ? "Saved on this PC. Gemini answers in the overlay use it."
      : "Not set. Get a free key from Google AI Studio; it's saved only on this PC.";
    const btns = keyRow.querySelector(".keybtns");
    btns.innerHTML = "";
    const add = h(`<button class="btn tonal">${ok ? "Change" : "Add key"}</button>`);
    add.onclick = () => { form.hidden = false; form.querySelector("input").focus(); };
    btns.append(add);
    if (ok) {
      const rm = h(`<button class="btn text">Remove</button>`);
      rm.onclick = async () => { await invoke("gemini_key_clear"); say("Key removed from this PC."); render(); };
      btns.append(rm);
    }
  };
  const [input, saveBtn, studio] = form.children;
  studio.onclick = () => invoke("gemini_open_studio");
  saveBtn.onclick = async () => {
    say("Checking your key…");
    try {
      await invoke("gemini_key_set", { key: input.value });
      input.value = "";
      form.hidden = true;
      say("Key saved.");
      render();
    } catch (e) {
      say(String(e), true);
    }
  };
  input.addEventListener("keydown", (e) => e.key === "Enter" && saveBtn.onclick());

  const voiceRow = h(`<div class="srow col"><span class="stext"><b>Answers</b><small>Shown as text, read aloud, or both.</small></span></div>`);
  voiceSeg = seg([{ value: "Text", label: "Text" }, { value: "Voice", label: "Voice" }, { value: "Both", label: "Both" }],
    settings.answer_voice, (v) => save((s) => (s.answer_voice = v)));
  voiceRow.append(voiceSeg);

  // Gemini's own voices read answers aloud (Voice / Both); Play tries one with a short sample.
  const speakRow = h(`<div class="srow col"><span class="stext"><b>Voice</b><small>Gemini's own natural voices read the answers. Each read-aloud uses your free key, so a busy day can run out.</small></span></div>`);
  const sample = h(`<button class="btn tonal" style="margin-top:12px"><span class="ms">play_arrow</span>Play a sample</button>`);
  sample.onclick = async () => {
    sample.disabled = true;
    say("Getting a sample from Gemini…");
    try { await invoke("gemini_voice_sample", { voice: settings.voice || "Aoede" }); say(""); }
    catch (e) { say(String(e), true); }
    sample.disabled = false;
  };
  invoke("gemini_voices").then((list) => {
    voicePick = seg(list.map(([v, d]) => ({ value: v, label: v, title: d })), settings.voice || "Aoede", (v) => save((s) => (s.voice = v)));
    voicePick.classList.add("wrap", "compact");
    speakRow.append(voicePick, sample);
  });

  // Gemini Live: how the mic behaves, how see-through the panel is, and the screen dim behind it.
  const liveRow = (title, desc, options, value, key) => {
    const row = h(`<div class="srow col"><span class="stext"><b></b><small></small></span></div>`);
    row.querySelector("b").textContent = title;
    row.querySelector("small").textContent = desc;
    const sg = seg(options, value, (v) => save((s) => (s[key] = v)));
    row.append(sg);
    return [row, sg];
  };
  let listenRow, glassRow, dimRow;
  [listenRow, listenSeg] = liveRow("Live listening", "Speakers: Gemini can't hear itself, hold Assistant to talk over it. Headphones: talk any time, like a call.",
    [{ value: "Speakers", label: "Speakers" }, { value: "Headphones", label: "Headphones" }], settings.live_listen || "Speakers", "live_listen");
  [glassRow, glassSeg] = liveRow("Live panel", "How see-through the Live panel is.",
    [{ value: "Light", label: "Light glass" }, { value: "Frosted", label: "Frosted glass" }, { value: "Solid", label: "Nearly solid" }], settings.live_glass || "Frosted", "live_glass");
  [dimRow, dimSeg] = liveRow("Live dim", "Eases off dims the screen as Live opens, then lightens it after 2 s so you can keep playing.",
    [{ value: "EasesOff", label: "Eases off" }, { value: "StaysDark", label: "Stays dark" }], settings.live_dim || "EasesOff", "live_dim");

  // Pill size: an open overlay resizes as the slider moves (set_settings tells it).
  const sizeRow = h(`<div class="srow col"><span class="stext"><b>Pill size</b><small>Standard is 600 × 64 px on a 1080p screen. The keyboard and answers follow it.</small></span></div>`);
  const setSize = (p) => save((s) => (s.overlay_scale = p / 100));
  sizeSlider = slider({ label: "Size", min: 70, max: 140, step: 5, value: pct(), onInput: (p) => { sizeSeg.set(quick(p)); setSize(p); } });
  sizeSeg = seg(SIZES, quick(pct()), (p) => { sizeSlider.set(p); setSize(p); });
  sizeRow.append(sizeSlider, sizeSeg);

  const animRow = h(`<div class="srow col"><span class="stext"><b>Entrance animation</b><small>How the pill appears. Simple fade or Off suits slower PCs, or if the glow distracts you mid-game.</small></span></div>`);
  animSeg = seg([{ value: "Glow", label: "Glow" }, { value: "Fade", label: "Simple fade" }, { value: "Off", label: "Off" }],
    settings.overlay_anim || "Glow", (v) => save((s) => (s.overlay_anim = v)));
  animRow.append(animSeg);

  const posRow = h(`<div class="srow"><span class="stext"><b>Overlay position</b><small>Drag the pill by its handle to move it. This puts it back at the bottom centre.</small></span><button class="btn text">Reset</button></div>`);
  posRow.querySelector("button").onclick = () => invoke("overlay_reset_pos").then(() => say("The overlay opens at the bottom centre again."));

  g.append(keyRow, form, msg, shortcutRow(say), voiceRow, speakRow, listenRow, glassRow, dimRow, sizeRow, animRow, posRow);
  render();
  return g;
}

/** Keyboard shortcut for the overlay: "Change" listens for the next key combination (Esc cancels). */
function shortcutRow(say) {
  const row = h(`<div class="srow"><span class="stext"><b>Keyboard shortcut</b><small></small></span><span class="keybtns"></span></div>`);
  const show = async () => {
    const k = settings.overlay_hotkey;
    row.querySelector("small").textContent = k
      ? `${await invoke("hotkey_label", { hotkey: k })}: tap to type, hold to talk. Works without the pad.`
      : "Off. Set one to open the overlay from the keyboard.";
    const btns = row.querySelector(".keybtns");
    btns.innerHTML = "";
    const change = h(`<button class="btn tonal">${k ? "Change" : "Set"}</button>`);
    change.onclick = listenForKeys;
    btns.append(change);
    if (k) {
      const off = h(`<button class="btn text">Off</button>`);
      off.onclick = () => apply(null);
      btns.append(off);
    }
  };
  const apply = async (hotkey) => {
    try {
      await invoke("set_overlay_hotkey", { hotkey });
      settings.overlay_hotkey = hotkey;
      say(hotkey ? `Shortcut set: ${await invoke("hotkey_label", { hotkey })}` : "Keyboard shortcut off.");
    } catch (e) {
      say(String(e), true);
    }
    show();
  };
  const MODS = ["Control", "Alt", "Shift", "Meta", "OS"];
  function listenForKeys() {
    row.querySelector("small").textContent = "Press the keys you want (Esc to cancel)…";
    const onKey = (e) => {
      e.preventDefault();
      e.stopPropagation();
      if (MODS.includes(e.key)) return;
      document.removeEventListener("keydown", onKey, true);
      if (e.key === "Escape") return show();
      apply({ ctrl: e.ctrlKey, alt: e.altKey, shift: e.shiftKey, win: e.metaKey, vk: e.keyCode });
    };
    document.addEventListener("keydown", onKey, true);
  }
  show();
  return row;
}

function renderAbout() {
  const s = status || {};
  const row = (k, v) => `<dt>${k}</dt><dd>${v}</dd>`;
  kv.innerHTML = row("Version", kv.dataset.version || "…") +
    row("ViGEmBus", s.vigem_ok ? "Installed" : "Missing") +
    row("HidHide", s.hidhide_ok ? "Installed" : "Not installed") +
    row("Connection", s.connection === "Usb" ? "USB cable" : s.connection === "Bluetooth" ? "Bluetooth" : "Not connected");
}
