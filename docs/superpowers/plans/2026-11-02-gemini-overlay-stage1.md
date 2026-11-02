# Gemini Overlay · Stage 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The Assistant button summons the "Ask Gemini" pill over any app: hold to talk (live Windows speech-to-text, release sends), tap to type with keyboard B (Orbit rail) on the pad; Gemini's answer appears as fading, closable, cascading cards (optionally read aloud), using the player's own key set up on first use.

**Architecture:** `pad-core` gains pure, tested pieces (Assistant tap/hold, an input gate that gives the game a neutral pad while the overlay has the pad, a Gemini request/stream parser) and Windows pieces (Credential Manager key store, dictation, Gemini HTTP). The Tauri app gets an on-demand, non-activating `overlay` window (overlay.html) that the dispatcher feeds with speech, answer and captured-pad events. The game-input hot path only gains one atomic load per report.

**Tech Stack:** Rust (pad-core, windows 0.62 crate, `ureq` 2 for HTTPS), Tauri 2, WebView2 HTML/CSS/JS modules, `node --test` for UI logic.

**Spec:** `docs/superpowers/specs/2026-11-02-gemini-overlay-design.md` (stage 1 only; stages 2–5 get their own plans). Mockup: `.superpowers/brainstorm/6133-1793588879/content/gemini-overlay-v2.html`; reference photo `C:\Users\User\Downloads\photo_2026-11-02_03-57-16.jpg`.

## Global Constraints

- No added latency on the pad: the bridge input path gains at most one `AtomicBool` load and the gate's O(1) filter per report; no locks, allocation or I/O added there.
- Nothing runs while the overlay is closed: no window, no recognizer, no network thread, no timers.
- The overlay never steals focus from the app in front unless the player clicks into the text field.
- The game never sees pad input while the overlay holds the pad, and never gets a phantom press when it hands the pad back.
- The player's own key only: stored in Windows Credential Manager under target `PadCompanion/GeminiApiKey`; never in `settings.json`, logs, events to the main window, or bug reports; never sent to the WebView.
- Network only to `https://generativelanguage.googleapis.com`.
- The Gemini model id lives in exactly one constant: `pad_core::gemini::MODEL = "gemini-flash-latest"`.
- Fonts: Google Sans Flex and Noto Color Emoji (COLRv1), both SIL OFL, bundled under `app/ui/assets/fonts/` with their licences.
- Assistant setting: `Overlay` (new default) / `Gemini` (open the app, unchanged behaviour) / `Off`. Upgrading users with `Gemini` move to `Overlay` once.
- Hold threshold: `ASSIST_HOLD_MS = 300`.
- Commit messages: no AI attribution lines (user preference).
- Never build the installer unless the user asks (user preference). `cargo check`/`cargo test` via the MSVC wrapper (`vcargo`, see README "Build from source") are fine.

## Review Focus

1. **Assistant released before the overlay window exists** (fast hold < window creation time): the talk session must still end and send; no stuck mic. → Task 7 test `talk_end_before_window_ready_still_sends`.
2. **Pad disconnects while the overlay holds it**: the gate resets, the overlay closes its keyboard, and on reconnect the game gets a neutral pad, not the last captured state. → Task 3 test `reset_clears_capture_and_latch`.
3. **Speech recognizer unavailable** (no speech language / online speech recognition off): the pill shows a one-line fix, typing still works, nothing hangs. → Task 6 maps the known HRESULTs; Task 8 renders `stt` `{error}`.
4. **Gemini errors**: no internet, invalid key (400/403), quota (429): one readable line in a card, the pill stays usable, the key isn't wiped. → Task 5 test `classify_http_errors`.
5. **Key pasted with spaces/newlines or quotes** around it: trimmed before validation and storage. → Task 4 test `normalize_key_trims_whitespace_and_quotes`.

---

### Task 1: Settings · Assistant overlay option, answer voice, one-time migration

**Files:**
- Modify: `crates/pad-core/src/settings.rs` (enums near line 33, `Settings` struct/Default near 145–175, `normalize`)
- Test: same file's `mod tests`

