// Boot: load settings + status, theme the app, wire the title bar, Home and the device pages.

import { init, invoke, listen, settings } from "./ipc.js";
import { applyScheme } from "./theme.js";
import { applyPadState } from "./pad.js";
import { initHome, renderStatus } from "./home.js";
import { initDevice, showPage } from "./device.js";

const $ = (id) => document.getElementById(id);

function goInside(inside) {
  document.body.classList.toggle("inside", inside);
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

  // Live pad: every pad drawing on screen follows the controller (events arrive ≤ 60/s).
  let frame = null, latest = null;
  listen("pad", (p) => {
    latest = p.state;
    window.dispatchEvent(new CustomEvent("padstate", { detail: p }));
    if (!frame) frame = requestAnimationFrame(() => {
      frame = null;
      document.querySelectorAll("svg.pad:not(.next)").forEach((s) => applyPadState(s, latest));
    });
  });
  listen("stats", (s) => window.dispatchEvent(new CustomEvent("padstats", { detail: s })));
}

boot().catch((e) => {
  document.body.insertAdjacentHTML("beforeend", `<div class="banner" style="position:fixed;top:60px;left:50%;translate:-50% 0"><span class="ms">error</span><span>Pad Companion couldn't load: ${e}</span></div>`);
});
