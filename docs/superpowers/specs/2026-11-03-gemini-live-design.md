# Gemini Live and Share screen: design

Date: 2026-11-03. Status: draft for the user's approval. Stage 4 of
`2026-11-02-gemini-overlay-design.md` (its section "Share screen with Live / Live voice"), brought forward
by the user ahead of PC commands and Circle to Search. Mockup the user approved ("it's good to go"):
`.superpowers/brainstorm/1878-1793670620/content/features-v1.html`, section 3.

## Intent

Mid-game, the player wants to *talk* to Gemini like a friend on the couch, not type questions: "where's the
lever?", "what should I upgrade first?", follow-ups, interruptions, back and forth. With Share screen on,
Gemini can see the game and answer about what is on screen right now. The user's words: "make that Gemini
feature of my app work", and earlier "implement share screen with live gemini".

It must sound natural: the user rejected robotic voices outright ("they sound so fake and unnatural").
Gemini Live's native audio voices are the bar.

## What the player sees and does

### Starting Live

- **The waveform button** in the pill starts Live (voice only).
- **"Share screen with Live"** (the small pill above the big one) starts Live with Share screen on.
- **Double-tap Assistant** starts Live from anywhere, with or without the pill open (mockup). My call: the
  second tap must come within 300 ms; this delays the plain tap (open the pill for typing) by up to 300 ms,
  which is acceptable because opening the pill is not time-critical. Hold (push-to-talk dictation) is
  unchanged and is recognised as before (≥ 300 ms).
- **The keyboard shortcut** (Ctrl+Alt+G by default): double-tap starts Live the same way.
- No key saved yet: the existing key setup card shows instead, as for typed questions.

### During Live

- The big pill turns into the **Live pill** (mockup): Gemini sparkle and "Live" on the left, a waveform in
  the middle that moves with whoever is speaking (Gemini's voice, or the player's mic level), and three round
  buttons on the right: **Mute mic**, **Share screen** (toggle), **End** (red).
- A soft blue glow breathes behind the pill while Live is on (the entrance animation setting applies: Off
  means no breathing either).
