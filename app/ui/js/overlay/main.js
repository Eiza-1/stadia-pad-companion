// Gemini overlay page (overlay.html). overlay.rs opens it hidden; we draw, report our height, and it
// shows the window without taking focus. Modes come from the Assistant button / shortcut ("ov_mode"),
// live words from dictation ("stt"), answers from Gemini ("answer"), the pad while we hold it ("pad").

import { createStack, isLong } from "./cards.js";
import { attach } from "./orbit-view.js";
import { attachLive } from "./live-view.js";

const tauri = window.__TAURI__;
const invoke = (cmd, args) => tauri.core.invoke(cmd, args);
const listen = (ev, fn) => tauri.event.listen(ev, (e) => fn(e.payload));
const $ = (id) => document.getElementById(id);

const stack = createStack({ max: 3 });
let voice = "Text";
let mode = "hidden";
let keyOk = false;
let typingBefore = "";
let prevPad = ""; // last pad buttons seen, for press edges
const keyboard = { onPad: null, onShow: null, onHide: null, learn: null }; // filled by the Orbit rail (orbit-view.js)
export { keyboard };

// ---------- sizing: the window follows the content, bottom edge fixed ----------
// overlay.css lays out at Standard size in 720 px; the pill size setting zooms it (--s).
const BASE_WIDTH = 720;
let ready = false;
let scale = 1;
const height = () => Math.ceil(document.getElementById("ov").getBoundingClientRect().height) + 2;
const size = () => ({ height: height(), width: Math.ceil(BASE_WIDTH * scale) });
const report = () => ready && invoke("overlay_resize", size());
new ResizeObserver(report).observe($("ov"));

/** Pill size, entrance animation and the Windows accent colour (Settings › Gemini, live). */
function applyStyle(st) {
  scale = Math.min(1.4, Math.max(0.7, Number(st.scale) || 1));
  document.documentElement.style.setProperty("--s", String(scale));
  if (st.accent) document.documentElement.style.setProperty("--accent", st.accent);
  document.body.dataset.anim = st.anim || "Glow";
  report();
}

/** Plays the entrance each time the pill comes up. */
function enter() {
  document.body.classList.remove("enter");
  void document.body.offsetWidth; // restart the animations
  document.body.classList.add("enter");
}

// ---------- the pill ----------
function setMode(m) {
  if (m !== mode) prevPad = ""; // a fresh start for pad edges
  if (mode === "hidden" && m !== "hidden" && m !== "live") enter();
  if (m === "hidden") document.body.classList.remove("enter");
  if (m === "live" && mode !== "live") invoke("get_settings").then((s) => liveView.show(s));
  if (m !== "live" && mode === "live") liveView.hide();
  mode = m;
  document.body.classList.toggle("talk", m === "talk");
  const showPill = m !== "hidden" && m !== "live";
  const needKey = showPill && !keyOk;
  $("keycard").hidden = !needKey;
  $("pillwrap").hidden = !showPill;
  hint("");
  if (m === "talk") {
    typingBefore = $("field").value;
    $("field").value = "";
    $("field").placeholder = "Listening…";
  } else {
    $("field").placeholder = "Ask Gemini";
  }
  if (m === "type") keyboard.onShow?.();
  else keyboard.onHide?.();
  idleCheck();
}

function hint(text, err = false) {
  $("hint").textContent = text;
  $("hint").classList.toggle("err", err);
}

/** Sends what's in the field; the pill tucks away and the pad goes back to the game (overlay.rs). */
export function send() {
  const text = $("field").value.trim();
  if (!text) return;
  if (!keyOk) return setMode(mode); // shows the key card
  $("field").value = "";
  fit();
  keyboard.learn?.(text); // suggestions get better with what the player asks
  invoke("overlay_ask", { text });
}

/** B on an empty field, or Esc: tuck the pill away; with no cards either, close the overlay. */
export function back() {
  if ($("field").value) return;
  stack.closeAll();
  drawCards();
  invoke("overlay_capture", { on: false });
  setMode("hidden");
}

export const field = () => $("field");

/** Fits the field's text size to its length (call after changing its value). */
export function fit() {
  const n = $("field").value.length;
  $("field").classList.toggle("long", n > 26 && n <= 40);
  $("field").classList.toggle("longer", n > 40);
}
$("field").addEventListener("input", fit);

$("field").addEventListener("mousedown", () => invoke("overlay_focus"));
listen("ov_focus", () => $("field").focus());
$("field").addEventListener("keydown", (e) => {
  if (e.key === "Enter") send();
  if (e.key === "Escape") back();
});
document.addEventListener("keydown", (e) => {
  if (e.key !== "Escape" || document.activeElement === $("field")) return;
  if (mode === "live") invoke("live_end");
  else close();
});
$("mic").onclick = () => hint("Hold the Assistant button (or your shortcut) and speak");
$("plus").onclick = () => hint("Coming soon: add files and search your PC");
// Gemini Live: the waveform button talks; "Share screen with Live" also shares the screen.
$("live").onclick = () => (keyOk ? invoke("live_start_cmd", { share: false }) : setMode(mode));
$("share").onclick = () => (keyOk ? invoke("live_start_cmd", { share: true }) : setMode(mode));
const liveView = attachLive({
  invoke,
  listen,
  onEnded: (why) => {
    if (!why) return;
    const id = `live-${Date.now()}`;
    stack.add(id, "Gemini Live");
    stack.error(id, why);
    drawCards();
  },
});

