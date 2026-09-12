# Rustle in the browser

A build of Rustle that runs entirely inside a browser tab, for machines where you
cannot install software and cannot grant permissions.

The desktop app needs macOS Accessibility and Input Monitoring. Both of those live
in a system-wide database protected by System Integrity Protection, only an
administrator can unlock them, and there is no per-user equivalent to fall back on.
On a managed machine you will never be given them. This target exists so that you
still get Rustle without asking anyone for anything.

## What the locked-down machine needs

A browser, and a URL. That is the whole list, and the URL already exists:
**https://nbarrett.github.io/rustle/**.

This is one of several ways to run Rustle, not a replacement for the others. The
standalone Mac and Windows apps give you a global hotkey and type straight into
whatever application you are already in. Use those wherever you are allowed to install
software; this target is for the machines where you are not.

Nothing is installed. No Homebrew, no Node, no Rust, no command line tools, no
administrator password, no Jamf package, no entry in Privacy and Security. The only
permission involved is the microphone prompt the browser shows you the first time,
which is stored in your own browser profile and needs no admin rights.

## How it is used

1. Open the URL.
2. Choose a model and press **Load model**. This downloads once and is then cached
   by the browser, so later visits start immediately.
3. Hold the **Hold to talk** button, or hold the space bar while the tab is focused.
4. Speak, then release.
5. The text appears in the box and is copied to your clipboard. Press Cmd+V where
   you want it.

## What it cannot do

Be honest with yourself about these before relying on it.

- **No global hotkey.** A web page cannot see keystrokes while another application
  has focus. You have to switch to the tab to dictate. This is a browser security
  boundary, not something we can code around.
- **No typing straight into the app you were using.** You get one manual Cmd+V per
  dictation. Inserting text into another application is exactly what Accessibility
  permission exists to control.
- **No live words as you speak.** That feature is direct text insertion through the
  macOS Accessibility API, which has no unprivileged equivalent.
- **Slower than the desktop app**, though WebGPU on Apple Silicon is respectable.

Everything else is the same. Corrections, spoken punctuation and British spellings
are applied by the same Rust code the desktop app uses, compiled to WebAssembly, so
the two cannot behave differently.

## What it stores and sends

Worth knowing, and worth quoting if anyone in security asks.

- Audio is never written to disk and never uploaded. It is captured, converted to
  samples in memory, transcribed in the tab, and discarded.
- Transcript history is stored only in that browser's local storage and can be
  cleared from the History tab. It is never uploaded.
- The only outbound request the page ever makes is a `GET` for the model files on
  first load. Nothing is ever uploaded. You can confirm both claims in the browser
  network panel in about a minute.
- Your corrections and your British-spellings preference are kept in that browser's
  local storage. Nothing you dictate is.

If the network blocks the model host, see [Serving the models yourself](#serving-the-models-yourself)
to make the page make no external requests at all.

## Building it

Build on a machine you control. The locked-down machine only ever opens the result.

### If that machine also has no admin rights

Every tool below installs into your home directory. None of them need an
administrator, and none of them need Homebrew.

Rust and the WebAssembly toolchain:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
```

```bash
source "$HOME/.cargo/env" && rustup target add wasm32-unknown-unknown && cargo install wasm-pack --locked
```

Node, from the official tarball rather than a package manager:

```bash
curl -fsSL https://nodejs.org/dist/v22.20.0/node-v22.20.0-darwin-arm64.tar.xz | tar -xJ -C "$HOME/.local"
```

```bash
export PATH="$HOME/.local/node-v22.20.0-darwin-arm64/bin:$PATH" && corepack enable --install-directory "$HOME/.local/bin"
```

Use `darwin-x64` instead of `darwin-arm64` on an Intel Mac, and add that `PATH` line
to your shell profile so it survives a new terminal.

### The build

```bash
pnpm install
```

```bash
pnpm run web:build
```

That compiles the Rust correction engine to WebAssembly and writes a static site to
`dist-web/`. It is plain files. There is no server component and nothing to run.

To work on it locally, with rebuilds as you edit:

```bash
pnpm run web:dev
```

## Where it is already published

Every push to `main` builds this app and publishes it with the landing page, so the
live copy is at **https://nbarrett.github.io/rustle/**. Open that page and press
**Start the browser version**, and it runs there with nothing to install.

The build is not committed to the repository. The `web` job in
`.github/workflows/test-and-release.yml` builds it, hands the result to the `pages`
job as an artifact, and that job publishes `docs/` with the app underneath at `/app/`.
Rebuilding therefore costs nothing in repository size.

If that hosted copy is reachable from your locked-down machine, you need nothing else.
Host it yourself only when your network blocks github.io.

## Publishing it yourself

`dist-web/` is static, so anything that serves files over HTTPS will do. GitHub
Pages, an internal web server, an S3 bucket, a shared drive served by a colleague.

One constraint decides this for you: **browsers only allow microphone access on a
secure origin.** In practice that means `https://` or `http://localhost`. Opening
`dist-web/index.html` straight from the filesystem is unreliable and Safari in
particular will refuse, so do not plan around it.

If your organisation blocks GitHub, host the folder somewhere internal that the
locked-down machine can reach. The page does not care where it is served from.

## Serving the models yourself

By default the model files come from the Hugging Face CDN on first load. If that is
blocked, or if you want a deployment that touches nothing external, mirror the model
repository next to the site and point the build at it:

```bash
VITE_RUSTLE_MODEL_HOST=https://your-internal-host/models pnpm run web:build
```

The page then fetches models from that host instead, and once the browser has cached
them it makes no network requests at all.

## Where the shared code lives

- `core/src/transcript.rs` holds the corrections, spoken punctuation, British
  spellings, hallucination stripping and sentence-capital rules. The desktop app and
  this target both call it, so behaviour cannot diverge between them.
- `web/wasm/` is a thin binding that exposes that module to JavaScript.
- `web/app/` is the page itself. The worker does speech to text, the main thread
  applies the shared Rust rules to the result.