**Interfaces:**
- Produces: `AssistantAction::{Overlay, Gemini, Off}` (default `Overlay`); `pub enum AnswerVoice { #[default] Text, Voice, Both }`; `Settings.answer_voice: AnswerVoice`; `Settings.overlay_migrated: bool` (default `false`); `Settings.overlay_intro_seen: bool` (default `false`); `Settings.overlay_pos: Option<(i32, i32)>` (dragged pill position, physical px; `None` = lower centre).

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn old_gemini_setting_moves_to_overlay_once() {
    let p = write_tmp(r#"{"assistant":"Gemini"}"#);
    let s = Settings::load(&p);
    assert_eq!(s.assistant, AssistantAction::Overlay);
    assert!(s.overlay_migrated);
    let mut back = s.clone(); back.assistant = AssistantAction::Gemini; back.save(&p).unwrap();
    assert_eq!(Settings::load(&p).assistant, AssistantAction::Gemini, "a later choice of Gemini app is kept");
}
#[test]
fn new_settings_default_to_overlay_and_text_answers() {
    let s = Settings::default();
    assert_eq!(s.assistant, AssistantAction::Overlay);
    assert_eq!(s.answer_voice, AnswerVoice::Text);
    assert_eq!(s.overlay_pos, None);
}
```

- [ ] **Step 2:** Run `vcargo test -p pad-core settings` → FAIL (no `Overlay`).
- [ ] **Step 3:** Implement. In `normalize`: `if !self.overlay_migrated { if self.assistant == AssistantAction::Gemini { self.assistant = AssistantAction::Overlay; } self.overlay_migrated = true; }` (fresh installs are already `Overlay`).
- [ ] **Step 4:** Run `vcargo test -p pad-core settings` → PASS (all existing settings tests too).
- [ ] **Step 5:** Commit `feat(settings): Gemini overlay as the Assistant default, answer voice`.

### Task 2: Special · Assistant tap / hold-to-talk

**Files:**
- Modify: `crates/pad-core/src/special.rs`
- Test: same file

**Interfaces:**
- Consumes: `AssistantAction::Overlay` (Task 1).
- Produces: `pub const ASSIST_HOLD_MS: u64 = 300;` new `Action` variants `OverlayTap`, `TalkStart`, `TalkEnd` (serde names as variants). `Special::next_deadline()` includes the pending Assistant hold deadline.

- [ ] **Step 1: Write failing tests**

```rust
#[test] fn overlay_tap_fires_on_quick_release() { /* down at 0, up at 120 → [OverlayTap] on release, nothing on press */ }
#[test] fn overlay_hold_starts_and_ends_talk() { /* down 0; update at 299 → []; at 300 → [TalkStart]; up at 900 → [TalkEnd] */ }
#[test] fn hold_fires_from_timeout_without_new_report() { /* down 0, then update(&same_state, 310) → [TalkStart] */ }
#[test] fn gemini_app_setting_unchanged() { /* AssistantAction::Gemini: press → [OpenGemini] exactly as before */ }
#[test] fn set_clears_a_pending_hold() { /* down 0, set(...) , up 50 → [] */ }
```

Write each with the existing test helpers in `special.rs`; assertions as in the comments.

- [ ] **Step 2:** `vcargo test -p pad-core special` → FAIL.
- [ ] **Step 3:** Implement with a third `Hold` (`assist_hold`) mirroring `capture_hold`: press → start; ≥300 ms held and not fired → `TalkStart` (fired); release → `TalkEnd` if fired else `OverlayTap`.
- [ ] **Step 4:** `vcargo test -p pad-core special` → PASS.
- [ ] **Step 5:** Commit `feat(special): Assistant tap and hold-to-talk for the overlay`.

### Task 3: Input gate · neutral pad while captured, no phantom presses

**Files:**
- Create: `crates/pad-core/src/gate.rs` (+ `pub mod gate;` in `lib.rs`)
- Modify: `crates/pad-core/src/bridge.rs` (`Shared`, `BridgeHandle`, `BridgeEvent`, session loop ~line 700)
- Test: `gate.rs` tests

**Interfaces:**
- Produces:
  - `pub const NEUTRAL: PadState` (no buttons, sticks 128, triggers 0).
  - `pub struct Gate` with `fn new() -> Gate`, `fn set_captured(&mut self, on: bool, current: &PadState)`, `fn game_state(&self, s: &PadState) -> PadState`, `fn observe(&mut self, s: &PadState)` (clears latched buttons once released), `fn reset(&mut self)`.
  - `BridgeHandle::set_capture(&self, on: bool)`; `BridgeEvent::Captured(PadState)` emitted on every changed report while captured (not throttled, not gated by `live`).

- [ ] **Step 1: Write failing tests**

```rust
#[test] fn captured_gives_neutral() { /* set_captured(true); game_state(A pressed, lx 255) == NEUTRAL */ }
#[test] fn release_latches_buttons_held_at_handback() {
    // A held when capture ends: game must not see A until it is released and pressed again.
    let mut g = Gate::new(); let a = st(Buttons::A);
    g.set_captured(true, &a); g.set_captured(false, &a);
    assert!(!g.game_state(&a).buttons.contains(Buttons::A));
    g.observe(&NEUTRAL); g.observe(&a);
    assert!(g.game_state(&a).buttons.contains(Buttons::A), "a fresh press goes through");
}
#[test] fn sticks_pass_through_after_handback() { /* lx 200 after release → 200 */ }
#[test] fn reset_clears_capture_and_latch() { /* captured + latched, reset() → game_state(a) has A */ }
```

- [ ] **Step 2:** `vcargo test -p pad-core gate` → FAIL.
- [ ] **Step 3:** Implement `Gate` (captured flag + `latched: Buttons`). Bridge: `Shared.capture: AtomicBool`; in the session loop keep a `Gate`; once per report compare the atomic with the gate's flag and call `set_captured` on change; `let game = gate.game_state(&state); gate.observe(&state); let out = mapper.map(&game);`. While captured, emit `Captured(state)` when `state != last_state`. `gate.reset()` at session start. `special.update` keeps the real `state` (the Assistant button must still work).
- [ ] **Step 4:** `vcargo test -p pad-core` → PASS (all 55+ tests).
- [ ] **Step 5:** Commit `feat(bridge): input gate hands the pad to the overlay without phantom presses`.

### Task 4: Key store · Credential Manager

**Files:**
- Create: `crates/pad-core/src/win/keystore.rs` (+ `pub mod keystore;` in `win/mod.rs`; add `"Win32_Security_Credentials"` to the `windows` features)
- Test: same file

**Interfaces:**
- Produces: `pub fn normalize_key(raw: &str) -> String`; `pub fn get() -> Option<String>`; `pub fn set(key: &str) -> Result<(), String>`; `pub fn clear()`. Target `PadCompanion/GeminiApiKey`, type `CRED_TYPE_GENERIC`, persist `CRED_PERSIST_LOCAL_MACHINE`. Tests use target `PadCompanion/Test-<random>` via an internal `fn with_target(t: &str)` variant.

- [ ] **Step 1: Write failing tests**

```rust
#[test] fn normalize_key_trims_whitespace_and_quotes() { assert_eq!(normalize_key("  \"AIzaXYZ\"\n"), "AIzaXYZ"); }
#[test] fn round_trip_and_clear() { /* set_in(t,"AIza1") ; get_in(t)==Some("AIza1"); clear_in(t); get_in(t)==None */ }
```

- [ ] **Step 2:** `vcargo test -p pad-core keystore` → FAIL.
- [ ] **Step 3:** Implement with `CredWriteW` / `CredReadW` (+ `CredFree`) / `CredDeleteW`; blob = UTF-8 bytes.
- [ ] **Step 4:** `vcargo test -p pad-core keystore` → PASS.
- [ ] **Step 5:** Commit `feat(keystore): Gemini key in Windows Credential Manager`.

### Task 5: Gemini client · request, stream, errors

**Files:**
- Create: `crates/pad-core/src/gemini.rs` (+ `pub mod gemini;`; add `ureq = { version = "2", features = ["json"] }` to pad-core)
- Test: same file

**Interfaces:**
- Produces:
  - `pub const MODEL: &str = "gemini-flash-latest";`
  - `pub const SYSTEM: &str` — instruction: "You are a gaming assistant shown in a small overlay over a game. Answer in one to three short sentences; use a compact list only for combos or steps. No preamble."
  - `pub struct Turn { pub user: bool, pub text: String }`
  - `pub fn request_body(history: &[Turn]) -> serde_json::Value` (`system_instruction`, `contents` with roles `user`/`model`, `tools: [{"google_search": {}}]`).
  - `pub fn parse_sse_line(line: &str) -> Option<String>` (text delta from a `data: {...}` line; concatenates all `candidates[0].content.parts[*].text`).
  - `pub enum AskError { Offline, BadKey, Quota, Other(String) }` with `pub fn classify(status: Option<u16>, body: &str) -> AskError` and `pub fn message(&self) -> &'static str` ("No internet connection", "Your Gemini key isn't working. Check it in Settings", "Gemini's free limit is used up for now. Try again in a minute", "Gemini couldn't answer").
  - `pub fn ask_stream(key: &str, history: &[Turn], on_delta: impl FnMut(&str)) -> Result<String, AskError>` → POST `https://generativelanguage.googleapis.com/v1beta/models/{MODEL}:streamGenerateContent?alt=sse` with header `x-goog-api-key` (never in the URL); returns the full text.
  - `pub fn validate_key(key: &str) -> Result<(), AskError>` → GET `.../v1beta/models/{MODEL}` with the header.
  - If a grounded request returns 400 mentioning `google_search`, retry once without `tools`.

