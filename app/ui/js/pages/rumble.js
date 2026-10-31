// Rumble: strength for the active profile, with a test pulse.

import { profile, save, onSettings, invoke, status, toast } from "../ipc.js";
import { h, slider, panel } from "../ui.js";

let strength;

export function init(el) {
  el.innerHTML = `<h1>Rumble</h1><p class="lede">Games send vibration to the virtual Xbox controller; Pad Companion passes it to the Stadia pad's motors at the strength you choose.</p>`;
  const p = panel("Vibration strength", "vibration");
  strength = slider({ label: "Strength", min: 0, max: 100, step: 5, value: profile().rumble_pct, onInput: (v) => save(() => (profile().rumble_pct = v)) });
  const test = h(`<button class="btn" style="margin-top:18px"><span class="ms">play_arrow</span>Test rumble</button>`);
  test.onclick = () => {
    if (!status?.connection) return toast("Connect the controller first", "link_off");
    invoke("test_rumble");
  };
  p.append(strength, test);
  p.style.maxWidth = "560px";
  el.append(p);
  onSettings(() => strength.set(profile().rumble_pct));
}
