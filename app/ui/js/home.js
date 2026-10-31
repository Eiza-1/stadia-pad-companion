// Home: the device card, live connection/battery pills, palette with colour diffusion, driver banners.

import { renderPad, colorPad, applyPadState } from "./pad.js";
import { SCHEMES, applyScheme, current } from "./theme.js";
import { settings, save, invoke, onStatus } from "./ipc.js";

const $ = (id) => document.getElementById(id);
const BLE_HINT = "Plug in the cable, or hold Stadia + Y for 2 s to pair over Bluetooth.";

export function initHome(goInside) {
  const cur = document.querySelector("#pw .cur");
  const next = document.querySelector("#pw .next");
  renderPad(cur, SCHEMES[settings.pad_color]?.pad);
  renderPad(next);

  // Palette: hovering a swatch diffuses its colour into the pad from the cursor; it is saved too.
  const pal = $("pal");
  pal.innerHTML = Object.entries(SCHEMES)
    .map(([k, s]) => `<button class="sw" role="option" data-c="${k}" aria-label="${s.name}"><i style="background:${s.swatch}"></i><span>${s.name}</span></button>`)
    .join("");
  let pending, palT;
  const mark = () => pal.querySelectorAll(".sw").forEach((s) => s.classList.toggle("on", s.dataset.c === current));
  const diffuse = (c, ev) => {
    if (c === current) return;
    const r = $("pw").getBoundingClientRect();
    next.style.setProperty("--mx", ev?.clientX ? `${((ev.clientX - r.left) / r.width) * 100}%` : "50%");
    next.style.setProperty("--my", "55%");
    colorPad(next, SCHEMES[c].pad);
    next.classList.remove("go");
    void next.getBoundingClientRect();
    next.classList.add("go");
    applyScheme(c);
    clearTimeout(pending);
    pending = setTimeout(() => { colorPad(cur, SCHEMES[c].pad); next.classList.remove("go"); }, 900);
    mark();
    save((s) => (s.pad_color = c));
  };
  const open = () => { clearTimeout(palT); pal.classList.add("open"); };
  const close = () => { palT = setTimeout(() => pal.classList.remove("open"), 350); };
  for (const el of [pal, $("palBtn")]) { el.onmouseenter = open; el.onmouseleave = close; }
  $("palBtn").onclick = () => pal.classList.toggle("open");
  pal.querySelectorAll(".sw").forEach((s) => {
    s.onmouseenter = (e) => diffuse(s.dataset.c, e);
    s.onfocus = () => diffuse(s.dataset.c);
    s.onclick = (e) => { diffuse(s.dataset.c, e); pal.classList.remove("open"); };
  });
  mark();

  $("stage").onclick = () => goInside(true);
  $("stage").onkeydown = (e) => (e.key === "Enter" || e.key === " ") && goInside(true);

  onStatus(renderStatus);
}

function battIcon(pct) {
  const bars = Math.max(0, Math.min(6, Math.round((pct / 100) * 6)));
  return pct >= 95 ? "battery_full" : `battery_${bars}_bar`;
}

/** Connection pill, battery chip, service chip and banners, from the bridge Status. */
export function renderStatus(s) {
  const conn = s.connection;
  const icon = conn === "Bluetooth" ? "bluetooth" : conn === "Usb" ? "usb" : "link_off";
  const text = conn === "Bluetooth" ? "Bluetooth" : conn === "Usb" ? "USB cable" : "Not connected";
  $("connPill").classList.toggle("live", !!conn);
  $("connIco").textContent = $("dhConnIco").textContent = icon;
  $("connTxt").textContent = text;
  $("connHint").hidden = !!conn;
  $("connHint").textContent = BLE_HINT;
  if (!conn) {
    const neutral = { buttons: "", lx: 128, ly: 128, rx: 128, ry: 128, l2: 0, r2: 0 };
    document.querySelectorAll("svg.pad").forEach((p) => applyPadState(p, neutral));
  }

  const batt = $("battPill");
  if (s.charging) {
    batt.hidden = false;
    $("battIco").textContent = "battery_charging_full";
    $("battTxt").textContent = "Charging";
  } else if (s.battery != null) {
    batt.hidden = false;
    $("battIco").textContent = battIcon(s.battery);
    $("battTxt").textContent = `${s.battery}%`;
  } else {
    batt.hidden = true;
  }
  $("dhConn").textContent = conn ? (s.battery != null ? `${text} · ${s.battery}%` : text) : text;

  const svc = $("svc");
  svc.classList.toggle("off", !s.vigem_ok);
  svc.firstChild.textContent = s.vigem_ok ? "ON" : "OFF";
  svc.title = s.vigem_ok ? "Controller service is running" : "ViGEmBus driver missing";

  renderBanners(s);
}

function renderBanners(s) {
  const list = [];
  if (!s.vigem_ok)
    list.push(["error", "ViGEmBus driver is missing, so games can't see the controller. Install it, then restart Pad Companion.", "Install", "https://github.com/nefarius/ViGEmBus/releases/latest"]);
  if (!s.hidhide_ok)
    list.push(["warning", "HidHide isn't installed. Some games and Steam may see two controllers.", "Install", "https://github.com/nefarius/HidHide/releases/latest"]);
  for (const m of s.other_mappers || [])
    list.push(["warning", `${m} is also running and may double your input. Close it for the best experience.`]);
  const box = $("banners");
  box.innerHTML = list
    .map(([ico, text, btn, url]) => `<div class="banner"><span class="ms">${ico}</span><span>${text}</span>${btn ? `<button data-url="${url}">${btn}</button>` : ""}</div>`)
    .join("");
  box.querySelectorAll("button").forEach((b) => (b.onclick = () => invoke("open_external", { url: b.dataset.url })));
}