// Dragging by the handle; the new spot is remembered once the window stops moving. Only a real drag
// counts: the window also moves whenever its height changes (overlay.rs keeps the bottom edge put).
let dragging = false, dragTimer;
$("handle").addEventListener("mousedown", () => {
  dragging = true;
  tauri.window.getCurrentWindow().startDragging();
});
tauri.window.getCurrentWindow().onMoved(() => {
  if (!dragging) return;
  clearTimeout(dragTimer);
  dragTimer = setTimeout(() => {
    dragging = false;
    invoke("overlay_drag_end");
  }, 400);
});

// ---------- speech ----------
listen("stt", (s) => {
  if (s.error) return hint(s.error, true);
  if (s.empty) {
    $("field").value = typingBefore;
    fit();
    return hint("Didn't catch that");
  }
  $("field").value = s.text;
  fit();
  $("field").scrollLeft = $("field").scrollWidth;
});

// ---------- answers (Voice / Both: overlay.rs has Gemini read them aloud and tells us how long) ----------
listen("answer", (a) => {
  if (a.question != null) stack.add(a.id, a.question);
  else if (a.delta) stack.append(a.id, a.delta);
  else if (a.done) stack.done(a.id);
  else if (a.speaking != null) {
    stack.speaking(a.id);
    setTimeout(() => { const c = stack.cards.find((k) => k.id === a.id); if (c) { c.voice = false; drawCards(); } }, a.speaking);
  }
  else if (a.voice_error) stack.voiceError(a.id, a.voice_error);
  else if (a.error === "nokey") {
    stack.close(a.id);
    keyOk = false;
    invoke("overlay_show_type"); // overlay.rs takes the pad and shows the pill (with the key card)
  } else if (a.error) stack.error(a.id, a.error);
  drawCards();
});

function drawCards() {
  const box = $("cards");
  box.replaceChildren(
    ...stack.cards.map((c) => {
      const el = document.createElement("article");
      el.className = `card glass${c.old ? " old" : ""}${c.error ? " error" : ""}`;
      const q = document.createElement("div");
      q.className = "q";
      q.append(document.createElement("i"), c.question);
      const a = document.createElement("div");
      a.className = "a";
      a.textContent = c.error || c.text;
      const note = document.createElement("div");
      note.className = "vnote";
      if (c.voiceError && voice !== "Text") note.textContent = `Voice: ${c.voiceError}`;
      else if (c.voice) note.textContent = "🔊 Speaking…";
      else if (voice !== "Text" && c.done && !c.error && !c.voiceSeen) note.textContent = "Getting the voice…";
      if (c.voice) c.voiceSeen = true;
      // Long answers: the first lines with a fade, and Show more / Show less (the card stays until closed).
      const long = !c.error && isLong(c.text);
      el.classList.toggle("long", long);
      el.classList.toggle("open", long && c.open);
      let more = null;
      if (long) {
        more = document.createElement("button");
        more.type = "button";
        more.className = "more";
        more.innerHTML = c.open ? 'Show less <span aria-hidden="true">▴</span>' : 'Show more <span aria-hidden="true">▾</span>';
        more.onclick = () => { stack.toggle(c.id); drawCards(); };
      }
      const x = document.createElement("button");
      x.className = "x";
      x.type = "button";
      x.setAttribute("aria-label", "Close");
      x.textContent = "×";
      x.onclick = () => {
        if (c.voice) invoke("overlay_hush");
        stack.close(c.id);
        drawCards();
      };
      el.append(q, a, ...(more ? [more] : []), note, x);
      return el;
    }),
  );
  idleCheck();
}

// Nothing on screen for 20 s: close the window, so nothing of the overlay keeps running.
let idleTimer;
function idleCheck() {
  clearTimeout(idleTimer);
  if (mode === "hidden" && stack.cards.length === 0) idleTimer = setTimeout(close, 20000);
}
function close() {
  invoke("overlay_close"); // overlay.rs stops the voice too
}

// ---------- key setup ----------
$("keyStudio").onclick = () => invoke("gemini_open_studio");
$("keyPaste").onclick = async () => {
  try { $("keyIn").value = (await navigator.clipboard.readText()).trim(); } catch { $("keyMsg").textContent = "Couldn't read the clipboard; press Ctrl+V in the box"; }
};
$("keyIn").addEventListener("mousedown", () => invoke("overlay_focus"));
$("keySave").onclick = async () => {
  const msg = $("keyMsg");
  msg.classList.remove("err");
  msg.textContent = "Checking your key…";
  try {
    await invoke("gemini_key_set", { key: $("keyIn").value });
    keyOk = true;
    $("keyIn").value = "";
    msg.textContent = "";
    if (mode === "hidden") await invoke("overlay_show_type");
    else setMode(mode);
    hint("Key saved. Ask away");
  } catch (e) {
    msg.classList.add("err");
    msg.textContent = String(e);
  }
};

// ---------- pad while the overlay holds it (the Orbit rail adds typing) ----------
listen("pad", (s) => {
  const now = new Set(s.buttons ? String(s.buttons).split(" | ") : []);
  const before = new Set(prevPad ? prevPad.split(" | ") : []);
  prevPad = s.buttons || "";
  const pressed = (b) => now.has(b) && !before.has(b);
  if (keyboard.onPad?.(s, pressed)) return;
  if (pressed("MENU")) send();
  if (pressed("B")) back();
});

// ---------- boot ----------
async function boot() {
  const [settings, ok, style] = await Promise.all([invoke("get_settings"), invoke("gemini_key_status"), invoke("overlay_style")]);
  voice = settings.answer_voice || "Text";
  keyOk = ok;
  applyStyle(style);
  await listen("ov_style", applyStyle);
  attach({ box: $("kb"), field: $("field"), fit, send, keyboard });
  await document.fonts.ready;
  await listen("ov_mode", (m) => setMode(m.mode));
  ready = true;
  await invoke("overlay_ready", size());
}

boot();
