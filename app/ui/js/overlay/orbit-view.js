// Draws the Orbit rail under the pill and drives it from the pad (overlay main.js hands us pad states).
// Holding the stick or D-pad glides: one step at once, the next after 320 ms, then every 70 ms
// (40 ms past 85 % stick deflection). Suggestions: D-pad up / down chooses, Y takes it; a click or Tab
// (Up / Down in the field) does the same from the mouse and keyboard.

import { createOrbit } from "./orbit.js";

const SPAN = 7; // characters shown each side of the focus key
const FIRST_MS = 320, REPEAT_MS = 70, FAST_MS = 40, FAST_AT = 0.85, DEAD = 0.5;

export function attach({ box, field, fit, send, keyboard }) {
  const orbit = createOrbit();
  let shown = false;
  let dir = 0, fast = false, nextAt = 0, ticker = null;

  const sync = () => {
    field.value = orbit.text;
    fit();
    field.scrollLeft = field.scrollWidth;
  };

  function draw() {
    if (!shown) return box.replaceChildren();
    const st = orbit.state;
    const chars = st.chars;
    const row = document.createElement("div");
    row.className = "rail glass";
    for (let d = -SPAN; d <= SPAN; d++) {
      const c = chars[(st.index + d + chars.length * 4) % chars.length];
      const s = document.createElement("span");
      s.className = d === 0 ? "n0" : Math.abs(d) <= 2 ? "n1" : "";
      s.textContent = d === 0 ? st.focus : st.caps && st.page === "abc" ? c.toUpperCase() : c;
      row.append(s);
    }
    const sugg = document.createElement("div");
    sugg.className = "sugg";
    st.suggestions.forEach((w, i) => {
      const b = document.createElement("button");
      b.type = "button";
      b.className = `glass${i === st.selected ? " on" : ""}`;
      b.textContent = w;
      if (i === st.selected) {
        const y = document.createElement("kbd");
        y.textContent = "Y";
        y.title = "Press Y (or Tab) to use this word";
        b.append(y);
      }
      b.addEventListener("mousedown", (e) => e.preventDefault()); // keep the caret in the field
      b.onclick = () => {
        orbit.pick(i);
        sync();
        draw();
      };
      sugg.append(b);
    });
    const tabs = document.createElement("div");
    tabs.className = "tabs";
    for (const [k, label] of [["LB", "LB"], ["abc", "abc"], ["123", "123"], ["emoji", "😀"], ["RB", "RB"]]) {
      const t = document.createElement("span");
      t.className = k === st.page ? "on" : "glass";
      t.textContent = label;
      tabs.append(t);
    }
    box.replaceChildren(sugg, row, tabs);
  }

  const step = (edge) => {
    orbit.input(edge);
    draw();
  };

  function setDir(d, isFast) {
    fast = isFast;
    if (d === dir) return;
    dir = d;
    clearInterval(ticker);
    ticker = null;
    if (!d) return;
    step(d < 0 ? "left" : "right");
    nextAt = performance.now() + FIRST_MS;
    ticker = setInterval(() => {
      const now = performance.now();
      if (now >= nextAt) {
        step(dir < 0 ? "left" : "right");
        nextAt = now + (fast ? FAST_MS : REPEAT_MS);
      }
    }, 15);
  }

  field.addEventListener("input", () => {
    orbit.text = field.value;
    draw();
  });
  field.addEventListener("keydown", (e) => {
    if (!shown || !orbit.state.suggestions.length) return;
    const edge = e.key === "Tab" ? "Y" : e.key === "ArrowDown" ? "down" : e.key === "ArrowUp" ? "up" : null;
    if (!edge) return;
    e.preventDefault(); // orbit.text already follows the field (input event above)
    orbit.input(edge);
    if (edge === "Y") sync();
    draw();
  });

  keyboard.learn = (t) => orbit.learn(t);
  keyboard.onShow = () => {
    shown = true;
    orbit.text = field.value;
    draw();
  };
  keyboard.onHide = () => {
    shown = false;
    setDir(0, false);
    draw();
  };
  /** Returns true when the rail used this pad state (main.js then does nothing more with it). */
  keyboard.onPad = (s, pressed) => {
    if (!shown) return false;
    const down = new Set(s.buttons ? String(s.buttons).split(" | ") : []);
    const fx = (s.lx - 127.5) / 127.5;
    const stick = Math.abs(fx) > DEAD ? Math.sign(fx) : 0;
    const pad = down.has("DPAD_LEFT") ? -1 : down.has("DPAD_RIGHT") ? 1 : 0;
    setDir(pad || stick, !pad && Math.abs(fx) > FAST_AT);
    if (pressed("B") && !field.value) return false; // empty: main.js tucks the pill away
    if (pressed("MENU")) {
      send();
      orbit.text = "";
      draw();
      return true;
    }
    let typed = false;
    for (const [b, e] of [["DPAD_UP", "up"], ["DPAD_DOWN", "down"]]) {
      if (pressed(b)) {
        orbit.input(e);
        draw();
      }
    }
    for (const e of ["A", "B", "X", "Y", "LB", "RB", "L3"]) {
      if (pressed(e)) {
        orbit.input(e);
        typed = true;
      }
    }
    if (typed) {
      sync();
      draw();
    }
    return true;
  };
}
