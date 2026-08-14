# Rustle

Open-source, local, **Rust** dictation. A Wispr Flow replacement you own.

Your voice is transcribed by Whisper **on your own machine** and never leaves it, so it is private, free, and impossible to rate-limit. The experience is the Wispr Flow one: hold a hotkey, talk, and the text appears in whatever app you are using.

> Working codename. Rename freely.

## Status: menu-bar app with a settings panel

Rustle now runs as a macOS **menu-bar app** (a 🎙 icon, no Dock clutter) with a small settings panel. From there you can:

- **Pick the push-to-talk key** (default is the fn / Globe key).
- **Choose and download Whisper models** (base.en to large-v3) without touching the terminal.
- **Select the microphone** to record from.
- **Launch Rustle at login**, and watch a live status indicator (Listening / Transcribing / Typed).

The dictation itself is unchanged and works in every application, because it operates at the operating-system level, not per-app:

**hold the hotkey → talk → release → Whisper transcribes on-device → the text is typed into the focused app.**

## Prerequisites (macOS)

- **Rust** via [rustup](https://rustup.rs)
- **Xcode command line tools:** `xcode-select --install`
- **cmake:** `brew install cmake` (whisper-rs compiles whisper.cpp under the hood)
- **Node.js** and **[pnpm](https://pnpm.io)** for the Tauri CLI that runs and builds the app (`pnpm install` in the repo installs the CLI locally)

## Permissions (one time)

Because it listens for a global hotkey and types into other apps, macOS will ask you to grant, under **System Settings → Privacy & Security**:

- **Accessibility** - to type into the focused app
- **Input Monitoring** - to hear the global hotkey

Grant these to whatever launches Rustle: your terminal while developing, or the built `.app` once bundled. Relaunch that app after granting, because the grant only takes effect on a fresh start.

If you keep the default **fn (Globe)** hotkey, also set **System Settings → Keyboard → "Press 🌐 key to: Do Nothing"**, otherwise macOS grabs fn for its own dictation or emoji picker.

## Setup and run

### The menu-bar app

```bash
# 1. Install the Tauri CLI locally
npm install

# 2. Run the app (a 🎙 icon appears in the menu bar)
npm run tauri dev
```

Click the menu-bar icon (or its menu → **Open Rustle Settings**) to open the panel. Download a model from there if you have not already (base.en is the quick start), pick your hotkey and microphone, and hold the hotkey in any text field.

### The terminal build (no UI)

Handy for a quick test without the app wrapper:

```bash
cargo run -p rustle-core --bin rustle-cli
```

It reads the same saved settings and prints what it hears.

### Where things live

Config and downloaded models are stored under `~/Library/Application Support/rustle/`. The bundled `download-model.sh` still works too, and drops models into a local `models/` folder that the terminal build will also find.

## Project layout

- `core/` - the reusable dictation engine: config, audio capture, Whisper transcription, hotkey listening, keystroke injection, model downloads. Also builds the `rustle-cli` binary.
- `src-tauri/` - the Tauri menu-bar app: tray icon, settings window, and the commands the UI calls.
- `ui/` - the settings panel (plain HTML/CSS/JS, no bundler).

## Roadmap

1. **Spike** *(done)* - record → transcribe → print.
2. **Type anywhere** *(done)* - global hotkey plus keystroke injection into the focused app.
3. **Cleanup pass** *(planned)* - run the raw transcript through a local LLM (Ollama) to strip filler, fix punctuation, and apply a custom vocabulary.
4. **Menu-bar app** *(done - you are here)* - a Tauri tray app with settings, a model picker, and launch-at-login.
5. **Ship it** - MIT on GitHub, so no one can ever switch it off.

## Why this exists

Whisper (the speech model) is open and runs beautifully offline on Apple Silicon. What you pay a subscription for is really just a tidy hotkey, a cleanup pass, and a nice wrapper. That is a genuinely lovely bit of Rust, and it means nobody can ever cap your own dictation again.

Prior art worth reading: [Handy](https://github.com/cjpais/Handy) already does much of this in Rust + Tauri.

## Licence

MIT. See [LICENSE](LICENSE).
