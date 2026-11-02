// Boot: load settings + status, theme the app, wire the title bar, Home and the device pages.

import { init, invoke, listen, settings } from "./ipc.js";
import { applyScheme } from "./theme.js";
import { applyPadState } from "./pad.js";
import { initHome, renderStatus, closePops } from "./home.js";
import { initDevice, showPage } from "./device.js";

const $ = (id) => document.getElementById(id);

function goInside(inside) {
  document.body.classList.toggle("inside", inside);
  // The hidden view still sits on top of the visible one; inert stops it taking clicks and focus.
  $("homeV").inert = inside;
  $("devV").inert = !inside;
  // Home and the device tab only exist inside the configuration; on Home they'd do nothing.
  $("homeBtn").inert = $("devTab").inert = !inside;
  if (inside) closePops();
  window.dispatchEvent(new Event("viewchange"));
}

async function boot() {
  const { status } = await init();
  applyScheme(settings.pad_color); // before building pages, so they render in the saved colour
  initHome(goInside);
  initDevice();
  applyScheme(settings.pad_color); // again, to colour the pad drawings the pages just created
  renderStatus(status);

  $("homeBtn").onclick = $("tabClose").onclick = () => goInside(false);
  $("settingsBtn").onclick = () => { goInside(true); showPage("other"); };
  $("winMin").onclick = () => invoke("window_minimize");
  $("winMax").onclick = () => invoke("window_maximize");
  $("winClose").onclick = () => invoke("window_close");
  document.addEventListener("keydown", (e) => e.key === "Escape" && document.body.classList.contains("inside") && goInside(false));
  // "Open settings" from the tray menu: on a new window via #page, on an open one via an event.
  const goto = (p) => { goInside(true); showPage(p); };
  listen("goto", goto);
  const start = location.hash.slice(1);
  if (start === "other") goto(start);

  // Live pad: only the drawing on the open configuration page follows the controller, at most once per
  // frame, and nothing is drawn while the window is minimised. Hidden pages catch up when they are
  // shown. The home pad is a still picture you click to open configuration: it never shows input.
  let frame = null, latest = null, lastEvent = null;
  const visiblePads = () =>
    document.body.classList.contains("inside") ? document.querySelectorAll(".page.on svg.pad") : [];
  const draw = () => {
    frame = null;
    if (!latest) return;
    visiblePads().forEach((s) => applyPadState(s, latest));
    window.dispatchEvent(new CustomEvent("padstate", { detail: lastEvent }));
  };
  const schedule = () => { if (!frame && !document.hidden) frame = requestAnimationFrame(draw); };
  listen("pad", (p) => { latest = p.state; lastEvent = p; schedule(); });
  window.addEventListener("viewchange", schedule);
  document.addEventListener("visibilitychange", schedule);
  listen("stats", (s) => !document.hidden && window.dispatchEvent(new CustomEvent("padstats", { detail: s })));

  // Decorative motion (drifting glow, floating pad, pulsing dot) stops whenever the window isn't the
  // one you're using, so it never competes with a game for the GPU.
  const still = () => document.body.classList.toggle("still", document.hidden || !document.hasFocus());
  window.addEventListener("blur", still);
  window.addEventListener("focus", still);
  document.addEventListener("visibilitychange", still);
  still();
}

boot().catch((e) => {
  document.body.insertAdjacentHTML("beforeend", `<div class="banner" style="position:fixed;top:60px;left:50%;translate:-50% 0"><span class="ms">error</span><span>Pad Companion couldn't load: ${e}</span></div>`);
});
