# AGENTS.md

Coding rules for Rustle. Non-negotiable. Applies to every human and every agent that touches this repo.

`CLAUDE.md` is a symlink to this file.

## Rule 1: No comments. Ever.

No `//`, no `///`, no `//!`, no `/* */`, no doc attributes, no `TODO`/`FIXME` comments. None. Not one. If a line seems to need a comment, the line is wrong. Rewrite it until it isn't.

## Rule 2: Functions describe themselves.

Every function name says exactly what it does, in full: `record_microphone_until_enter`, `downmix_to_mono`, `resample_linear`, `transcribe_with_whisper`. If you feel the urge to write a comment above a function, that urge is the bug. Rename it, split it, or tighten the types until the comment would only repeat the name.

Small, single-purpose functions with precise names beat one big function propped up by explanation.

## Rule 3: Names over notes.

Name variables, types and constants so the intent is obvious. `WHISPER_SAMPLE_RATE`, not `RATE` with a comment telling you what it is.

## Why (stated once, here, because this file is documentation, not code)

Comments rot. Names get refactored along with the code; comments get left behind, lying to the next reader. Self-describing code cannot lie, because it is the thing it describes.

## Where prose is allowed

In Markdown. The README and this file are where explanation belongs: the project, the roadmap, the setup. All fine, all here. Never inside a `.rs` file.

## Rule 4: Do not commit or push unless Nick says so.

Local edits are fine. `git add`, `git commit`, and `git push` are not. Wait for an explicit instruction: "commit", "push", "commit this", "get it to origin". "Fix it" is not permission to commit. "Make it work" is not permission to push.

This is trunk-based: commits go to `main`. That is why untested junk on `main` is worse here than in a branch-and-PR shop. Do not use `main` as a scratch pad.

## Rule 5: Do not put untested work on `main`.

A change is not done because it compiles, because `cargo test` passed, because `engine.log` said `iterm insert used`, or because a probe window showed a marker. Those are intermediate checks.

Dictation is done when Nick can hold the hotkey with the settings window closed and the words appear only in the app he was using. Until that is true, do not describe the work as fixed, and do not commit it.

`engine.log` is a diagnostic. It is not proof the product works.

## Rule 6: Settings is not part of dictation.

Rustle is a menu-bar app. The settings window is for the hotkey, mic, model, and corrections. It is not a destination for typed text. Dictation must work with that window closed. Do not type into settings. Do not require it to be open. Do not flip macOS activation policy when it opens or closes.

If hold-to-talk only works while settings is frontmost, Input Monitoring is off for this signed binary. macOS will still deliver keys to a frontmost app without that grant. Enable Rustle in Privacy & Security → Input Monitoring, then quit and reopen.

## Rule 7: Do not call it fixed.

If Nick is still seeing the old failure, it is not fixed. Say what you changed and what you have not verified. Do not announce another install as the solution.

## Rule 8: After a push, stay with it until GitHub Actions is green.

A push is not finished when `git push` returns. Watch the GitHub Actions run for that push (and for a release tag, the tag run that publishes the apps) until it succeeds. If it fails, fix it, push, and watch again. Do not wait for Nick to ask whether it passed.

A tag release is not done because the tag exists. It is done when the workflow has published the installers. A `main`-only push is done when the test job has passed.

When you mention a run, use the Actions URL with its `databaseId` (`/actions/runs/<databaseId>`). Never write `#123` for a run number.

Signed, the management.
