# Rustle — Handover

_For the next person or agent picking this up. Written after a long, messy session. It is deliberately honest about what works, what does not, and what is unverified._

## How to use this document

If you are a fresh Claude Code session: read this whole file first. The single most important open problem is **typing into Claude Code (and possibly other Chromium/Electron apps)**, and the biggest process pain is **macOS permissions resetting on every rebuild**. Do not "improve" the UI or the engine before you have read the "macOS permissions" section, or you will re-enter the loop that ate the last session.

## What Rustle is

An open-source, local dictation app for macOS. A Wispr Flow replacement. Whisper runs on-device (via `whisper-rs`), so nothing leaves the machine. Hold a push-to-talk key, speak, release, and the transcript is injected into whatever app is focused. It runs as a menu-bar app (a waveform tray icon, no Dock icon).

## Current state (honest, one paragraph)

The core pipeline works: global hotkey capture, microphone recording, on-device Whisper transcription, a live transcript preview, model download in-app, and a word-corrections dictionary. Transcription quality is good on `small.en`. **What is NOT confirmed working is the final step: injecting the text into Claude Code.** Every attempt reaches the "Typed" state with no error, but the text does not appear in Claude Code's input. As of this handover, injection was just switched from synthetic keystrokes to a clipboard-paste approach, and that change is UNTESTED. The user is (justifiably) frustrated; treat "does it type into Claude Code?" as the acceptance test for everything.

## Repo layout

```
rustle/
  core/            rustle-core library + rustle-cli binary
    src/
      config.rs      Config (hotkey, model, mic, corrections), load/save, corrections engine, model catalog
      audio.rs       cpal recording, downmix, resample
      transcribe.rs  whisper-rs wrapper; strips [BLANK_AUDIO]-style non-speech tags
      mac_hotkey.rs  macOS CGEventTap (HID level) global hotkey listener (replaces rdev)
      engine.rs      orchestration: hotkey -> record -> partial previews -> transcribe -> inject
      download.rs    ureq model downloader
      hotkey.rs      HotkeyChoice enum + macOS keycodes
      bin/rustle_cli.rs  terminal build
  src-tauri/       Tauri v2 menu-bar app (crate: rustle-app)
    src/main.rs      tray icon, settings window, IPC commands, window auto-resize, accessibility request
    tauri.conf.json  window config, identifier com.annix.rustle
    Info.plist       NSMicrophoneUsageDescription, LSUIElement
    capabilities/default.json
    icons/           waveform icons (tray.png is the monochrome template)
  ui/              frontend (plain HTML/CSS/JS, no bundler, uses withGlobalTauri)
    index.html       Dictation + History tabs
    styles.css
    main.js
  package.json     pnpm; @tauri-apps/cli
  download-model.sh
  README.md
```

Toolchain: Rust 1.97.1 (rustup), pnpm, cmake (for whisper.cpp), Xcode CLT. Node present.

## Build, install, run

The app must run as an installed `.app`, NOT via `pnpm tauri dev` (the dev binary does not get macOS permissions properly).

```bash
pnpm install                       # once
pnpm tauri build --debug           # builds target/debug/bundle/macos/Rustle.app
rm -rf /Applications/Rustle.app
cp -R target/debug/bundle/macos/Rustle.app /Applications/
```

Then the user launches it from Spotlight (Cmd-Space, "Rustle"). Quit via the menu-bar icon.

Config and models live in `~/Library/Application Support/rustle/` (`config.json`, `models/`).

Terminal build for quick pipeline checks: `cargo run -p rustle-core --bin rustle-cli`.

## macOS permissions — the hard part (read this)

Three separate TCC permissions, all granted to bundle id **com.annix.rustle**:
- **Input Monitoring** — to hear the global hotkey (CGEventTap).
- **Microphone** — to record.
- **Accessibility** — to inject text into other apps. This is the one that matters and the one that keeps failing.

Hard-won facts, all confirmed by reading the app's own status log during the session:

