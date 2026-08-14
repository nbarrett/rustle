# CLAUDE.md

Coding rules for Rustle. Non-negotiable. Applies to every human and every agent that touches this repo.

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

Signed, the management.
