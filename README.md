# Sayso

A macOS-only voice-to-text overlay app. Press a global shortcut, speak, and get a local transcript — no cloud, no account.

## How it works

1. Press **⌘+Shift+Space** anywhere on macOS to show a small floating, draggable overlay.
2. It records from your mic and shows a live amplitude display while you talk.
3. Press **⌘+Shift+Space** again (or click the stop button). The recording is transcribed **locally** (offline, via [whisper-rs](https://github.com/tazz4843/whisper-rs) / whisper.cpp — no audio leaves your machine), pasted into the app you were in (Terminal, Notes, Slack, …), and saved as a text file. The transcript also stays on your clipboard.
4. Transcripts land in `output/transcripts/`, one `.txt` file per recording, named by timestamp.

The Whisper model (`ggml-base.en.bin`, ~140MB) downloads automatically into the app's local data directory the first time you record — not committed to this repo.

## Status

- ✅ Global shortcut, draggable overlay, live amplitude bars
- ✅ Local transcription (offline Whisper) with output saved to `output/transcripts/`
- ✅ Pastes the transcript into the app that was focused when you started recording (clipboard + ⌘V; needs Accessibility permission)
- ⬜ Settings (hotkey, model, language), history view, auto-launch

See `plan.md` / `claudeplan.md` for the original project plan, and `docs/superpowers/` for the design spec and implementation plan behind the transcription feature.

## Running it

```bash
cd app
npm install
npm run tauri dev
```

Requires [Rust](https://www.rust-lang.org/tools/install), [CMake](https://cmake.org/) (`brew install cmake` — needed to build whisper.cpp), and Xcode Command Line Tools.

On first launch, macOS will prompt for microphone and accessibility permissions.

## Project layout

- `app/src/` — React frontend (overlay UI, mic capture)
- `app/src-tauri/src/` — Rust backend (global shortcut, tray, whisper-rs transcription, transcript output)
- `output/transcripts/` — generated transcripts (gitignored — personal voice data, not source)
- `docs/superpowers/` — design specs and implementation plans for individual features
