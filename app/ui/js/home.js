// Home: the device card (real pad photo), live connection/battery pills, palette with a top-to-bottom
// colour wipe, firmware and manual, driver banners.

import { renderPad, colorPad, applyPadState, preloadPads } from "./pad.js";
import { SCHEMES, applyScheme, current } from "./theme.js";
import { settings, status, save, invoke, onStatus, toast } from "./ipc.js";

const $ = (id) => document.getElementById(id);
const BLE_HINT = "Plug in the cable, or hold Stadia + Y for 2 s to pair over Bluetooth.";
const MANUAL_URL = "https://support.google.com/stadia/answer/9565956?hl=en";

// Home popovers (palette, firmware): one open at a time, each grows out of the button that opened it.
const pops = {};
let openPop = null;

/** Anchors a popover above its button and points its grow-circle at the button's centre. */
function placePop(key) {
  const { btn, el } = pops[key];
  const card = el.offsetParent.getBoundingClientRect(), b = btn.getBoundingClientRect();
  el.style.right = `${card.right - b.right - 6}px`;
  el.style.bottom = `${card.bottom - b.top + 10}px`;
  // Clip circle starts on the button's centre (just below the popover) and grows to cover it.
  const p = el.getBoundingClientRect();
  const ox = b.left + b.width / 2 - p.left, oy = b.top + b.height / 2 - p.top;
  el.style.setProperty("--ox", `${ox}px`);
  el.style.setProperty("--oy", `${oy}px`);
  el.style.setProperty("--or", `${Math.hypot(Math.max(ox, p.width - ox), oy) + 8}px`);
}

function showPop(key) {
  closePops();
  const { btn, el, onOpen } = pops[key];
  onOpen?.();
  placePop(key);
  void el.getBoundingClientRect();
  el.inert = false;
  el.classList.add("open");
  btn.classList.add("act");
  btn.setAttribute("aria-expanded", "true");
  openPop = key;
  (el.querySelector('[tabindex="0"]') || el.querySelector("button"))?.focus({ preventScroll: true });
}

export function closePops() {
  const p = pops[openPop];
  if (!p) return;
  openPop = null;
  p.el.classList.remove("open");
  p.el.inert = true;
  p.btn.classList.remove("act");
  p.btn.setAttribute("aria-expanded", "false");
  p.onClose?.();
}

/** Firmware panel rows, also used for "Copy details". */
function fwRows(d) {
  if (!d) return [];
  return [
    ["Model", d.model],
    ["Firmware", d.firmware ?? `Version ${d.version}`],
    ["Bluetooth mode", d.bluetooth_firmware ? "Installed" : "Pair over Bluetooth to confirm"],
    ["Hardware ID", d.hardware_id],
    ...(d.serial ? [["Serial", d.serial]] : []),
  ];
}

function renderFirmware(s) {
  const d = s?.connection ? s.device : null;
  $("fwSub").textContent = d ? (s.connection === "Bluetooth" ? "Read over Bluetooth" : "Read over USB") : "";
  $("fwList").innerHTML = "";
  for (const [k, v] of fwRows(d)) {
    const dt = document.createElement("dt"), dd = document.createElement("dd");
    dt.textContent = k;
    dd.textContent = v;
    if (k === "Bluetooth mode") dd.classList.toggle("ok", !!d.bluetooth_firmware);
    $("fwList").append(dt, dd);
  }
  $("fwList").hidden = $("fwCopy").hidden = !d;
  $("fwEmpty").hidden = !!d;
}

