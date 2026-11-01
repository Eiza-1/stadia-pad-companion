// Rumble: the Rumble Translation Engine profile for the active pad profile (pad-core rumble.rs), the
// master strength, and a Test rumble that plays a fixed scene through the selected profile.

import { profile, save, onSettings, onStatus, invoke, status, toast } from "../ipc.js";
import { h, slider, panel } from "../ui.js";

const MODES = [
  ["Default", "Default", "graphic_eq",
    "Exactly what the game sends, on both motors."],
  ["Competitive", "Competitive", "my_location",
    "No heavy motor shaking your aim. Every hit becomes a crisp 50 ms tick on the light motor, kicked to full power so it lands even on short taps. Idle rumble is ignored."],
  ["Immersive", "Immersive", "bolt",
    "Explosions and impacts hit at full force on the heavy motor; the high-frequency buzz is held to 20%. Rumble lasts as long as the game wants."],
];

let strength, test, note, modes;

export function init(el) {
  el.innerHTML = `<h1>Rumble</h1><p class="lede">Games send vibration to the virtual Xbox controller. Pad Companion translates it for the Stadia pad's two motors (heavy on the left, light on the right) with the profile you pick, then applies your strength.</p>`;
  const grid = h(`<div class="grid2"></div>`);

  const prof = panel("Rumble profile", "vibration");
  modes = h(`<div class="modes"></div>`);
  for (const [value, title, icon, text] of MODES) {
    const b = h(`<button class="mode" data-m="${value}"><b>${title}<span class="ms">${icon}</span></b><span class="d">${text}</span></button>`);
    b.onclick = () => save(() => (profile().rumble_mode = value));
    modes.append(b);
  }
  prof.append(modes);

  const str = panel("Strength", "tune");
  strength = slider({ label: "Strength", min: 0, max: 100, step: 5, value: profile().rumble_pct, onInput: (v) => save(() => (profile().rumble_pct = v)) });
  test = h(`<button class="btn" style="margin-top:18px"><span class="ms">play_arrow</span>Test rumble</button>`);
  test.onclick = () => {
    if (!status?.connection) return toast("Connect the controller first", "link_off");
    if (status.rumble === false) return toast("Rumble needs the USB cable", "usb");
    invoke("test_rumble");
  };
  const scene = h(`<p class="hint2">Test rumble plays the same short scene through your profile: an explosion, a three-round burst, then an engine hum.</p>`);
  // Over Bluetooth Windows refuses the pad's rumble report (a limit of the pad's Bluetooth firmware).
  note = h(`<div class="note-box info" hidden><span class="ms">usb</span><span>Rumble works over the USB cable. Over Bluetooth, Windows can't deliver vibration to this pad, a limit of its Bluetooth firmware, so plug in the cable to feel it. Everything else works as usual.</span></div>`);
  str.append(strength, note, test, scene);

  grid.append(prof, str);
  el.append(grid);

  const refresh = () => {
    strength.set(profile().rumble_pct);
    const m = profile().rumble_mode || "Default";
    modes.querySelectorAll(".mode").forEach((b) => b.classList.toggle("on", b.dataset.m === m));
  };
  onSettings(refresh);
  refresh();
  const show = (s) => {
    const off = s?.rumble === false;
    note.hidden = !off;
    test.classList.toggle("dis", off);
    test.setAttribute("aria-disabled", String(off));
  };
  onStatus(show);
  show(status);
}
