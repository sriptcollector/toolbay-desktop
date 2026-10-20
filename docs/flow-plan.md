# Toolbay Flow: status and plan

Toolbay Flow is the dictation tool built into the hub (`src-tauri/src/flow.rs`).
Design source: `toolbay-research/wispr-flow-report.pdf`, section 7.

## Works today (Windows prototype)

- Hold **Ctrl+Shift+Space**: records the default mic with `cpal` (any rate/format, mixed to mono).
- Release: resampled to 16 kHz, written to a temp WAV, transcribed by `whisper-cli`
  (whisper.cpp release b5454, CPU build) with `ggml-base.en` (~148 MB). A 4 s clip takes
  under a second on a Ryzen 9 5950X.
- Text goes on the clipboard, Ctrl+V is sent with `enigo`, then the previous clipboard text is restored.
- Model and binary download from the Flow page; nothing leaves the machine after that.
- On/off state persists; the tray keeps Flow alive when the window is closed.

## Next, in order

1. **Ctrl+Win hold** via a low-level keyboard hook (`SetWindowsHookEx(WH_KEYBOARD_LL)`), swallowing
   the Win key-up so the Start menu doesn't open. Configurable hotkey.
2. **Pill window**: bottom-centre, always-on-top, click-through, never focused (40x8 at rest,
   waveform while recording). Start/stop sounds.
3. **Speed**: link whisper.cpp in-process with `whisper-rs` (needs LLVM/libclang + CMake on the build
   machine), keep the model loaded, CUDA/Vulkan builds, and a model picker (tiny/base/small/turbo)
   chosen by hardware. Silero VAD to trim silence and for hands-free mode.
4. **macOS**: CGEventTap for Fn, Accessibility + Input Monitoring + Microphone prompts, Metal
   whisper build bundled (there is no prebuilt whisper-cli for macOS; today it uses one on PATH,
   e.g. `brew install whisper-cpp`, and the model download works the same).
5. **Paste reliability**: Shift+Insert fallback, delayed clipboard restore, confirm the text landed;
   test against ~30 common apps (terminals, Electron apps, browsers, Office, elevated windows).
6. **Cleanup pass**: optional local llama.cpp model or the user's own API key (Claude Haiku, etc.)
   to fix punctuation and filler words; dictionary and snippets; app-aware tone.
7. **History** in SQLite (text only by default, audio off).