- [ ] **Step 1: Write failing tests**

```rust
#[test] fn body_maps_roles_and_tools() { /* 2 turns → contents[0].role=="user", [1].role=="model"; tools[0] has google_search; system_instruction.parts[0].text == SYSTEM */ }
#[test] fn sse_line_extracts_text() { assert_eq!(parse_sse_line(r#"data: {"candidates":[{"content":{"parts":[{"text":"Back, "},{"text":"Forward"}]}}]}"#).as_deref(), Some("Back, Forward")); assert_eq!(parse_sse_line(""), None); }
#[test] fn classify_http_errors() { assert!(matches!(classify(None, ""), AskError::Offline)); assert!(matches!(classify(Some(400), "API key not valid"), AskError::BadKey)); assert!(matches!(classify(Some(403), ""), AskError::BadKey)); assert!(matches!(classify(Some(429), ""), AskError::Quota)); }
```

- [ ] **Step 2:** `vcargo test -p pad-core gemini` → FAIL.
- [ ] **Step 3:** Implement (ureq agent with 30 s read timeout; read the body line by line).
- [ ] **Step 4:** `vcargo test -p pad-core gemini` → PASS. Manual: `examples/gemini_probe.rs` reading the key from `keystore::get()`, prints a streamed answer to "what is a hadouken?".
- [ ] **Step 5:** Commit `feat(gemini): streaming client with grounded answers and readable errors`.

