// Device view: sidebar navigation, profile chip, and the seven settings pages.

import { settings, status, onSettings, invoke, toast, appInfo } from "./ipc.js";
import * as mapping from "./pages/mapping.js";
import * as special from "./pages/special.js";
import * as performance from "./pages/performance.js";
import * as sticks from "./pages/sticks.js";
import * as rumble from "./pages/rumble.js";
import * as profiles from "./pages/profiles.js";
import * as other from "./pages/other.js";

const PAGES = { mapping, special, performance, sticks, rumble, profiles, other };
const $ = (id) => document.getElementById(id);

/** The support site (site/ in the repo, published to GitHub Pages): bug reports and feature requests
 *  become GitHub issues there. The app's details ride along so the report form is pre-filled. */
const SUPPORT_SITE = "https://eiza-1.github.io/stadia-pad-companion/";
let appVersion = "";
appInfo().then((i) => (appVersion = i?.version || "")).catch(() => {});
function supportUrl() {
  const p = new URLSearchParams();
  if (appVersion) p.set("v", appVersion);
  p.set("conn", status?.connection || "none");
  const fw = status?.device?.firmware || status?.device?.version;
  if (fw) p.set("fw", fw);
  return `${SUPPORT_SITE}?${p}#bug`;
}

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
  $("bugBtn").onclick = () => invoke("open_external", { url: supportUrl() }).catch(() => toast("Couldn't open the support site", "error"));
  $("bugBtn").title = "Opens the support site to report a bug or suggest a feature";
  const prof = () => ($("profName").textContent = `Profile · ${settings.active_profile}`);
  prof();
  onSettings(prof);
}
