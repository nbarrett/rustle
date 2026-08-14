# Rustle

Open-source, local, **Rust** dictation. A Wispr Flow replacement you own.

Your voice is transcribed by Whisper **on your own machine** and never leaves it, so it is private, free, and impossible to rate-limit. The goal is the Wispr Flow experience: press a hotkey, talk, and the text appears in whatever app you are using.

> Working codename. Rename freely.

## Status: Milestone 0 (the spike)

Right now it does the smallest useful thing, to prove the pipeline works end to end:

**run it → it records from the mic until you press Enter → Whisper transcribes on-device → it prints the text.**

No global hotkey, no typing into other apps, no menu-bar app yet. Those come next (see the roadmap).

## Prerequisites (macOS)

- **Rust** via [rustup](https://rustup.rs)
- **Xcode command line tools:** `xcode-select --install`
- **cmake:** `brew install cmake` (whisper-rs compiles whisper.cpp under the hood)

## Setup and run

```bash
# 1. Grab a Whisper model (base.en is small and fast for the spike)
./download-model.sh base.en

# 2. Build and run
cargo run

# 3. Talk, then press Enter. The transcript prints to the terminal.
```

For better accuracy later, swap the model:

```bash
./download-model.sh large-v3
RUSTLE_MODEL=models/ggml-large-v3.bin cargo run
```

## Roadmap

1. **Spike** *(you are here)* — hotkey-free record → transcribe → print. Prove the ears and brain work.
2. **Type anywhere** — inject the transcript into whatever app is focused (Claude, Slack, Mail, a browser, your editor), triggered by a **global hotkey** from anywhere. This is the Wispr Flow "works in every app" behaviour, via `enigo` + macOS Accessibility. Needs a one-time grant of **Accessibility** and **Input Monitoring** in System Settings, exactly like Wispr Flow.
3. **Cleanup pass** — run the raw transcript through a local LLM (Ollama) to strip filler, fix punctuation, and apply a custom vocabulary, so "Bamba" becomes "bandwagon".
4. **Menu-bar app** — wrap it in a small [Tauri](https://tauri.app) tray app with settings and a model picker.
5. **Ship it** — MIT on GitHub, so no one can ever switch it off.

## Why this exists

Whisper (the speech model) is open and runs beautifully offline on Apple Silicon. What you pay a subscription for is really just a tidy hotkey, a cleanup pass, and a nice wrapper. That is a genuinely lovely bit of Rust, and it means nobody can ever cap your own dictation again.

Prior art worth reading: [Handy](https://github.com/cjpais/Handy) already does much of this in Rust + Tauri.

## Licence

MIT. See [LICENSE](LICENSE).