### Task 6: Dictation · live speech-to-text

**Files:**
- Create: `crates/pad-core/src/win/speech.rs`, `crates/pad-core/examples/dictation_probe.rs` (add features `"Media_SpeechRecognition"`, `"Globalization"`)

**Interfaces:**
- Produces: `pub enum SpeechEvent { Partial(String), Final(String), Error(&'static str) }`; `pub struct Dictation` with `pub fn start(on: impl Fn(SpeechEvent) + Send + Sync + 'static) -> Dictation` and `pub fn stop(self) -> String` (stops the continuous session, waits ≤ 1.5 s for the last result, returns the full transcript = finals + latest partial).
- Errors mapped: `0x80045509` (privacy policy not accepted) → "Turn on Online speech recognition in Windows Settings › Privacy › Speech"; no recognizer / no mic → "No microphone or speech language found".

- [ ] **Step 1:** Implement with `SpeechRecognizer::new()`, a `SpeechRecognitionTopicConstraint` (`Dictation`, "dictation"), `CompileConstraintsAsync`, `ContinuousRecognitionSession.StartAsync`, `HypothesisGenerated` → `Partial(accumulated finals + hypothesis)`, `ResultGenerated` → append, `Final`. COM: MTA on its own thread, created at `start`, dropped at `stop`.
- [ ] **Step 2:** `vcargo run -p pad-core --example dictation_probe` and speak "what's scorpion's spear combo" → partials print live, final line matches; with the Windows privacy toggle off it prints the mapped error, not a crash.
- [ ] **Step 3:** Commit `feat(speech): live dictation via Windows speech recognition`.

### Task 7: Overlay window and wiring

**Files:**
- Create: `app/src-tauri/src/overlay.rs`
- Modify: `app/src-tauri/src/main.rs` (dispatcher, commands), `app/src-tauri/src/ipc.rs` (key commands), `app/src-tauri/capabilities/default.json` (`"overlay"` window), `app/src-tauri/tauri.conf.json` (CSP `connect-src` unchanged: the WebView makes no Gemini calls)

**Interfaces:**
- Consumes: Tasks 2–6.
- Produces (Tauri commands, all called by overlay.js): `overlay_ask(text: String)`, `overlay_capture(on: bool)`, `overlay_close()`, `overlay_resize(height: f64)`, `overlay_focus()` (lets the field take real-keyboard focus), `overlay_drag_end(x: i32, y: i32)`; key: `gemini_key_status() -> bool`, `gemini_key_set(key: String) -> Result<(), String>` (normalize → validate → store), `gemini_key_clear()`, `gemini_open_studio()` (opens `https://aistudio.google.com/apikey`).
- Events to the `overlay` window: `ov_mode` `{ mode: "type" | "talk" }`, `stt` `{ text, final, error }`, `answer` `{ id, delta, done, error }`, `pad` (PadState, while captured), `ov_settings` `{ answer_voice }`.