export function initHome(goInside) {
  const cur = document.querySelector("#pw .cur");
  const next = document.querySelector("#pw .next");
  renderPad(cur, settings.pad_color);
  renderPad(next, settings.pad_color);
  preloadPads();

  // Palette: clicking a colour sweeps the new pad photo down over the old one (top to bottom), the way
  // a real colour swap is shown; the app's accents follow at the same moment.
  $("swgrid").innerHTML = Object.entries(SCHEMES)
    .map(([k, s]) => `<button class="swc" role="radio" data-c="${k}" aria-checked="false"><i style="background:${s.swatch}"></i><span>${s.name}</span><span class="ms ck">check</span></button>`)
    .join("");
  const tiles = [...$("swgrid").querySelectorAll(".swc")];
  let pending;
  const WIPE_MS = 850;
  const wipe = (c) => {
    if (c === current) return;
    clearTimeout(pending);
    colorPad(cur, current);          // a wipe still running finishes instantly on the colour it was showing
    colorPad(next, c);
    next.classList.remove("go");
    void next.getBoundingClientRect();
    next.classList.add("go");
    applyScheme(c);
    pending = setTimeout(() => { colorPad(cur, c); next.classList.remove("go"); window.dispatchEvent(new Event("viewchange")); }, WIPE_MS + 50);
    $("palName").textContent = SCHEMES[c].name;
  };
  const mark = () => tiles.forEach((t) => {
    const on = t.dataset.c === settings.pad_color;
    t.classList.toggle("on", on);
    t.setAttribute("aria-checked", String(on));
    t.tabIndex = on ? 0 : -1;
  });
  for (const [i, t] of tiles.entries()) {
    t.onclick = () => {
      wipe(t.dataset.c);
      if (settings.pad_color !== t.dataset.c) save((s) => (s.pad_color = t.dataset.c));
      mark();
      closePops();
    };
    t.onkeydown = (e) => {
      const step = { ArrowRight: 1, ArrowDown: 2, ArrowLeft: -1, ArrowUp: -2 }[e.key];
      if (!step) return;
      e.preventDefault();
      tiles[(i + step + tiles.length) % tiles.length].focus();
    };
  }
  mark();

  pops.pal = { btn: $("palBtn"), el: $("palPop"),
    onOpen: () => { mark(); $("palName").textContent = SCHEMES[settings.pad_color]?.name ?? ""; } };
  pops.fw = { btn: $("fwBtn"), el: $("fwPop"), onOpen: () => renderFirmware(status) };
  for (const [k, p] of Object.entries(pops)) p.btn.onclick = () => (openPop === k ? closePops() : showPop(k));
  document.addEventListener("pointerdown", (e) => {
    const p = pops[openPop];
    if (p && !p.el.contains(e.target) && !p.btn.contains(e.target)) closePops();
  });
  document.addEventListener("keydown", (e) => {
    if (e.key !== "Escape" || !openPop) return;
    const btn = pops[openPop].btn;
    closePops();
    btn.focus();
  });
  window.addEventListener("resize", () => openPop && placePop(openPop));
  // Tabbing out of a popover closes it (focus moving to its own button is a toggle, handled above).
  for (const p of Object.values(pops))
    p.el.addEventListener("focusout", (e) => {
      if (pops[openPop] === p && e.relatedTarget && !p.el.contains(e.relatedTarget) && e.relatedTarget !== p.btn) closePops();
    });

  $("fwCopy").onclick = async () => {
    try {
      await navigator.clipboard.writeText(fwRows(status?.device).map(([k, v]) => `${k}: ${v}`).join("\n"));
      toast("Firmware details copied");
    } catch {
      toast("Couldn't copy to the clipboard", "error");
    }
  };
  $("manBtn").onclick = () => invoke("open_external", { url: MANUAL_URL }).catch(() => toast("Couldn't open the manual", "error"));

  hoverTilt($("stage"), $("tilt"));
  $("stage").onclick = () => goInside(true);
  $("stage").onkeydown = (e) => (e.key === "Enter" || e.key === " ") && goInside(true);

  onStatus(renderStatus);
}

/** The Home pad leans toward the pointer and grows a little; eased per frame so it glides, never snaps. */
function hoverTilt(stage, el) {
  if (matchMedia("(prefers-reduced-motion: reduce)").matches) return;
  const target = { rx: 0, ry: 0, s: 1 }, now = { rx: 0, ry: 0, s: 1 };
  let frame = null;
  const step = () => {
    let moving = false;
    for (const k of ["rx", "ry", "s"]) {
      now[k] += (target[k] - now[k]) * 0.09;
      if (Math.abs(target[k] - now[k]) > 0.0005) moving = true;
      else now[k] = target[k];
    }
    el.style.setProperty("--rx", `${now.rx.toFixed(3)}deg`);
    el.style.setProperty("--ry", `${now.ry.toFixed(3)}deg`);
    el.style.setProperty("--s", now.s.toFixed(4));
    frame = moving ? requestAnimationFrame(step) : null;
  };
  const kick = () => (frame ??= requestAnimationFrame(step));
  stage.addEventListener("pointermove", (e) => {
    const r = stage.getBoundingClientRect();
    const nx = ((e.clientX - r.left) / r.width) * 2 - 1, ny = ((e.clientY - r.top) / r.height) * 2 - 1;
    Object.assign(target, { rx: -ny * 6, ry: nx * 8, s: 1.035 });
    kick();
  });
  const settle = () => { Object.assign(target, { rx: 0, ry: 0, s: 1 }); kick(); };
  stage.addEventListener("pointerleave", settle);
  stage.addEventListener("click", settle); // Home fades out without a pointerleave; come back level
}

function battIcon(pct) {
  const bars = Math.max(0, Math.min(6, Math.round((pct / 100) * 6)));
  return pct >= 95 ? "battery_full" : `battery_${bars}_bar`;
}

/** Connection pill, battery chip and banners (a missing ViGEmBus driver shows there), from the bridge Status. */
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

  renderBanners(s);
  if (openPop === "fw") renderFirmware(s);
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