1. **Accessibility only takes effect on a fresh launch.** Granting it to a running instance does nothing until you quit and relaunch. The app logs `Typed(...)` and enigo returns Ok, but macOS silently drops the injection until relaunch.
2. **Every rebuild resets all grants.** The app is ad-hoc signed, so each build has a new code hash, and macOS treats it as a different app. This is the churn that dominated the session. THE fix is a stable code signature (see Open items).
3. **The grant is a toggle in the Accessibility LIST, not the pop-up dialog.** The "Rustle would like to control..." dialog just points at System Settings; clicking it grants nothing. The user must flip the switch in the list.
4. **Launching the app from a shell may break TCC "responsibility."** During the session the app was launched with `open --stderr /tmp/log ...` to capture output; there is an untested theory that this makes the shell (not Rustle) the responsible process, so grants to Rustle do not apply. The current guidance is: the USER launches it from Spotlight, and you do NOT launch it with `open --stderr`. This theory is unconfirmed.
5. Reset grants cleanly with: `tccutil reset Accessibility com.annix.rustle` (and `ListenEvent` for Input Monitoring, `Microphone`). Then relaunch and re-grant.
6. The **fn / Globe key** is unreliable as a hotkey unless the user frees it: System Settings -> Keyboard -> "Press Globe key to: Do Nothing", and Dictation Off. Right Option / Left Option have no such conflict. Current config uses `RightOption`.

## Full problem log (every issue raised this session)

Status key: FIXED (verified), CLAIMED (changed in code, not confirmed by user), OPEN (not done), INFO (not a bug).