- [ ] **Step 1: Write failing test** (pure state in `overlay.rs`):

```rust
#[test] fn talk_end_before_window_ready_still_sends() {
    let mut s = OverlayState::default();
    s.on_action(Action::TalkStart); s.on_action(Action::TalkEnd);
    assert_eq!(s.on_window_ready(), vec![Cmd::Show(Mode::Talk), Cmd::SendTranscript]);
}
#[test] fn tap_while_open_toggles_type_mode() { /* open(type) then OverlayTap → Cmd::Hide */ }
```

- [ ] **Step 2:** `vcargo test -p pad-companion overlay` → FAIL.
- [ ] **Step 3:** Implement. Window `overlay`: `transparent`, no decorations/shadow, `always_on_top`, `skip_taskbar`, `focused(false)`, `visible(false)` until `overlay_resize` first call; width 760 logical, height from JS; position lower centre of the work area of the monitor holding `GetForegroundWindow()` (or `overlay_pos`), 24 px above its bottom. Set `WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW` on the HWND after creation; `overlay_focus` removes `WS_EX_NOACTIVATE` and calls `SetForegroundWindow`; closing restores focus to the previous foreground HWND. `TalkStart` → `Dictation::start` (events → `stt`), `bridge.set_capture(true)`; `TalkEnd` → `stop()` → `overlay_ask(transcript)` path. `overlay_ask` → thread: `ask_stream` with the session history (kept in `OverlayState`, cleared on close) → `answer` events; then `set_capture(false)`. Window destroyed on `overlay_close` and 20 s after the last card faded with the pill hidden. Dispatcher: `BridgeEvent::Captured` → `pad` to `overlay`; `Action::{OverlayTap,TalkStart,TalkEnd}` → `OverlayState`.
- [ ] **Step 4:** `vcargo test -p pad-companion` and `vcargo check -p pad-companion` → PASS, no warnings.
- [ ] **Step 5:** Commit `feat(overlay): on-demand non-activating Gemini overlay window and wiring`.

### Task 8: Overlay UI · the pill, cards, voice, key setup

**Files:**
- Create: `app/ui/overlay.html`, `app/ui/css/overlay.css`, `app/ui/js/overlay/main.js`, `app/ui/js/overlay/cards.js`
- Assets: `app/ui/assets/fonts/GoogleSansFlex.ttf` (+ `GoogleSansFlex-OFL.txt`), `app/ui/assets/fonts/Noto-COLRv1.ttf` (+ `NotoEmoji-OFL.txt`), downloaded from the official `google/fonts` (`ofl/googlesansflex`) and `googlefonts/noto-emoji` (`fonts/Noto-COLRv1.ttf`) GitHub repos; verify each opens and its licence is OFL before committing.

**Interfaces:**
- Consumes: Task 7 commands/events.
- Produces: `cards.js` exports `createStack(el, { max = 3 })` → `{ add(id) , append(id, delta), done(id), error(id, msg), closeAll() }` and pure `fadeMs(text) = clamp(4000 + 60 * words, 6000, 20000)`.

- [ ] **Step 1: Write failing test** `app/ui/js/overlay/cards.test.mjs` (`node --test`): `fadeMs("one two") === 6000`; `fadeMs` of 400 words `=== 20000`; a stack with `max 3` after 4 adds keeps 3 elements and the oldest has class `old`.
- [ ] **Step 2:** `node --test app/ui/js/overlay/` → FAIL.
- [ ] **Step 3:** Implement. Pill and "Share screen with Live" chip exactly as the v2 mockup section 1 (sizes scaled to the 760 px window: big pill 132 px tall, chip 78 px; handle, plus, "Ask Gemini" 32 px Google Sans Flex, blue mic `#2b4c7e` → listening `#a8c7fa`, waveform). Stage-1 behaviour of Live chip, waveform and +: visible, show a one-line "Coming soon" hint (stages 3–4). Cards above the pill per spec (stack, `old` class, × button, bottom time line, fade with `fadeMs`). After send: pill hides, cards stay, `overlay_capture(false)`. Read-aloud: `speechSynthesis` when `answer_voice` is `Voice`/`Both` (Voice hides the card text, shows a small speaking chip). Key setup card when `gemini_key_status()` is false: copy from the spec ("what the key is for", free-tier limits, free-tier data note), buttons Open AI Studio / Paste (`navigator.clipboard.readText`) / Save; errors in one line. Report height with `overlay_resize` on every layout change (ResizeObserver). Drag via the handle (`startDragging`, then `overlay_drag_end`).
- [ ] **Step 4:** `node --test app/ui/js/overlay/` → PASS; `sh scripts/check-ui.sh` → ok (extend it to `js/overlay/*.js`). Headless render check with a stubbed `__TAURI__` (as done for tray.html): pill matches the mockup, no console errors.
- [ ] **Step 5:** Commit `feat(overlay-ui): Ask Gemini pill, cascading answer cards, read-aloud, key setup`.

