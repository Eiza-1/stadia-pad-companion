// Other settings: start with Windows, hide pad from games, Gemini (key, answer voice, overlay position),
// logs, version, drivers.

import { settings, status, save, onSettings, onStatus, invoke, appInfo } from "../ipc.js";
import { h, toggle, panel, seg } from "../ui.js";

let start, hide, kv, voiceSeg;

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
  onSettings(() => { start.set(settings.start_with_windows); hide.set(settings.hide_from_games); voiceSeg.set(settings.answer_voice); });
  onStatus(renderAbout);
  renderAbout();
}

/** Your Gemini key (saved only on this PC), how answers are given, and the overlay's position. */
function geminiPanel() {
  const g = panel("Gemini", "auto_awesome");
  const keyRow = h(`<div class="srow"><span class="stext"><b>Your Gemini key</b><small></small></span><span class="keybtns"></span></div>`);
  const form = h(`<div class="keyform" hidden><input type="password" placeholder="Paste your key (AIza…)" autocomplete="off" spellcheck="false"><button class="btn tonal">Save</button><button class="btn text">Get a free key</button></div>`);
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

  const posRow = h(`<div class="srow"><span class="stext"><b>Overlay position</b><small>Drag the pill by its handle to move it. This puts it back at the bottom centre.</small></span><button class="btn text">Reset</button></div>`);
  posRow.querySelector("button").onclick = () => invoke("overlay_reset_pos").then(() => say("The overlay opens at the bottom centre again."));

  g.append(keyRow, form, msg, shortcutRow(say), voiceRow, posRow);
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
