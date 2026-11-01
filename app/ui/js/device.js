// Device view: sidebar navigation, profile chip, and the seven settings pages.

import { settings, onSettings, invoke } from "./ipc.js";
import * as mapping from "./pages/mapping.js";
import * as special from "./pages/special.js";
import * as performance from "./pages/performance.js";
import * as sticks from "./pages/sticks.js";
import * as rumble from "./pages/rumble.js";
import * as profiles from "./pages/profiles.js";
import * as other from "./pages/other.js";

const PAGES = { mapping, special, performance, sticks, rumble, profiles, other };
const $ = (id) => document.getElementById(id);

export function showPage(name) {
  document.querySelectorAll(".nav").forEach((n) => n.classList.toggle("on", n.dataset.p === name));
  document.querySelectorAll(".page").forEach((p) => p.classList.toggle("on", p.id === `p-${name}`));
  PAGES[name].shown?.();
  // The top tab names the open page; its × closes it back to Home.
  const nav = document.querySelector(`.nav[data-p="${name}"]`);
  const label = nav ? nav.textContent.replace(nav.querySelector(".ms")?.textContent ?? "", "").trim() : "";
  const tab = $("tabName");
  if (label && tab.textContent !== label) {
    tab.textContent = label;
    $("tabIco").textContent = nav.querySelector(".ms").textContent;
    $("tabClose").setAttribute("aria-label", `Close ${label}, back to Home`);
    $("tabClose").title = `Close ${label}`;
    $("devTab").classList.remove("swap");
    void $("devTab").offsetWidth;
    $("devTab").classList.add("swap");
  }
  window.dispatchEvent(new Event("viewchange")); // the page's pad catches up with the live state
}

export function initDevice() {
  for (const [name, mod] of Object.entries(PAGES)) mod.init($(`p-${name}`));
  document.querySelectorAll(".nav").forEach((n) => (n.onclick = () => showPage(n.dataset.p)));
  $("profChip").onclick = () => showPage("profiles");
  $("bugBtn").onclick = () => invoke("open_log_folder");
  $("bugBtn").title = "Opens the log folder: attach log.txt to your bug report";
  const prof = () => ($("profName").textContent = `Profile · ${settings.active_profile}`);
  prof();
  onSettings(prof);
}
