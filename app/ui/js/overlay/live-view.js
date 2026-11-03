// Draws the Gemini Live panel and drives it from overlay.rs's "live" events: state line, captions,
// the orb's sound level (with rings on loud moments), Mute / Share screen / End. On Frosted and
// Nearly solid glass, the window gets Windows' frosted backdrop clipped to the panel (live_glass).

import { createCaptions } from "./live.js";

const $ = (id) => document.getElementById(id);
const RING_AT = 0.62, RING_EVERY_MS = 380;

export function attachLive({ invoke, listen, onEnded }) {
  let caps = createCaptions();
  let lv = 0.12, target = 0.12, lastRing = 0, raf = 0, glass = "Frosted";
  const state = (t) => ($("lstate").textContent = t);
  const press = (id, on) => $(id).setAttribute("aria-pressed", String(on));

  function drawCaptions() {
    const v = caps.view();
    $("lyou").textContent = v.you;
    $("lold").textContent = v.older;
    $("lnow").textContent = v.current;
  }

  // The orb follows the louder of the two voices, eased, with a slow breath when it's quiet.
  function tick(now) {
    lv += (target - lv) * 0.35;
    const breath = 0.12 + 0.05 * Math.sin(now / 600);
    document.documentElement.style.setProperty("--lv", Math.max(breath, lv).toFixed(3));
    if (lv > RING_AT && now - lastRing > RING_EVERY_MS) {
      lastRing = now;
      const r = document.createElement("div");
      r.className = "lring";
      $("lstage").append(r);
      setTimeout(() => r.remove(), 1900);
    }
    raf = requestAnimationFrame(tick);
  }

  /** The panel's rounded rect for the frosted backdrop (window px), once it has settled. */
  function placeGlass() {
    const r = $("lpanel").getBoundingClientRect();
    invoke("live_glass", { rect: [r.left, r.top, r.right, r.bottom], radius: 44 * (Number(getComputedStyle(document.documentElement).getPropertyValue("--s")) || 1), frosted: glass !== "Light" });
  }

  $("lmute").onclick = () => {
    const on = $("lmute").getAttribute("aria-pressed") !== "true";
    press("lmute", on);
    document.body.classList.toggle("muted", on);
    invoke("live_mute", { on });
    state(on ? "Mic muted" : "Listening…");
  };
  $("lshare").onclick = () => {
    const on = $("lshare").getAttribute("aria-pressed") !== "true";
    press("lshare", on);
    invoke("live_share", { on });
  };
  $("lend").onclick = () => invoke("live_end");

  listen("live", (e) => {
    if (e.state === "connecting") state("Connecting…");
    else if (e.state === "listening") state("Listening…");
    else if (e.state === "nokey") invoke("overlay_show_type");
    else if (e.state === "ended") onEnded(e.error || null);
    if (e.speaking != null) state(e.speaking ? "Speaking · hold Assistant to cut in" : document.body.classList.contains("muted") ? "Mic muted" : "Listening…");
    if (e.level_you != null) target = Math.max(e.level_you, e.level_gemini || 0);
    if (e.you) { caps.you(e.you); drawCaptions(); }
    if (e.gemini) { caps.gemini(e.gemini); drawCaptions(); }
    if (e.turn) caps.turnDone();
    if (e.sharing != null) press("lshare", e.sharing);
  });

  return {
    /** Live opened: spring out of the pill, start the orb. */
    show(settings) {
      glass = settings.live_glass || "Frosted";
      document.body.dataset.glass = glass;
      caps = createCaptions();
      drawCaptions();
      press("lmute", false);
      press("lshare", false);
      document.body.classList.remove("muted");
      state("Connecting…");
      const live = $("livewrap");
      live.classList.add("from-pill");
      live.hidden = false;
      document.body.classList.add("live");
      requestAnimationFrame(() => requestAnimationFrame(() => live.classList.remove("from-pill")));
      setTimeout(placeGlass, document.body.dataset.anim === "Glow" ? 750 : 250);
      cancelAnimationFrame(raf);
      raf = requestAnimationFrame(tick);
    },
    hide() {
      $("livewrap").hidden = true;
      document.body.classList.remove("live", "muted");
      cancelAnimationFrame(raf);
    },
    placeGlass,
  };
}
