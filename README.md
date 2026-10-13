# Sayso

A macOS-only voice-to-text app. Press a global shortcut, speak, and your words are typed into whatever app you were using — transcribed locally, no cloud, no account.

## Tour

1. **Click into any text field** — Terminal, Notes, Slack, a browser address bar.
2. **Press ⌘⇧Space and speak.** A small floating overlay appears (drag it anywhere); the bars move with your voice.
3. **Press ⌘⇧Space again** (or click ■). Your speech is transcribed on this Mac — no audio leaves your machine.
4. **It's pasted where your cursor was.** It's also left on your clipboard, so ⌘V works if the paste didn't land. Every transcript is saved to `output/transcripts/` as well.

**First run:** a welcome window walks through these steps and shows whether **Microphone** and **Accessibility** are granted, with a Grant button for each. Reopen it any time from the menu-bar icon → **How to use Sayso**.

The Whisper model (`ggml-base.en.bin`, ~140MB) downloads automatically into the app's data directory the first time you record; after that everything works offline.

## Troubleshooting

| Symptom | Cause | Fix |
|---|---|---|
| Overlay says "Microphone access is off", or bars don't move | No Microphone permission | System Settings → Privacy & Security → Microphone → enable Sayso. In dev, enable the **terminal app** you ran `npm run tauri dev` from, then restart it. |
| Transcript isn't pasted | No Accessibility permission, or macOS refused to switch back to the target app | Press ⌘V — the transcript is on your clipboard. Check Privacy & Security → Accessibility. In dev, look for a `paste failed:` line in the terminal. |
| Nothing happens for silence | Expected | Silent recordings (Whisper's `[BLANK_AUDIO]` etc.) are dropped; focus returns to your app. |

## How it works

```
⌘⇧Space ─▶ remember frontmost app ─▶ show overlay ─▶ mic capture (Web Audio)
                                                          │
⌘⇧Space / ■ ◀─────────────────────────────────────────────┘
    │
    ▼
samples ─▶ Rust: resample to 16 kHz ─▶ Whisper (local) ─▶ drop non-speech markers
    │
    ├─▶ save output/transcripts/<timestamp>.txt
    └─▶ clipboard ─▶ re-activate remembered app ─▶ wait until frontmost ─▶ ⌘V
```

- The first shortcut press records which app was in front (`NSWorkspace.frontmostApplication`) before the overlay takes focus.
- The overlay captures raw audio in the webview; on stop, the samples go to the `transcribe_recording` command.
- Transcription runs `whisper-rs` (whisper.cpp) against a model held in memory.
- Pasting puts the text on the clipboard, re-activates the remembered app, and posts ⌘V only once that app is confirmed frontmost — so it never pastes into the wrong window. If there's nothing to paste, focus is still handed back.

| File | Responsibility |
|---|---|
| `app/src/App.tsx` | Overlay: amplitude bars, mic capture, stop handling |
| `app/src/Welcome.tsx` | Welcome tour and live permission status |
| `app/src-tauri/src/lib.rs` | Global shortcut, tray menu, window wiring, `stop_recording` |
| `app/src-tauri/src/audio.rs` | Resampling to 16 kHz |
| `app/src-tauri/src/transcribe.rs` | Whisper transcription, non-speech filtering |
| `app/src-tauri/src/model.rs` | Model download and location |
| `app/src-tauri/src/paste.rs` | Remembered target app, clipboard, activation, ⌘V |
| `app/src-tauri/src/welcome.rs` | First-launch detection, welcome window |
| `app/src-tauri/src/output.rs` | Writing transcript files |

## Status

- ✅ Global shortcut, draggable overlay, live amplitude bars
- ✅ Local transcription (offline Whisper) saved to `output/transcripts/`
- ✅ Pastes into the app that was focused when you started recording (clipboard + ⌘V; needs Accessibility)
- ⚠️ On recent macOS, re-activating some apps (seen with Chrome) can be refused — the transcript stays on the clipboard
- ✅ Welcome tour with live permission status
- ⬜ Settings (hotkey, model, language), history view, auto-launch

## Running it

```bash
cd app
npm install
npm run tauri dev
```

Requires [Rust](https://www.rust-lang.org/tools/install), [CMake](https://cmake.org/) (`brew install cmake` — needed to build whisper.cpp), and Xcode Command Line Tools.

Tests: `cd app/src-tauri && cargo test` (Rust) and `cd app && npm run build` (type-check).

To see the welcome window again as a first-time user, delete `~/Library/Application Support/com.sayso.app/welcome-seen` and relaunch.

## Project layout

- `app/src/` — React frontend (overlay, welcome tour, mic capture)
- `app/src-tauri/src/` — Rust backend (shortcut, tray, transcription, paste, welcome)
- `output/transcripts/` — generated transcripts (gitignored — personal voice data, not source)
- `docs/superpowers/` — design specs and implementation plans (local only; `*.md` is gitignored apart from this README)
- `plan.md` / `claudeplan.md` — the original project plan
