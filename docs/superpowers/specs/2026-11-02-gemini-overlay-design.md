# Gemini overlay: design

Date: 2026-11-02. Status: approved by the user 2026-11-02, with the two changes below (any app; exclusive fullscreen).

## Intent

Mid-game, controller in hand, the player wants a quick answer (a Mortal Kombat combo, how to get past a
puzzle, what an item does) or wants to control the PC ("mute Discord", "clip the last 30 seconds") without
reaching for the mouse or opening a browser. The Stadia pad's Assistant button summons a small dark Gemini
overlay at the lower centre of the screen, over the game. You talk or type, it answers in a line or two (or
does the PC task), and gets out of the way.

Pad Companion's promise holds throughout: it never adds lag to the pad and costs nothing while you play.

## What the player sees and does

The look is taken from the user's reference photo (`photo_2026-11-02_03-57-16.jpg`) and the approved mockup
`.superpowers/brainstorm/6133-1793588879/content/gemini-overlay-v2.html`.

### The pill

Two separate floating shapes, lower centre of the screen the game is on, nothing else:

- A smaller pill above, left-aligned to the big one: upload-screen icon + **"Share screen with Live"**.
- The big pill: a short drag handle at the top centre, **+** on the left, **"Ask Gemini"** placeholder, a
  filled blue circular mic button, and a Live waveform icon on the right.
- Dark glassy gradient (near-black navy, faint lighter rim at the bottom), soft edges, Google typography.

Behaviour:

- **Tap Assistant:** the pill opens with the text field active and keyboard B (below) under it.
- **Hold Assistant:** the pill opens (if closed) and the mic lights up (pale blue). Your words replace
  "Ask Gemini" as you speak, in real time. **Release sends.**
