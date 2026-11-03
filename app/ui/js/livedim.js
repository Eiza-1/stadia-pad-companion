// Live dim window (livedim.html): fades the dim in as Live opens (easing off after 2 s unless the
// setting says Stays dark), and shows the red frame and chip while Share screen is on ("dim" events
// from overlay.rs). The first share ever also says not to show private things.

const tauri = window.__TAURI__;
const NOTE_KEY = "pc.live.shareNoteSeen";

async function boot() {
  const s = await tauri.core.invoke("get_settings");
  document.body.classList.toggle("ease", (s.live_dim || "EasesOff") === "EasesOff");
  requestAnimationFrame(() => document.body.classList.add("on"));
  await tauri.event.listen("dim", (e) => {
    const on = !!e.payload.sharing;
    document.body.classList.toggle("sharing", on);
    const label = document.querySelector("#chip span");
    if (on && !localStorage.getItem(NOTE_KEY)) {
      localStorage.setItem(NOTE_KEY, "1");
      label.textContent = "Sharing screen with Gemini · don't show private things";
      setTimeout(() => (label.textContent = "Sharing screen with Gemini"), 6000);
    }
  });
}

boot();
