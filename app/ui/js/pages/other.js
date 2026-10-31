// Other settings: start with Windows, hide pad from games, logs, version, drivers.

import { settings, status, save, onSettings, onStatus, invoke } from "../ipc.js";
import { h, toggle, panel } from "../ui.js";

let start, hide, kv;

export function init(el) {
  el.innerHTML = `<h1>Other settings</h1><p class="lede">Pad Companion keeps running in the tray when you close this window, so the controller keeps working. Quit it from the tray icon.</p>`;
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
  grid.append(p, about);
  el.append(grid);

  invoke("app_info").then((i) => { kv.dataset.version = i.version; renderAbout(); });
  onSettings(() => { start.set(settings.start_with_windows); hide.set(settings.hide_from_games); });
  onStatus(renderAbout);
  renderAbout();
}

function renderAbout() {
  const s = status || {};
  const row = (k, v) => `<dt>${k}</dt><dd>${v}</dd>`;
  kv.innerHTML = row("Version", kv.dataset.version || "…") +
    row("ViGEmBus", s.vigem_ok ? "Installed" : "Missing") +
    row("HidHide", s.hidhide_ok ? "Installed" : "Not installed") +
    row("Connection", s.connection === "Usb" ? "USB cable" : s.connection === "Bluetooth" ? "Bluetooth" : "Not connected");
}
