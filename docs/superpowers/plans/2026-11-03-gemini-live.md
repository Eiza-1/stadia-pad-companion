# Gemini Live and Share screen Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Real-time voice conversation with Gemini (natural native-audio voice, captions, interruptible) from the overlay, with optional screen sharing, in the approved orb panel.

**Architecture:** Live is a new mode of the existing overlay window (`Mode::Live`), driven by the pure `OverlayState` machine. A Rust Live session (`pad-core/src/win/live_session.rs`) holds the WebSocket to Google, the mic (cpal), playback (cpal) and screen frames (GDI, 1 fps); the page only draws and gets level/caption events, so the key never reaches the WebView. A second, click-through full-screen window (`livedim`) draws the dim, red frame and chip; both windows are excluded from screen capture.

**Tech Stack:** Rust (pad-core, Tauri 2 app), `tungstenite` 0.28 (rustls, webpki roots), `cpal` (WASAPI), `jpeg-encoder`, `base64` 0.22, Win32 via `windows` 0.62; WebView2 UI (ES modules, node --test).

**Spec:** `docs/superpowers/specs/2026-11-03-gemini-live-design.md` (look: `.superpowers/brainstorm/1878-1793670620/content/live-orb-v4.html`)

## Global Constraints

- Never add latency to the pad: no changes to the bridge input path; Live never captures pad input.
- Nothing runs when Live is off: mic, capture, socket, playback, `livedim` window all closed on End, overlay close, app quit.
- The key only in the `x-goog-api-key` header or the WebSocket URL built in Rust; never logged (log the model and error kind, not the URL).
- Model ids in one place: `LIVE_MODEL = "gemini-3.1-flash-live-preview"`, `LIVE_FALLBACK = "gemini-3.8-live"` (verified working on the user's free key 2026-11-03).
- Mic out: 16 kHz, 16-bit mono PCM, `audio/pcm;rate=16000`. Gemini in: 24 kHz 16-bit mono PCM.
- Share screen: about 1 frame/s, JPEG, long side 1024 px, only while Share screen is on.
- Defaults: Live listening = Speakers; Live panel = Frosted; Live dim = Eases off.
- No outline around the panel; soft Google-colour edge light only.
- Settings copy and UI text in the app's existing plain voice; British spelling as elsewhere in the UI is not required (match existing strings).
- Commits: no AI attribution lines (user's standing rule).
- Do not build the installer unless the user asks.

## Review Focus

1. Ending Live while Gemini is mid-sentence: voice stops at once and nothing keeps playing (test: `player_clear_drops_queued_audio` in Task 4, `end_live_stops_everything` in Task 7).
2. Speakers mode self-interruption: the mic must stay closed while playback is queued, and for 300 ms after (test: `mic_gate_closes_while_gemini_speaks` in Task 3).
3. A single tap must still open the pill for typing (after the 300 ms double-tap window), and a hold must still dictate (tests in Task 2).
4. Google ending the session (goAway / close / quota): one clear line in the panel, everything closed, Live can start again (test: `server_close_ends_live_with_a_message` in Task 7; parse of goAway in Task 3).
5. Share screen off: no frame is ever sent (test: `no_frames_while_share_is_off` in Task 3).

---

### Task 1: Live settings

**Files:**
- Modify: `crates/pad-core/src/settings.rs`
- Modify: `app/ui/js/pages/other.js`

**Interfaces:**
- Produces: `pub enum LiveListen { Speakers (default), Headphones }`, `pub enum LiveGlass { Light, Frosted (default), Solid }`, `pub enum LiveDim { EasesOff (default), StaysDark }` (all `Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default`); fields `Settings.live_listen`, `Settings.live_glass`, `Settings.live_dim`. JSON values are the variant names ("Speakers", "Frosted", "EasesOff").

- [ ] **Step 1: Failing test** `live_defaults_are_speakers_frosted_eases_off` in settings tests: `Settings::default()` has `live_listen == Speakers`, `live_glass == Frosted`, `live_dim == EasesOff`; and `{"live_glass":"Light","live_dim":"StaysDark","live_listen":"Headphones"}` loads as those.
- [ ] **Step 2: Run** `vcargo test -p pad-core --lib settings` → compile error (no such types).
- [ ] **Step 3: Implement** the enums and fields (defaults as above).
- [ ] **Step 4: Run** → pass.
- [ ] **Step 5: Settings UI** in `geminiPanel()` after the Voice row: three `seg` rows saved with `save()`:
  - "Live listening" — Speakers / Headphones; small text: "Speakers: Gemini can't hear itself, hold Assistant to talk over it. Headphones: talk any time, like a call."
  - "Live panel" — Light glass / Frosted glass / Nearly solid; small text: "How see-through the Live panel is."
  - "Live dim" — Eases off / Stays dark; small text: "Eases off dims the screen as Live opens, then lightens it after 2 s so you can keep playing."
  - Keep them in sync in the existing `onSettings` callback.
- [ ] **Step 6: Run** `bash scripts/check-ui.sh` → `ui modules ok`.
- [ ] **Step 7: Commit** `feat(settings): Live listening, panel glass and dim options`

### Task 2: Double-tap starts Live

**Files:**
- Modify: `crates/pad-core/src/special.rs` (TapHold, Action)
- Modify: `crates/pad-core/src/hotkey.rs` (KeyHold)
- Modify: `crates/pad-core/src/win/hotkey_win.rs` (poll loop)
- Modify: `app/src-tauri/src/main.rs` (route `Action::LiveStart` like the other overlay actions)

**Interfaces:**
- Produces: `Action::LiveStart`; `pub const DOUBLE_TAP_MS: u64 = 300;` TapHold now defers `OverlayTap` until `DOUBLE_TAP_MS` after release with no second press; a second press within that window emits `LiveStart` at once (on press) and its release emits nothing. `TapHold::deadline()` also covers the pending tap. `TapHold::busy() -> bool` (a press or a pending tap). `KeyHold::active()` = `tap.busy()`; `KeyHold::pressed` must not clear a pending tap.

- [ ] **Step 1: Failing tests** in special.rs:
  - `tap_waits_for_a_possible_second_tap`: down 0, up 100 → `None`; `update(false, 399)` → `None`; `update(false, 400)` → `Some(OverlayTap)`; `deadline()` after the up = `Some(400)`.
  - `double_tap_starts_live_at_once`: down 0, up 100, down 250 → `Some(LiveStart)`; up 300 → `None`; `update(false, 1000)` → `None`.
  - `hold_still_talks`: down 0, `update(true, 300)` → `TalkStart`, up 900 → `TalkEnd` (unchanged behaviour).
  - Update the existing Special tests that expect `OverlayTap` on release to advance time past the window (e.g. call `s.update(&b(empty), t + 300)`).
  - hotkey.rs `key_double_tap_starts_live`: `pressed(0)`, `poll(100, false)` → None, `active()` true; `pressed(200)` (second WM_HOTKEY) or `poll(200, true)` → `Some(LiveStart)`; then `poll(260, false)` → None and `active()` false.
- [ ] **Step 2: Run** → fail.
- [ ] **Step 3: Implement.** In hotkey_win.rs keep one `KeyHold` across messages and poll while `hold.active()` (the loop already polls; it now also runs through the 300 ms wait after a release, detecting the second press with `GetAsyncKeyState`). Bridge needs no change: it already wakes for `special.next_deadline()`.
- [ ] **Step 4: Run** `vcargo test -p pad-core --lib` → pass.
- [ ] **Step 5: Commit** `feat(special): double-tap Assistant or the shortcut starts Live`

### Task 3: Live protocol, audio maths and gates (pure)

**Files:**
- Create: `crates/pad-core/src/live.rs` (+ `pub mod live;` in lib.rs)

**Interfaces:**
- Produces:
  - `pub const LIVE_MODEL: &str`, `pub const LIVE_FALLBACK: &str`, `pub const MIC_RATE: u32 = 16_000`, `pub const OUT_RATE: u32 = 24_000`, `pub const LIVE_SYSTEM: &str` (gaming assistant, one to three short spoken sentences, never read symbols or lists literally, answer in the player's language).
  - `pub fn ws_url(key: &str) -> String` → `wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent?key=…`
  - `pub fn setup_msg(model: &str, voice: &str, grounded: bool, resume: Option<&str>) -> Value`: `setup.model = "models/<model>"`, `generationConfig.responseModalities = ["AUDIO"]`, speechConfig prebuilt voice, `systemInstruction`, `inputAudioTranscription: {}`, `outputAudioTranscription: {}`, `contextWindowCompression: {slidingWindow: {}}`, `sessionResumption: {handle?}`, `tools: [{google_search:{}}]` when grounded.
  - `pub fn audio_msg(pcm: &[i16]) -> Value` → `{"realtimeInput":{"audio":{"data":b64(le bytes),"mimeType":"audio/pcm;rate=16000"}}}`
  - `pub fn frame_msg(jpeg: &[u8]) -> Value` → `{"realtimeInput":{"video":{"data":b64,"mimeType":"image/jpeg"}}}`
  - `pub enum LiveEvent { SetupComplete, Audio(Vec<i16>), YouSaid(String), GeminiSaid(String), Interrupted, TurnComplete, GoAway, Resume(String), Error(String) }`
  - `pub fn parse_server(text: &str) -> Vec<LiveEvent>` (one message may carry several: audio parts, transcriptions, flags).
  - `pub fn to_mono_16k(samples: &[f32], channels: u16, rate: u32) -> Vec<i16>` (average channels, linear resample, clamp).
  - `pub fn resample_24k_to(pcm: &[i16], rate: u32) -> Vec<f32>` (for the output device).
  - `pub fn level(pcm: &[i16]) -> f32` (RMS 0..1, scaled so normal speech ≈ 0.3–0.8: `(rms / 8000).min(1.0)`).
  - `pub fn mic_open(listen: LiveListen, muted: bool, talk_over: bool, last_audio_ms: Option<u64>, now_ms: u64) -> bool`: false if muted; Headphones → true; Speakers → `talk_over || last_audio_ms.map_or(true, |t| now_ms >= t + 300)`.
  - `pub fn frame_due(sharing: bool, last_ms: Option<u64>, now_ms: u64) -> bool`: false if !sharing; else `last_ms.map_or(true, |t| now_ms >= t + 1000)`.
- [ ] **Step 1: Failing tests** (`live.rs` tests): `setup_has_model_voice_transcripts_and_resumption` (asserts the paths above; `tools` only when grounded; `sessionResumption.handle` set when `resume` is `Some("h1")`); `audio_msg_is_little_endian_base64` (`[1, -2]` → bytes `01 00 fe ff`); `parse_audio_and_captions` (a serverContent with one inlineData part of base64 `[1,0,2,0]`, `outputTranscription.text "Hi"`, `inputTranscription.text "where"` → `[Audio([1,2]), GeminiSaid("Hi"), YouSaid("where")]` in that order); `parse_flags` (`interrupted:true` → `Interrupted`; `turnComplete:true` → `TurnComplete`; `{"setupComplete":{}}` → `SetupComplete`; `{"goAway":{"timeLeft":"5s"}}` → `GoAway`; `{"sessionResumptionUpdate":{"newHandle":"h2","resumable":true}}` → `Resume("h2")`; garbage → `[]`); `stereo_48k_becomes_mono_16k` (4800 stereo frames of 0.5 → 1600 samples ≈ 16383); `level_of_silence_is_zero_and_loud_is_one`; `mic_gate_closes_while_gemini_speaks` (Speakers, last audio at 1000: closed at 1000 and 1299, open at 1300; talk_over opens it; muted always closed; Headphones open while speaking); `no_frames_while_share_is_off` (false whatever the times; on: true first, false at +999, true at +1000).
- [ ] **Step 2: Run** → fail. **Step 3: Implement.** **Step 4: Run** `vcargo test -p pad-core --lib live` → pass.
- [ ] **Step 5: Commit** `feat(live): Gemini Live protocol, audio conversion and mic/frame gates`

### Task 4: Audio in and out

**Files:**
- Create: `crates/pad-core/src/win/audio.rs` (+ `pub mod audio;`)
- Modify: `crates/pad-core/Cargo.toml` (add `cpal`)
- Create: `crates/pad-core/examples/audio_probe.rs`

**Interfaces:**
- Consumes: `live::to_mono_16k`, `live::resample_24k_to`.
- Produces:
  - `pub struct Mic` — `Mic::start(on_chunk: impl FnMut(Vec<i16>) + Send + 'static) -> Result<Mic, String>`; chunks of ~100 ms (1600 samples) of 16 kHz mono; capture stops when `Mic` is dropped. Error text for "no microphone" / "mic blocked in Windows privacy settings".
  - `pub struct Player` — `Player::start() -> Result<Player, String>`; `push(&self, pcm24k: &[i16])`; `clear(&self)` (drops everything queued, silence at once); `queued_ms(&self) -> u64`; `level(&self) -> f32` (of what is playing now); stops on drop.
- [ ] **Step 1: Failing test** `player_clear_drops_queued_audio` against the pure queue type the player uses (`AudioQueue { push, pull(n) -> Vec<f32>, clear, queued_ms }` in audio.rs, no device): push 1 s at 24 kHz, `queued_ms ≈ 1000`, `clear()`, `queued_ms == 0`, `pull` returns silence.
- [ ] **Step 2: Run** → fail. **Step 3: Implement** queue + cpal streams (default input/output device, f32 or i16 formats). **Step 4: Run** → pass.
- [ ] **Step 5: Manual probe** `audio_probe`: records 3 s, prints the peak level, plays it back. Expected: peak level > 0.05 when speaking.
- [ ] **Step 6: Commit** `feat(audio): mic capture at 16 kHz and interruptible playback`

### Task 5: Screen frames

**Files:**
- Create: `crates/pad-core/src/win/screen.rs` (+ `pub mod screen;`)
- Modify: `crates/pad-core/src/win/overlay_win.rs` (add `exclude_from_capture(hwnd: isize)` via `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)`; `click_through(hwnd: isize)` adding `WS_EX_TRANSPARENT | WS_EX_LAYERED | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW`)
- Modify: `crates/pad-core/Cargo.toml` (add `jpeg-encoder`)

**Interfaces:**
- Produces: `pub fn capture_jpeg(monitor_of: isize, long_side: u32) -> Option<Vec<u8>>` (GDI BitBlt of the monitor showing `monitor_of`, scaled to `long_side`, JPEG quality 70); pure `pub fn fit(w: u32, h: u32, long_side: u32) -> (u32, u32)`.
- [ ] **Step 1: Failing test** `fit_keeps_aspect`: `fit(1920,1080,1024) == (1024,576)`, `fit(800,600,1024) == (800,600)`.
- [ ] **Step 2–4:** run fail → implement → pass; plus a test `capture_returns_a_jpeg` that calls `capture_jpeg(0, 1024)` and checks the bytes start with `FF D8` (works on the dev desktop).
- [ ] **Step 5: Commit** `feat(screen): 1024 px JPEG frames of the game's screen; hide own windows from capture`

### Task 6: Live session

**Files:**
- Create: `crates/pad-core/src/win/live_session.rs` (+ `pub mod live_session;`)
- Modify: `crates/pad-core/Cargo.toml` (add `tungstenite = { version = "0.28", features = ["rustls-tls-webpki-roots"] }`)
- Create: `crates/pad-core/examples/live_probe.rs`

**Interfaces:**
- Consumes: Task 3 messages/gates, Task 4 `Mic`/`Player`, Task 5 `capture_jpeg`.
- Produces:
  - `pub struct LiveConfig { key: String, voice: String, listen: LiveListen, monitor_of: isize }`
  - `pub enum LiveUpdate { Connecting, Ready, Level { you: f32, gemini: f32 }, YouSaid(String), GeminiSaid(String), TurnDone, Speaking(bool), Ended(Option<&'static str>) }` (`Ended(Some(msg))` for errors: "Gemini ended the Live session (free limit). Double-tap Assistant to start again.", "No internet connection", "Your Gemini key isn't working. Check it in Settings", mic errors).
  - `pub struct LiveSession` — `start(cfg, on: impl Fn(LiveUpdate) + Send + Sync + 'static) -> LiveSession`; `set_muted(bool)`, `set_talk_over(bool)`, `set_sharing(bool)`, `end()`; dropping ends it. Everything (socket, mic, player, capture) is owned by its thread and closed when it returns.
- Behaviour: connect `LIVE_MODEL`, fall back to `LIVE_FALLBACK` if the socket closes before `setupComplete`; first try grounded, and if the server closes with a message mentioning `google_search`, reconnect ungrounded. Socket read timeout 20 ms; loop: drain mic chunks (send if `mic_open`), send a frame if `frame_due`, read server events (Audio → `player.push`; Interrupted → `player.clear()`; Resume → keep handle; GoAway → reconnect with the handle), emit `Level` at most every 33 ms.
- [ ] **Step 1: Manual probe** `live_probe` (with the saved key): `--text "what is a hadouken"` sends one text turn and prints captions plus "first audio after N ms"; `--talk 8` opens the mic for 8 s. Expected: a spoken answer through the speakers and a caption.
- [ ] **Step 2: Commit** `feat(live): Live session thread with mic, playback, screen frames and resume`

### Task 7: Overlay state and glue for Live

**Files:**
- Modify: `app/src-tauri/src/overlay.rs` (state machine + glue + commands)
- Modify: `app/src-tauri/src/main.rs` (route `LiveStart`; register commands; capability windows)
- Modify: `app/src-tauri/capabilities/default.json` (add `"livedim"`)
- Create: `app/ui/livedim.html`, `app/ui/css/livedim.css`, `app/ui/js/livedim.js`

**Interfaces:**
- Consumes: `Action::LiveStart`, `LiveSession`, `LiveUpdate`, `overlay_win::{exclude_from_capture, click_through}`.
- Produces: `Mode::Live` (serialised "live"); `Cmd::LiveStart`, `Cmd::LiveEnd`, `Cmd::LiveTalk(bool)`; OverlayState `on_live_ended()`; Tauri commands `live_start(share: bool)` (the pill's waveform button = false, "Share screen with Live" = true; runs `on_action(LiveStart)` and turns sharing on once ready), `live_mute(on: bool)`, `live_share(on: bool)`, `live_end()`, `live_glass(rect: [f64;4], radius: f64)` (page reports the panel's rect for the acrylic region); page events `ov_mode {mode:"live"}`, `live {state, you?, gemini?, level_you?, level_gemini?, sharing?, error?}`; dim window events `dim {sharing: bool, dim: "EasesOff"|"StaysDark"}`.
- State rules (tests first, in overlay.rs tests):
  - `double_tap_opens_live`: closed → `on_action(LiveStart)` = `[Open]`; `on_window_ready()` → `[Show(Live), LiveStart]`. Ready + pill shown → `[Show(Live), Capture(false), LiveStart]` (Live never holds the pad).
  - `tap_during_live_ends_it`: in Live, `OverlayTap` → `[LiveEnd, Hide]`.
  - `hold_during_live_talks_over`: in Live, `TalkStart` → `[LiveTalk(true)]` (no StartListening, no Capture); `TalkEnd` → `[LiveTalk(false)]`.
  - `end_live_stops_everything`: `on_closed()` while in Live emits nothing further and resets; glue drops the session (checked in the manual run).
  - `server_close_ends_live_with_a_message`: `on_live_ended()` in Live → `[Hide]` and state no longer Live; a later `LiveStart` starts again.
  - `pad_gone_does_not_end_live`: `on_pad_gone()` in Live → `[]`.
- Glue: `Cmd::LiveStart` → key check (no key: show type mode with the key card, as typed questions do); create `livedim` window (full monitor of the game, transparent, click-through, excluded from capture, `livedim.html`); exclude the overlay window from capture; start `LiveSession`, forwarding `LiveUpdate`s to the page as `live` events; `LiveEnd`/close/quit → drop session, destroy `livedim`. While in Live the idle-close timer of the page must not run.
- [ ] **Steps:** failing tests → run → implement state → glue → `vcargo check -p pad-companion` clean → `vcargo test --workspace` pass → commit `feat(overlay): Live mode, dim window and session glue`.

### Task 8: Live panel UI

**Files:**
- Modify: `app/ui/overlay.html`, `app/ui/css/overlay.css`, `app/ui/js/overlay/main.js`
- Create: `app/ui/js/overlay/live.js`, `app/ui/js/overlay/live.test.mjs`
- Modify: `app/src-tauri/src/overlay.rs` (acrylic on/off + rounded window region via `live_glass`)

**Interfaces:**
- Consumes: page events from Task 7; settings `live_glass`, `live_dim`, `overlay_anim`, `overlay_scale`.
- Produces: `live.js` pure model `createCaptions({ now })` with `you(text)`, `gemini(text)`, `turnDone()`, `interrupted()`, `view() -> { you: string, current: string, older: string }` (current = last ~12 words of Gemini's turn, older = the rest of the last two lines dimmed; a new turn clears the older one after `turnDone`).
- Look (copy from `live-orb-v4.html`): panel 560 x 470 at scale 1, radius 44, bottom-anchored where the pill is; spring easing `linear(...)` from the mockup, 0.7 s; orb = five blurred colour blobs (red #ea4335, yellow #fbbc04, green #34a853, blue #4285f4, purple #8f6bff) inside a 170 px sphere with shade + specular, `--lv` drives scale and halo, rings on level > 0.62 (at most every 380 ms); no outline; edge light gradients as in the mockup; glass presets `Light [.10,.26]`, `Frosted [.32,.5]`, `Solid [.78,.88]` (tint, edge tint). Window acrylic (Tauri `Effect::Acrylic` via `set_effects`) for Frosted and Solid, applied after the entrance finishes and limited to the panel's rounded rect (`SetWindowRgn(CreateRoundRectRgn)` from the page-reported rect); Light = no acrylic. Off on End.
- First Share screen ever: the chip reads "Sharing screen with Gemini · don't show private things" for 6 s, then the short text; remembered in the livedim page's `localStorage` (`shareNoteSeen`).
- livedim page: full-screen dim (`radial-gradient(70% 70% at 50% 75%, rgba(4,5,9,.35), rgba(4,5,9,.78))`, opacity 1, or eases to .45 after 2 s), red frame (`inset 0 0 0 3px rgba(242,139,130,.85)`) and "Sharing screen with Gemini" chip while sharing.
- [ ] **Step 1: Failing tests** in `live.test.mjs`: `captions_keep_the_current_words_bright` (gemini "a b c … o" (15 words) → current = last 12, older = first 3); `turn_done_then_new_turn_clears_old` ; `interrupted_keeps_what_was_said` ; `you_line_shows_latest_words`.
- [ ] **Step 2–4:** run fail → implement → pass (`node --test "app/ui/js/overlay/*.test.mjs"`).
- [ ] **Step 5:** headless check with the stub harness: fire `ov_mode {mode:"live"}` and `live` events; screenshots of the panel at Light/Frosted/Solid; no page errors; `overlay_resize` reports the panel height.
- [ ] **Step 6: Commit** `feat(overlay-ui): Live panel with the Google orb, captions and glass`

### Task 9: Whole-branch check

- [ ] `vcargo test --workspace`, `node --test "app/ui/js/overlay/*.test.mjs"`, `bash scripts/check-ui.sh` all green.
- [ ] `live_probe --text` speaks an answer; `audio_probe` records and plays.
- [ ] Fresh-context review of the branch (one reviewer), fix Critical/Important RED→GREEN.
- [ ] Do not build the installer; tell the user what to try once they ask for a build.
