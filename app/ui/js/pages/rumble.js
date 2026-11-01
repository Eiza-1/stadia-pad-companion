// Rumble: strength for the active profile, with a test pulse.

import { profile, save, onSettings, onStatus, invoke, status, toast } from "../ipc.js";
import { h, slider, panel } from "../ui.js";

let strength, test, note;

export function init(el) {
  el.innerHTML = `<h1>Rumble</h1><p class="lede">Games send vibration to the virtual Xbox controller; Pad Companion passes it to the Stadia pad's motors at the strength you choose.</p>`;
  const p = panel("Vibration strength", "vibration");
  strength = slider({ label: "Strength", min: 0, max: 100, step: 5, value: profile().rumble_pct, onInput: (v) => save(() => (profile().rumble_pct = v)) });
  test = h(`<button class="btn" style="margin-top:18px"><span class="ms">play_arrow</span>Test rumble</button>`);
  test.onclick = () => {
    if (!status?.connection) return toast("Connect the controller first", "link_off");
    if (status.rumble === false) return toast("Rumble needs the USB cable", "usb");
    invoke("test_rumble");
  };
  // Over Bluetooth Windows refuses the pad's rumble report (a limit of the pad's Bluetooth firmware).
  note = h(`<div class="note-box info" hidden><span class="ms">usb</span><span>Rumble works over the USB cable. Over Bluetooth, Windows can't deliver vibration to this pad, a limit of its Bluetooth firmware, so plug in the cable to feel it. Everything else works as usual.</span></div>`);
  p.append(strength, note, test);
  p.style.maxWidth = "560px";
  el.append(p);
  onSettings(() => strength.set(profile().rumble_pct));
  const show = (s) => {
    const off = s?.rumble === false;
    note.hidden = !off;
    test.classList.toggle("dis", off);
    test.setAttribute("aria-disabled", String(off));
  };
  onStatus(show);
  show(status);
}