| # | Problem raised | Status | Notes |
|---|---|---|---|
| 1 | Get it installed and running from scratch | FIXED | Rust, cmake, model download, first build all done |
| 2 | Hotkey should be the fn/Globe key | FIXED | Works after the user frees fn in System Settings |
| 3 | App crashed when clicking Save | FIXED | autostart plugin panic; hardened with try_state + catch_unwind |
| 4 | App terminated the moment fn was held | FIXED | Root cause: rdev called macOS TSM off the main thread (SIGTRAP). Replaced rdev with a custom CGEventTap in mac_hotkey.rs |
| 5 | Microphone kill on record | FIXED | Added NSMicrophoneUsageDescription (this was partly a red herring; #4 was the real crash) |
| 6 | **Does not type into Claude Code** | **OPEN** | The central unresolved issue. Pipeline reaches Typed with no error; text never appears. Tried: Accessibility grant, relaunch-after-grant, HID-level tap, user-launched app, clipboard-paste (latest, UNTESTED). Discriminator not yet run: does it type into a plain field (Spotlight/TextEdit) but not Claude Code? |
| 7 | Tray icon looked rubbish (teal square) | FIXED | Replaced with a monochrome waveform template tray icon + rounded app icon |
| 8 | Globe option should show a globe icon | FIXED | Picker label shows a globe emoji |
| 9 | Two Rustle icons in the menu bar | FIXED | Killed duplicate dev + bundle instances; stopped the tauri dev watcher |
| 10 | App not shown in Accessibility list | FIXED | Switched to installed /Applications app; added explicit AXIsProcessTrustedWithOptions request on launch |
| 11 | Want to see text appear in real time (Wispr-Flow floating waveform overlay) | OPEN | Not built. Deferred because the overlay window must be non-activating (must not steal focus) which is fiddly. The in-app live transcript exists but only in the settings window |
| 12 | Window should auto-size to content, not be resizable | CLAIMED | Now measures the content block (`.app` offsetHeight) and locks the window via min/max size. Earlier bugs fixed along the way: set_resizable toggle blanked the WKWebview; wrong scrollHeight measurement made it too tall; Save button was clipped (fixed with a title-bar allowance) |
| 13 | Annoying scrollbar on settings | CLAIMED | Addressed via auto-size + tabs; last user screenshots still showed problems, latest build should resolve. UNVERIFIED |
| 14 | Too tall / too much in one column | CLAIMED | Split into Dictation + History tabs |
| 15 | Tabs hid the live transcript while testing | FIXED | Live transcript kept on the Dictation tab alongside the settings |
| 16 | Selecting a not-downloaded model broke dictation | FIXED | Save now keeps an installed model if the chosen one is not downloaded |
| 17 | "gets" became "Gits" | FIXED | Corrections now match whole words, not substrings |
| 18 | [BLANK_AUDIO] leaking into output | FIXED | Non-speech bracketed tags filtered in transcribe.rs |
| 19 | Laggy on medium.en model | MITIGATED | Live-preview interval raised 600ms -> 900ms; recommend base.en/small.en. The preview re-transcribes the whole clip each tick, which is O(n^2); a real fix is a bounded window |
| 20 | Save should close the window | CLAIMED | main.rs hides the settings window after save. UNTESTED |
| 21 | Dictation history too hidden / at the bottom | CLAIMED | Now a first-class History tab. UNTESTED |
| 22 | Do I have to say "question mark"? | INFO | No. Whisper auto-punctuates. Saying it types the literal words |
| 23 | Should it be TypeScript not JavaScript? | OPEN | Frontend is vanilla JS with no build step. TS would need a Vite/tsc pipeline. Not done |
| 24 | Left Option as a hotkey choice | OPEN | Was added then reverted; currently NOT in the picker. Trivial to re-add (keycode 58) |
| 25 | Re-granting permissions on every rebuild is maddening | OPEN | Root cause: ad-hoc signing. Fix: a stable self-signed code-signing cert (see below). A cert was partially generated in /tmp during the session but NOT installed |
| 26 | "Rust is the wrong environment for this" | INFO | The pain is macOS permissions + packaging + my mistakes, not Rust. Any native stack pays the same TCC tax; a web app physically cannot do global hotkeys or cross-app typing |

## Open items / recommended next steps (in priority order)

1. **Make it type into Claude Code.** First run the discriminator: does dictation land in Spotlight/TextEdit but not Claude Code? If yes, it is Chromium-specific input rejection; the clipboard-paste change (just added, `type_via_paste` in engine.rs) is the intended fix, so verify it. If it lands nowhere, Accessibility still is not effective; investigate TCC responsibility (stop using `open --stderr`, have the user launch from Spotlight) and check for duplicate/stale "Rustle" entries in the Accessibility list.
2. **Set up a stable code signature to end the re-grant churn.** Create a self-signed code-signing certificate once (Keychain Access -> Certificate Assistant -> Create a Certificate -> self-signed root, type Code Signing), then `codesign --force --deep --sign "<cert name>" Rustle.app` on every build. TCC then keys on the stable identity and grants persist across rebuilds. This single change makes iteration bearable. (An automated attempt via openssl + `security import` was started but failed because macOS has no `timeout` command and the import step did not run; the identity is not installed.)
3. Verify the UNTESTED changes: window auto-size/no-scrollbar/Save-visible, Save-closes-window, History tab.
4. Build the floating live-transcript overlay (item 11) if still wanted. Must be a non-activating, always-on-top, borderless window so it never steals focus from the target app.
5. Optional: reduce dictation lag with a bounded live-preview window; add Left Option to the picker; consider TypeScript for the frontend.

## Git / cleanup state

- Branch `main`, trunk-based (no PRs). Last commit: `3687dfa` "feat: menu-bar app with settings, live transcript and word corrections".
- **Uncommitted** (all the session's later work): `Cargo.lock`, `core/Cargo.toml`, `core/src/config.rs`, `core/src/engine.rs`, `src-tauri/src/main.rs`, `src-tauri/tauri.conf.json`, `ui/index.html`, `ui/main.js`, `ui/styles.css`. Commit these once the Claude Code typing is confirmed.
- **Diagnostic logging is still in**: `eprintln!("[rustle] {status:?}")` in src-tauri/src/main.rs. Remove or gate it before a real release.
- Leftover incomplete download `~/Library/Application Support/rustle/models/ggml-large-v3.partial` can be deleted.
- Leftover cert files in `/tmp` (rustle-cert.pem, rustle-key.pem, rustle.p12) from the abandoned signing attempt.
- Coding rule for this repo (see CLAUDE.md): **no comments in any `.rs` file, ever.** Self-describing names only. Prose belongs in Markdown.

## Current config (for reference)

```json
{
  "hotkey": "RightOption",
  "model_file_name": "ggml-small.en.bin",
  "input_device_name": null,
  "launch_at_login": true,
  "corrections": [
    { "spoken": "whisper flow", "written": "Wispr Flow" },
    { "spoken": "russell", "written": "Rustle" },
    { "spoken": "Get", "written": "Git" }
  ]
}
```