- **Captions**: what Gemini says appears as text above the pill, a line or two at a time, newest words
  brightest, older lines fading (mockup). What the player said may show dimmer above it (my call: yes,
  one line, from the API's input transcription).
- **The game keeps the pad.** Live does not capture pad input; the player keeps playing while talking. The
  keyboard rail is not shown in Live.
- Assistant button during Live: **tap = end Live**; **hold = talk over Gemini** (see "Echo and
  interrupting"). Shortcut: same.
- The Live pill's buttons work with the mouse. Pad control of those buttons is not needed (my call: Assistant
  tap ends; Share screen and Mute are mouse-only in v1).

### Share screen

- When on: a **red frame** around the edge of the shared screen and a **"Sharing screen with Gemini"** chip
  at the top centre (mockup). Both are drawn by Pad Companion, are never sent to Gemini themselves, and must
  not take focus or clicks from the game.
- What is shared: the screen the game is on (the screen under the pill). About **one picture a second**,
  scaled down (builder's call, within the API's limits; about 1024 px on the long side is a good start).
- Turning Share screen off stops capture at once; turning it on again resumes without restarting Live.
- The privacy line already on the key setup card ("don't share a screen with private things on it") also
  shows the first time Share screen is used (my call: once, as a one-line note in the chip).

### Ending

- End button, Assistant tap, Esc while the overlay has focus, or the session ending on Google's side.
- After ending: the pill tucks away, the overlay closes after its usual idle time, and **nothing keeps
  running** (mic, capture, network connection, audio output all closed).
- Ending mid-sentence stops Gemini's voice immediately.

## Echo and interrupting (my call, with a setting)

With speakers, Gemini hears its own voice through the mic and interrupts itself. Two modes, chosen in
Settings › Other › Gemini › **Live listening**:

- **Headphones (open mic)**: the mic is always open, like a phone call; the player can interrupt Gemini
  mid-sentence just by talking (Gemini Live's own voice activity detection).
- **Speakers (default)**: the mic is muted while Gemini is speaking and opens again when it finishes; to
  interrupt, hold Assistant (or the shortcut) and talk. The Live pill shows which state the mic is in.

The builder should first try Windows' own echo cancellation (capturing and playing in the "communications"
audio category, which Windows 11 cleans up with Voice Clarity). If that reliably stops self-interruption
on plain laptop speakers, the default may become open mic and the setting can say so. Either way the mic is
never on without the Live pill visible.

## Voice and language

- Gemini Live native audio, in the voice chosen in Settings › Gemini › Voice (the same six names the read-
  aloud setting already offers; all are Live voices).
- Gemini answers in the language the player speaks.
- Live gets the same short, gaming-focused instructions as typed questions ("one to three short
  sentences…"), adapted for speech (no lists read out symbol by symbol).
- Google Search grounding if the key's tier allows it in Live; if Google refuses it, Live runs without it
  (same rule as typed answers).

## Goals

1. Talk to Gemini hands-free while playing, with natural, interruptible speech and captions.
2. Share the game screen so answers are about what is on screen now.
3. Start in one move (double-tap Assistant, or one click), end in one move.
4. Nothing running when Live is not on; no effect on pad latency at any time.

## Non-goals

- Video from a webcam.
- Recording or saving Live conversations, or syncing them into the Gemini app / gemini.google.com.
- Pad navigation of the Live pill's buttons (v1).
- Sharing a different screen or a single window than the one the game is on (v1).
- Exclusive-fullscreen games (stage 5): Live voice still works there; the pill and captions won't show.

## Anti-goals (a failure even if it works)

- **Any added latency or stutter on the pad**, or the game losing focus, pausing or leaving fullscreen
  because Live started, ended or showed captions.
- The mic, screen capture or connection still running after Live ends, the overlay closes, or the app quits.
- Gemini endlessly interrupting itself on speakers.
- Gemini's voice continuing after the player pressed End.
- The red frame, chip or captions ending up in the frames sent to Gemini (it would describe its own UI).
- The key appearing in logs, URLs that get logged, or error messages.
- A robotic or Windows voice anywhere in Live.

## Constraints

- Builds on the existing overlay (Tauri 2 window `overlay`, `overlay.rs` state machine, `pad-core`), Windows
  10 1903+; the Live session runs in Rust (as the typed answers do), not in the WebView, so the key stays out
  of the page.
- Gemini Live API (WebSocket, `BidiGenerateContent`) with the player's own key. Verified 2026-11-03 with the
  user's free key: `gemini-3.8-live`, `gemini-2.5-flash-native-audio-latest` and
  `gemini-3.1-flash-live-preview` all accepted a session and replied with natural speech plus a transcript;
  first audio 2.2–3.7 s after connecting. The model id lives in one constant; the builder picks the best of
  these (or newer) and keeps a fallback.
- Free-tier limits: Google limits how long a Live session runs and how many run at once (audio+video sessions
  are much shorter unless context compression is on). The builder enables whatever keeps a session going
  (context window compression, session resumption on "go away") and, if Google ends it anyway, shows one
  clear line ("Gemini ended the Live session (free limit). Double-tap Assistant to start again.").
- Mic: 16 kHz 16-bit mono PCM to Google; Gemini's audio comes back as 24 kHz PCM. Playback must start as
  soon as the first chunk arrives and must stop instantly on interrupt or End.
- New dependencies are fine where they save real work (a WebSocket client with TLS; an audio I/O library);
  each must be small, maintained and Windows-supported.
- Network access only to Google's Gemini API endpoints.
- The existing behaviour stays: tap/hold Assistant for typed and dictated questions, answer cards, read-aloud.

## Decided by the user

- Gemini Live and Share screen come next, before Circle to Search and "+" (2026-11-03, "it's good to go"
  on the features mockup, Live first).
- The look in the features mockup, section 3: Live pill with waveform, mute / share / end buttons, captions
  above, red frame and "Sharing screen with Gemini" chip.
- Natural voices only; no robotic speech.
- Each player uses their own free key.

## My calls (not confirmed by the user; easy to change)

- Double-tap Assistant (and the shortcut) starts Live; tap during Live ends it; hold during Live talks over.
- Speakers mode (mic muted while Gemini talks) is the default until echo cancellation is proven.
- Captions on, including one dim line of what the player said.
- Share screen captures the screen the game / pill is on, about one frame a second, ~1024 px.
- First Share screen shows the privacy note once.
- Live uses the read-aloud voice setting.

## Left to the builder

- Audio library and exact capture/playback buffering; resampling the mic to 16 kHz.
- Screen capture method (Windows Graphics Capture or GDI) as long as Pad Companion's own red frame, chip,
  pill and captions are excluded from the frames (e.g. exclude those windows from capture, or capture before
  drawing them).
- WebSocket threading, reconnect/resume details, and the exact message flow.
- Waveform drawing and caption timing.
- How the Live state fits the existing `OverlayState` (it should stay a pure, tested state machine).

## Done means

- From a fresh install with a new free key: double-tap Assistant over a borderless game, ask a question out
  loud, hear a natural answer within about 3 seconds, interrupt it (hold Assistant on speakers, or just talk
  on headphones), turn Share screen on and ask "what's on my screen?" and get a correct answer, press End.
- After End: no mic indicator in Windows, no capture, no open connection, no audio.
- Pad latency read-outs unchanged before, during and after Live.
- Tests cover: the Live states and transitions (start, mute, share on/off, interrupt, end, Google ending the
  session, pad disconnect), double-tap vs tap vs hold, the message building/parsing for the Live API, mic
  resampling, and that no frame is sent while Share screen is off.