### Task 9: Keyboard B · Orbit rail

**Files:**
- Create: `app/ui/js/overlay/orbit.js`, `app/ui/js/overlay/orbit.test.mjs`, `app/ui/js/overlay/words.js` (≈ 3k common English + gaming words, lower-case, frequency order)
- Modify: `app/ui/js/overlay/main.js`, `app/ui/css/overlay.css`

**Interfaces:**
- Produces: `createOrbit()` → `{ state, input(edge), text }` where `edge` ∈ `"left" | "right" | "A" | "B" | "X" | "Y" | "LB" | "RB" | "L3" | "MENU"`; pages `abc` (a–z), `123` (0–9 and `. , ? ! ' - : ( )`), `emoji` (40 Google emoji, gaming-friendly first: 😂 🔥 👍 💀 🎮 😭 …); `suggest(prefix, n = 4)` from `words.js`. `MENU` returns `{ send: text }`.

- [ ] **Step 1: Write failing tests** (`node --test`): starting on `abc` at `t`; `right` ×2 → focus `v`; `A` → text `v`; `B` → `""`; `X` → `" "`; typing `wa` then `Y` → text `"warden "` (first suggestion for "wa" in `words.js` must be a real word; assert `suggest("wa")[0]` is used); `RB` → page `123`, `RB` again → `emoji`, `LB` → `123`; wrap-around at both ends; `L3` toggles caps (`A` types `T`); `MENU` → `{ send }`.
- [ ] **Step 2:** `node --test app/ui/js/overlay/` → FAIL.
- [ ] **Step 3:** Implement logic; render per mockup section 2 (suggestion chips, sliding letter row with the big focus key, LB · abc · 123 · emoji · RB tabs). Pad input from `pad` events: edge-detect buttons; left stick / D-pad → `left`/`right` with repeat (first after 320 ms, then every 70 ms; faster beyond 85 % deflection: 40 ms). Real keyboard typing goes straight into the field.
- [ ] **Step 4:** `node --test app/ui/js/overlay/` → PASS; headless render shows the rail as in the mockup.
- [ ] **Step 5:** Commit `feat(overlay-ui): Orbit rail keyboard driven by the pad`.

### Task 10: Settings page, docs, end-to-end check

**Files:**
- Modify: `app/ui/js/pages/special.js` (Assistant options: "Gemini overlay" / "Gemini app" / "Off", text per option), `app/ui/js/pages/other.js` (Gemini section: key set/change/remove via the Task 7 commands, Answer voice Text/Voice/Both, "Reset overlay position"), `app/ui/js/home.js` or `other.js` (one-time "Assistant now opens the Gemini overlay" card until `overlay_intro_seen` (Task 1) is set; dismissing sets it), `README.md` (feature line + key setup), `scripts/check-ui.sh`.

- [ ] **Step 1:** Implement the settings UI; reuse `seg`/`toggle` helpers already used on those pages.
- [ ] **Step 2:** `vcargo test --workspace`, `node --test app/ui/js/overlay/`, `sh scripts/check-ui.sh` → all pass; headless sweep of every page (as in the earlier review) → no console errors.
- [ ] **Step 3:** Manual end-to-end with the pad over a borderless game and over the desktop (only after the user asks for a build): tap → type with the rail → Menu → card fades; hold → live words → release → card; Performance page read-outs unchanged with the overlay closed and open; Task Manager shows no overlay WebView after it closes.
- [ ] **Step 4:** Commit `feat(settings): Gemini overlay options, key management, answer voice`.
