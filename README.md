# Rustle

Open-source, local, **Rust** dictation. A Wispr Flow replacement you own.

**[Download for your computer](https://nbarrett.github.io/rustle/)** — macOS, Windows, or Linux. The page picks the file that matches the machine you are on. Phone builds (iOS and Android) are in the repo as a hold-to-talk app: same on-device Whisper. On iPhone, a Rustle keyboard stands in for the phone’s own dictation and types into WhatsApp, Mail or Notes.

Your voice is transcribed by Whisper **on your own machine** and never leaves it, so it is private, free, and impossible to rate-limit. The experience is the Wispr Flow one: hold a hotkey, talk, and the text appears in whatever app you are using.

> Working codename. Rename freely.

## Status: tray app with a settings panel

Rustle runs as a **tray / menu-bar app** on macOS, Windows, and Linux, with a small settings panel. From there you can:

- **Pick the push-to-talk key** (Globe on a Mac; Right Control, Right Alt, F8 or F9 elsewhere).
- **Choose and download Whisper models** (base.en to large-v3) without touching the terminal.
- **Export and import word corrections and dictation history** as a JSON file, so the same list can be copied to another computer.
- **Select the microphone** to record from.
- **Launch Rustle at login**, and watch a live status indicator (Listening / Transcribing / Typed).

**hold the hotkey → talk → release → Whisper transcribes on-device → the text is typed into the focused app.**

On a Mac, live words can appear as you speak. On Windows and Linux, the grey HUD still updates live, then the finished transcript is pasted on release (Ctrl+V). Linux paste needs `xdotool` (X11), `wtype`, or `ydotool`. Wayland global hotkeys are limited; X11 is the path that actually works today.

## Prerequisites

- **Rust** via [rustup](https://rustup.rs)
- **cmake** (whisper-rs compiles whisper.cpp)
- **Node.js** (npm / npx is enough; a global pnpm is not required)
- **macOS:** Xcode command line tools (`xcode-select --install`)
- **Windows:** Visual Studio C++ build tools
- **Linux:** a C compiler, ALSA/PipeWire headers, and WebKitGTK for the Tauri UI (`libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev`, `libasound2-dev`, `libx11-dev`, `libxtst-dev`)

## Permissions (one time)

**macOS** — first launch copies Rustle into Applications if you opened it from the DMG, then a setup panel asks for Microphone, Input Monitoring and Accessibility. Allow the microphone prompt; turn the other two switches on. Rustle restarts itself when Input Monitoring and Accessibility are granted.

The GitHub download is not Apple-notarised yet (that needs a Developer ID certificate). If macOS shows **Open Anyway**, use it once for that copy.

Then System Settings → Privacy & Security:

- **Microphone** - to record what you say
- **Input Monitoring** - to hear the global hotkey
- **Accessibility** - to type into the focused app

Grant these to whatever launches Rustle: your terminal while developing, or the built `.app` once bundled. Relaunch that app after granting, because the grant only takes effect on a fresh start.

If you keep the default **fn (Globe)** hotkey, also set **System Settings → Keyboard → "Press 🌐 key to: Do Nothing"**, otherwise macOS grabs fn for its own dictation or emoji picker.

**Windows** — allow the microphone when asked. A low-level keyboard hook does not need a special settings pane.

**Linux** — allow the microphone. Global keys and paste work most reliably on X11. On Wayland you may need to be in the `input` group, and you still need `xdotool`, `wtype`, or `ydotool` to paste.

## Setup and run

You do not need to copy anything into `/Applications`. On a locked-down machine, run it from the repo (or open the DMG and copy `Rustle.app` into your home folder).

### From source (npm / npx, no pnpm)

Needs Rust and cmake as well as Node. Packages stay inside this project; nothing is installed globally.

```bash
npm install
npx tauri dev
```

`npm start` does the same. A tray icon appears. Click it (or **Open Rustle Settings**) to open the panel. Download a model if you have not already (base.en is the quick start), pick your hotkey and microphone, and hold the hotkey in any text field.

On macOS, grant Microphone, Input Monitoring and Accessibility to the process that launched it (often Terminal, or the `rustle-app` binary under `target/`). Quit and reopen after granting Input Monitoring or Accessibility.

### Prebuilt apps (Mac, Windows, Linux)

Use the [download page](https://nbarrett.github.io/rustle/). It offers:

| Computer | File |
|---|---|
| macOS (Apple Silicon) | `Rustle-macos-aarch64.dmg` |
| Windows (x64) | `Rustle-windows-x64-setup.exe` |
| Linux (x64) | `Rustle-linux-x64.AppImage` or `.deb` |

Those files appear on a GitHub release (a version tag, or publish on the workflow). Until that release exists, run from source below.

Installed copies check GitHub for a newer version and offer **Install update** in settings. The first time you still install by hand; after that, the app can replace itself.

A Mac DMG does not have to go in `/Applications`. Open it, put `Rustle.app` in your home folder or Downloads, and open it from there. The lock-down that blocks `/Applications` does not usually block a user folder.

### The terminal build (no UI)

Handy for a quick test without the app wrapper:

```bash
cargo run -p rustle-core --bin rustle-cli
```

It reads the same saved settings and prints what it hears.

### Where things live

Config and downloaded models are stored under the OS app-data folder: `~/Library/Application Support/rustle/` on a Mac, `%APPDATA%\rustle` on Windows, `~/.config/rustle` on Linux. The bundled `download-model.sh` still works too, and drops models into a local `models/` folder that the terminal build will also find.

## Project layout

- `core/` - the reusable dictation engine: config, audio capture, Whisper transcription, hotkey listening, keystroke injection, model downloads. Also builds the `rustle-cli` binary.
- `src-tauri/` - the Tauri menu-bar app: tray icon, settings window, and the commands the UI calls.
- `ui/` - the settings panel (TypeScript, built with Vite).

## Roadmap

1. **Spike** *(done)* - record → transcribe → print.
2. **Type anywhere** *(done)* - global hotkey plus keystroke injection into the focused app.
3. **Cleanup pass** *(planned)* - run the raw transcript through a local LLM (Ollama) to strip filler, fix punctuation, and apply a custom vocabulary.
4. **Menu-bar app** *(done - you are here)* - a Tauri tray app with settings, a model picker, and launch-at-login.
5. **Phone** *(started)* - iOS hold-to-talk in the Rustle app, plus a Rustle keyboard so WhatsApp, Mail and Notes can take the words. Android is the same pad today; an Android keyboard is next.
6. **Ship it** - MIT on GitHub, so no one can ever switch it off.

### Phone (iOS and Android)

Apple will not let a third-party app replace the system dictation key on the stock keyboard. The way to plug Rustle in *in place of* that is the **Rustle keyboard**, with Whisper doing the English.

1. Turn off Apple Dictation: **Settings → General → Keyboard → Enable Dictation**.
2. Add **Settings → General → Keyboard → Keyboards → Add New Keyboard → Rustle**, then **Allow Full Access**.
3. In WhatsApp, Mail or Notes, switch to the Rustle keyboard and tap the mic. Talk in that app; tap again when you are done. The words go into the field.

iOS still has to wake Rustle for a moment to start the microphone. After that you stay in the app you were writing in. In the Rustle app itself the control is still hold-to-talk.

```bash
rustup target add aarch64-apple-ios aarch64-linux-android
npx tauri ios init
npx tauri android init
npx tauri ios dev
npx tauri android dev
```

iOS needs Xcode and your Apple Development team (`93S4343SAT` is already in `src-tauri/tauri.ios.conf.json`). Android needs Android Studio / NDK. First run downloads a Whisper model on the device; start with base.en.

## Why this exists

Whisper (the speech model) is open and runs beautifully offline on Apple Silicon. What you pay a subscription for is really just a tidy hotkey, a cleanup pass, and a nice wrapper. That is a genuinely lovely bit of Rust, and it means nobody can ever cap your own dictation again.

Prior art worth reading: [Handy](https://github.com/cjpais/Handy) already does much of this in Rust + Tauri.

## Licence

MIT. See [LICENSE](LICENSE).
