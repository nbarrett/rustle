# iPad and iPhone beta preparation and acceptance

Updated 12 October 2026.

Mac, iPhone, iPad and Android all use bundle id `io.github.nbarrett.rustle` on team `93S4343SAT`. The first iOS beta is a standalone dictation app: hold the microphone, speak, release, then copy the transcript. There is no keyboard extension in this beta.

## Apple identifiers

On [Identifiers](https://developer.apple.com/account/resources/identifiers/list) there should be one App ID: `io.github.nbarrett.rustle` (currently listed as Rustle Personal). That is Mac, iOS and Android.

1. Keep `io.github.nbarrett.rustle`. Automatic signing reuses it.
2. After a Mac build of 0.1.15 or later is installed and dictation works, delete the leftover `com.annix.rustle` App ID. Do not delete it before that install.
3. Do not add a keyboard extension App ID for this beta. The keyboard sources stay in the repo and are excluded from the archive.
4. No App Group, Push Notifications, or Associated Domains are required for the standalone app.

## Privacy policy URL

The policy in this repository is [ios-privacy.md](ios-privacy.md). After it is on `main`, the public URL is:

https://nbarrett.github.io/rustle/ios-privacy.html

Paste that URL into App Store Connect → the Rustle app → TestFlight → Test Information → Privacy Policy URL. Review email is `nick.barrett36@me.com`. Apple also asks for a contact telephone number before external testers can be approved; that field stays with Nick.

## Build and export

Need Xcode, XcodeGen (`brew install xcodegen`), an Apple developer login in Xcode, and the iOS Rust target:

```bash
rustup target add aarch64-apple-ios
pnpm build:ios-testflight 2
```

Use a new positive integer for every upload. The script runs desktop tests, regenerates the Xcode project from `src-tauri/gen/apple/project.yml`, archives for `aarch64`, and exports an App Store Connect package under `target/ios-testflight-<build>`. It does not upload.

Upload with Transporter or:

```bash
xcrun altool --upload-app --type ios --file target/ios-testflight-2/*.ipa --apiKey <key> --apiIssuer <issuer>
```

## What to accept on a device

Install from TestFlight on an iPhone or iPad running iOS 16 or later.

1. Open Rustle. Grant microphone access when asked.
2. In General, download a speech model while on a network.
3. Hold the microphone, speak a short sentence, release.
4. The transcript appears in the app. Copy it and paste it into Notes or Messages.
5. Airplane mode after the model is present: a second take still transcribes.
6. Deny microphone once on a fresh install: the app asks you to allow it in Settings instead of hanging.

Do not expect words to appear in other apps without paste. Do not expect a custom keyboard. Do not expect the macOS HUD or global hotkey.

## Out of this beta

- Custom keyboard / WhatsApp field insertion
- Android store package
- Automatic TestFlight upload from GitHub Actions