- **+** opens a small menu: Circle to Search, and (builder's call) anything else that fits.
- **Waveform** starts a Live voice conversation; **Share screen with Live** starts one that can see the screen.
- Close: Assistant tap while open with an empty field, **B** on an empty field, Esc, or after inactivity.

### Answers

- A short card appears above the pill. Gemini is asked for brief, gaming-focused answers (one to three
  sentences unless the question needs a list, e.g. a combo).
- **After you send, the pill tucks away and the pad goes straight back to the game** (user's decision); only
  the answer card stays on screen, without taking input.
- Cards fade on their own after a time based on their length; a thin line along the bottom shows the time
  left. **Players can also close them** (user's decision): a × on the card for the mouse; with the pad,
  opening the pill (Assistant tap) and pressing B on the empty field closes the cards with it (my call).
- Cascading: a new card pushes older ones back (slightly smaller and dimmer, up to about three visible).
- Answers can also be **read aloud**. Setting: Text / Voice / Both (default Text: the user said "either").

### PC commands

Spoken or typed. The command runs, a short chime plays, and a one-line confirmation shows ("✓ Discord muted"),
then fades. Commands the user named:

| Command | Notes |
|---|---|
| Mute / unmute Discord (or any app) | per-app mute of its audio session |
| Clip the last 30 seconds | the recorder's "record that" / instant replay (Game Bar, NVIDIA, Steam, AMD: reuse the Capture button's recorder choice) |
| Check my frame rate | see "Open questions for the builder" |
| Open Spotify (any app by name) | Start menu apps |
| Pause / play / next music | media keys |
| Take a screenshot | the existing Capture screenshot action |
| Volume up / down / mute | system volume |
| Search for pictures or files on the PC | results listed in the overlay, open with the pad; or an Explorer search |
| Pin to taskbar | Windows doesn't let apps pin programmatically; see open questions |
| Shut down / restart / sleep | **5-second countdown with "press B to cancel"**, so a misheard word can't end the session |
| Open File Explorer | |

"etc.": the same kind of everyday Windows control. The builder picks a sensible small set beyond the table
(e.g. lock, open Settings, Task Manager, brightness where supported) without turning this into a general
automation tool.

A phrase that matches a command runs locally with no network. Anything else goes to Gemini as a question.
Looser phrasings ("turn it down a bit") may be mapped to the same whitelisted commands by Gemini (function
calling); Gemini can never run anything outside the whitelist.

### Keyboard B · Orbit rail

Opens under the pill. Not a phone-style grid:

- One row of letters sliding under a big focus key. Left stick / D-pad glides, **A** types, **B** deletes,
  **X** space, **Y** takes the highlighted word suggestion, **LB / RB** switch letters · numbers · emoji,
  **Menu** sends, L3 caps.
- A row of word suggestions above the letters.
- Emoji page uses Google's emoji (Noto Color Emoji).
- A real keyboard and mouse work at all times too.

### Circle to Search

- Started from the pill's **+** menu (builder may add a shortcut).
- The screen freezes (a screenshot shown full-screen) and dims; a hint bar says "Circle anything to search ·
  Esc to cancel". The player draws a loop with the mouse; a Gemini-coloured trail follows it.
- The circled area (plus any question typed or spoken) goes to Gemini; the answer appears in the overlay's
  answer card with actions: **Ask a follow-up** (continues in the pill), **Continue in Gemini**, **Copy**.
- **Continue in Gemini:** copies the circled image and the question to the clipboard and opens Gemini (the
  installed Gemini app, else gemini.google.com in the default browser) so the player pastes with Ctrl+V. The
  API conversation can't appear in Gemini's own history: Google offers no way to do that. The user accepted
  this.

### Share screen with Live / Live voice

- A Live conversation with Gemini: you talk, it answers out loud, interruptible.
- With screen share: a thin blue frame around the screen and a small "● Live · sharing your screen" badge.
  Stop from the badge, the pill, or the Assistant button.
- Push-to-talk with Assistant hold inside Live is the builder's call (open mic with echo handling is the
  other option); either way the mic is never on without the visible Live badge.

### First use: the player's own Gemini key

Each player uses **their own free Gemini API key** (user's decision; the developer's key is never shipped).

- The first time something needs Gemini, the overlay shows a short setup card: what the key is for, an
  **Open AI Studio** button (aistudio.google.com/apikey), and a field to paste it (Ctrl+V, or the pad: a
  "Paste" action). It is checked with one cheap request before being saved.
- The card says plainly that the free tier has limits and that, on the free tier, Google may use what is sent
  to improve its products, so avoid sharing a screen with private things on it.
- Everything that doesn't need Gemini (speech-to-text, PC commands, keyboard) works without a key.
- The key is stored only on the PC, protected for the Windows user (Credential Manager / DPAPI), never in
  `settings.json`, logs or bug reports. Other settings gets a Gemini section: key (change / remove), answer
  voice (Text / Voice / Both).

## Goals

1. Ask Gemini by voice or pad-typing without leaving the game, with answers in a glance.
2. Control the PC by voice with instant, local commands.
3. Circle to Search and Live (voice and screen share) from the same pill.
4. Look exactly like the reference pill; keyboard B as mocked up.
5. Available to every Pad Companion user, safely (their own key).

## Non-goals

- Syncing overlay conversations into the Gemini app or gemini.google.com history (impossible; handoff instead).
- A general automation / macro system.
- Replacing the existing "open the Gemini app" behaviour: it stays as an option.

## Where it shows (user's changes at approval)

- **Over any app, not just games**: desktop, browser, anything. The Assistant button summons it wherever you are.
- **Over exclusive-fullscreen games too** (user's request). Stage 5, after stages 1–4. A plain always-on-top
  window can't appear over a true exclusive-fullscreen game, so this needs drawing inside the game's own
  frames (the technique Steam, Discord and OBS use: hooking the game's present call from a loaded module).
  Constraints on that stage:
  - Opt-in, off by default, with a clear warning: anti-cheat systems (Vanguard, Easy Anti-Cheat, BattlEye,
    etc.) may treat an unknown module in the game as cheating and ban the account.
  - Never loaded into games known to use kernel anti-cheat (a maintained skip list), and no attempt to hide
    from or evade anti-cheat: if a game refuses it, the overlay falls back.
  - Fallback everywhere it can't draw: voice answers and the chime still work (read-aloud forced on while
    the game is exclusive fullscreen).
  - Many games that say "fullscreen" already run flip-model / with Windows fullscreen optimisations, where
    the normal overlay window does show; stage 5 is only for the rest.

## Anti-goals (a failure even if it "works")

- **Any added latency or stutter on the pad.** The bridge's input path stays as it is.
- **Pad presses leaking into the game while typing**, or the game receiving half a button press when the
  overlay opens or closes.
- **Stealing focus so a game pauses, minimises or drops out of fullscreen** just because the overlay opened.
- Anything running, listening or capturing while the overlay is closed (mic, screen capture, network,
  WebView), or measurable CPU/GPU cost while gaming with it closed.
- A misheard word shutting down, restarting or sleeping the PC without a chance to cancel.
- A key ending up in the repo, the installer, logs or bug reports.
- Looking like a stock phone keyboard or a generic chat box instead of the reference pill.

## Constraints

- Builds on the existing app: Tauri 2 + WebView2 UI, Rust `pad-core` bridge, Windows 10 1903+.
- Windows that exist only while used, as with the main window and tray menu: the overlay, Circle to Search
  and Live windows are created on demand and destroyed after use.
- The Assistant button setting gains an option: **Gemini overlay** (new default) / Gemini app (current) /
  Off. The current "Gemini app" behaviour must keep working unchanged.
- While the overlay is taking pad input, the game sees a neutral, centred pad (all released). Handing input
  back must not send a phantom press (e.g. the A that typed the last letter).
- Google typography: **Google Sans Flex** (open source, SIL OFL) bundled with the app; **Noto Color Emoji**
  (OFL) bundled. Licences go next to the existing font licences.
- Gemini model ids are not hard-coded in more than one place; the builder picks a current fast model for
  answers and Circle to Search and the current Live model, and makes them easy to update.
- Network access goes only to Google's Gemini API endpoints (update the CSP accordingly).

## Decided by the user

- The pill exactly like the reference photo.
- Hold Assistant = live speech-to-text into the pill, release = send; tap = text field ready.
- Keyboard B (Orbit rail).
- Answers as short auto-dismissing, cascading cards; optional read-aloud.
- PC voice commands (the list above), with a chime and a one-line confirmation.
- Circle to Search answered in the overlay, with a handoff to the Gemini app / browser.
- Share screen with Live.
- Each user brings their own free Gemini API key, with guided setup.

## My calls (not confirmed by the user; easy to change)

- Default answer output: Text (Voice and Both selectable).
- 5-second cancellable countdown for shut down / restart / sleep.
- Local command matching first, Gemini function calling only for loose phrasings, whitelist only.
- Hold threshold for Assistant push-to-talk ≈ 300 ms (tap below that).
- Speech-to-text: Windows' built-in speech recognition (live partial results, no key, no cost). If it is
  unavailable (no speech language installed, or Windows' online speech recognition setting is off), the
  overlay says how to turn it on; the builder may add a fallback that sends the recording to Gemini after
  release (no live text in that case).
- Read-aloud uses Windows' built-in voices.
- Circle to Search starts from the + menu.
- Key storage in Windows Credential Manager (DPAPI-protected).
- A conversation lasts while the overlay is open: follow-ups keep context; closing the overlay starts fresh.
- Answers use Google Search grounding when the key's tier allows it (game facts change with patches).
- Speech recognition uses the Windows display language; Gemini answers in the language you asked in.
- The drag handle really drags (mouse); the position is remembered per screen, with a reset in settings.
- Upgrading users whose Assistant setting is "Gemini" move to **Gemini overlay** (the user wants everyone to
  get it); a one-time card explains it and where to switch back.
- Build in stages, each usable on its own: (1) pill, speech-to-text, answers, key setup, keyboard B;
  (2) PC commands; (3) Circle to Search; (4) Live voice and screen share; (5) exclusive fullscreen (opt-in).

## Left to the builder

- Exact window strategy for focus: e.g. a non-activating overlay while driven by the pad, taking keyboard
  focus only when the player clicks into the field or types on a real keyboard. Must satisfy the focus
  anti-goal.
- How the bridge hands pad input to the overlay (one cheap flag on the input path; neutral output while set).
- Word suggestions source (a bundled word list is fine).
- Exact card timings, stack depth, chime sound (original, short, quiet).
- How "search for pictures or files" shows results.
- Lasso smoothing, crop/mask of the circled area, screenshot method.
- Live audio pipeline (capture, playback, echo handling) and screen frame rate/size within free-tier limits.
- Error states: no internet, key invalid or over limit ("Gemini's free limit is used up for now, try again
  in a minute"), mic missing, no speech recognizer.

## Open questions for the builder (flagged, not blockers)

- **Check my frame rate:** measuring another game's FPS needs Windows event tracing (PresentMon-style) or a
  vendor overlay. Acceptable v1: toggle the Game Bar / NVIDIA / AMD performance overlay that the Capture
  recorder setting already points to, and say so; measuring FPS natively is a follow-up.
- **Pin to taskbar:** Windows blocks programmatic pinning by design. Acceptable v1: open the app's Start menu
  entry / location and say "Right-click it and choose Pin to taskbar" in the confirmation, rather than a
  fake success.

## Done means

- Every flow above works with the pad alone, over a borderless game, on a fresh install with a new key.
- Pad latency read-outs on the Performance page are unchanged with the overlay closed and while it is open.
- With the overlay closed: no extra processes, threads waking, mic or capture activity.
- Tests cover command matching, the shut down countdown/cancel, the Assistant tap/hold logic, input hand-off
  (no phantom presses), and key storage never touching settings or logs.
