// Tray right-click menu (tray.html): pad photo, name, connection and battery ring, then actions.
// The window is created on right-click and destroyed when it loses focus (tray.rs), so nothing here
// runs while you play.

import { SCHEMES } from "./theme.js";

const tauri = window.__TAURI__;
const invoke = (cmd, args) => tauri.core.invoke(cmd, args);
const $ = (id) => document.getElementById(id);
const CIRC = 2 * Math.PI * 19;

function scheme(color) {
  const p = SCHEMES[color] ? color : "JustBlack";
  const s = SCHEMES[p];
  const root = document.documentElement.style;
  const map = { "--primary": s.pri, "--on-primary": s.onp, "--sc-low": s.l1, "--sc-high": s.l3, "--sc-highest": s.l4,
    "--on-surface": s.on, "--on-sv": s.sv, "--outline": s.ol };
  for (const [k, v] of Object.entries(map)) root.setProperty(k, v);
  $("pad").src = `assets/pads/${p}.png`;
  return p;
}

function status(s) {
  const conn = s.connection;
  $("conn").textContent = !s.vigem_ok ? "ViGEmBus driver missing"
    : conn === "Bluetooth" ? "Bluetooth"
    : conn === "Usb" ? (s.charging ? "USB cable · Charging" : "USB cable")
    : "Not connected";
  const ring = $("ring"), pct = $("pct");
  const b = conn ? s.battery : null;
  ring.classList.toggle("low", b != null && b <= 20);
  ring.classList.toggle("off", !conn);
  if (b != null) {
    $("arc").style.strokeDashoffset = CIRC * (1 - b / 100);
    pct.textContent = `${b}%`;
    ring.title = `Battery ${b}%`;
  } else {
    // No reading over USB (it's charging) or while disconnected: an icon instead of a number.
    $("arc").style.strokeDashoffset = conn ? 0 : CIRC;
    pct.innerHTML = `<span class="ms">${!conn ? "link_off" : s.charging ? "bolt" : "usb"}</span>`;
    ring.title = !conn ? "Not connected" : s.charging ? "Charging" : "USB cable";
  }
}

async function boot() {
  const [settings, st] = await Promise.all([invoke("get_settings"), invoke("get_status")]);
  scheme(settings.pad_color);
  status(st);
  const auto = $("auto");
  auto.setAttribute("aria-checked", String(!!settings.start_with_windows));
  tauri.event.listen("status", (e) => status(e.payload));

  document.querySelectorAll("[data-a]").forEach((b) => (b.onclick = () => invoke("tray_action", { action: b.dataset.a })));
  auto.onclick = () => {
    const on = auto.getAttribute("aria-checked") !== "true";
    auto.setAttribute("aria-checked", String(on));
    invoke("tray_set_autostart", { on }).catch(() => auto.setAttribute("aria-checked", String(!on)));
  };

  // Keyboard: arrows move through the items, Enter/Space activate (native buttons), Esc closes.
  const items = [...document.querySelectorAll("button")];
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape") return invoke("tray_action", { action: "close" });
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
    e.preventDefault();
    const i = items.indexOf(document.activeElement);
    const n = e.key === "ArrowDown" ? (i + 1) % items.length : (i - 1 + items.length) % items.length;
    items[n].focus();
  });
  document.addEventListener("contextmenu", (e) => e.preventDefault());

  // Show once the photo is decoded, so the menu never appears half drawn.
  await $("pad").decode().catch(() => {});
  await invoke("tray_ready");
  requestAnimationFrame(() => document.body.classList.add("in"));
}

boot().catch(() => invoke("tray_ready"));
