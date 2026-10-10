# Dictation behaviour and acceptance checks

Rustle keeps transcription cleanup separate from insertion. Corrections, regional spelling and Whisper cleanup produce the recognised words. The insertion policy uses the field context captured when the hotkey is pressed to prepare those words for the destination. Live and final insertion use the same policy.

## Insertion rules

| Context | Behaviour |
| --- | --- |
| Empty multiline field | Keep sentence capitalisation and punctuation. |
| Empty single-line title or search field | Keep initial capitalisation; omit the automatic final full stop. |
| Unfinished sentence before the caret | Lowercase the initial word, preserving I and acronyms; omit the automatic final full stop. |
| Words or closing punctuation after the selection | Omit the automatic final full stop. |
| Inserted words touch an existing word | Add one separating space at the affected boundary. |
| Existing whitespace | Preserve it without adding another space. |
| Attached punctuation | Keep it attached. |
| Selected text | Derive context from the text outside the selection. |
| Unavailable or invalid caret context | Keep recognised capitalisation, omit the automatic final full stop, and do not guess surrounding spaces. |

Caret offsets use UTF-16, matching macOS accessibility. Negative ranges, ranges beyond the field, and ranges splitting a Unicode character are rejected rather than clamped into a plausible position.

Sentence capitalisation is heuristic. Rustle preserves I and acronyms, but cannot reliably distinguish every proper name from a word capitalised by Whisper. British spelling follows the system locale and includes capitalise, capitalised and capitalisation. User corrections run after regional spelling.

Browsers receive the completed transcript through clipboard paste when the hotkey is released. Live words remain in the Rustle HUD while recording. Browser accessibility writes can report success without updating the web editor, so they are not used for browser insertion.

## Clipboard ownership on macOS

Clipboard insertion preserves every item and readable data format before writing the transcript. If preservation fails or the clipboard changes during capture, insertion stops before replacing it. After the paste delay, restoration runs only if the clipboard still belongs to Rustle's transcript. A new copy is preserved. An originally empty clipboard is restored to empty. The delay is not an acknowledgement from the destination app; real image and paste behaviour must still be checked.

## HUD placement on macOS

The HUD prefers a position just above the text caret, separated by 16 points. It moves below if there is insufficient room above and stays within the screen edges. Rustle requests the fuller accessibility interface in Chromium apps and checks the system-wide focused element only when it belongs to the intended destination. macOS caret bounds are read through AXSelectedTextMarkerRange and AXBoundsForTextMarkerRange for rich editors, or AXBoundsForRange for ordinary text fields. If the app rejects zero-length caret ranges, Rustle tries the adjacent character bounds. If caret geometry remains unavailable, it anchors the HUD above or below the mouse pointer rather than the top of the screen. Clear screen-edge positions are used only if neither nearby position fits. If no clear position exists, only the menu-bar indicator remains visible. The HUD remains nonactivating and ignores mouse events.

## Automated verification

The insertion matrix checks the complete resulting sentence, existing whitespace, punctuation, repeat dictation and stable formatting on a second pass. Accessibility tests check selections, Unicode and invalid ranges. Clipboard tests use isolated pasteboards and check image formats, empty contents and newer copies. These checks do not establish that real dictation works in another app.

## Real-app acceptance

Run with Rustle settings closed, using the normal hold-to-talk hotkey. Test in Codex and another app you regularly use. These checks remain pending for the consolidated build until observed by Nick.

| Check | Procedure | Expected result |
| --- | --- | --- |
| New sentence | Dictate into an empty message field. | Words appear only in the destination. |
| Continuation | Type `Please`, place the caret at its end, dictate `send the invoice`. | `Please send the invoice`, without an inserted full stop. |
| Leading boundary | Type `embellish`, caret immediately at its end, dictate `now`. | `embellish now`. |
| Trailing boundary | Type `Please the invoice`, caret before `the`, dictate `send`. | One space at each word boundary. |
| Both boundaries | Put the caret between two adjacent words and dictate a phrase. | Neither boundary joins words. |
| Selection | Select a word in a sentence, then dictate its replacement. | Only the selected word is replaced; surrounding text is intact. |
| Punctuation | Insert immediately before an existing full stop, comma or closing bracket. | No duplicate full stop or added space before punctuation. |
| Repeat dictation | Dictate twice into an unfinished sentence. | Each insertion respects the updated context. |
| Image clipboard | Copy an image, dictate, wait for insertion to finish, then paste the image. | The original image pastes. |
| New copy | Copy something else while dictation finishes. | Rustle does not restore an older clipboard over it. |
| HUD | Dictate into a field near the bottom, then near the top of the screen. | The HUD stays outside the field; typing focus remains in the destination. |
| British spelling | Dictate capitalised, capitalisation and behaviour. | British spellings under a British system locale. |
| History and corrections | Resize the window and change tabs. | Lists use the available space and history shows dates for new entries. |

Do not label dictation fixed, commit it or push it solely because automated verification passes.

## Local build and restart

Run `pnpm build:restart` on macOS. It tests the workspace, checks both frontends, builds the signed release app, installs it, verifies the executable and restarts Rustle. It does not stage, commit or push source changes. If installation or restart fails, it attempts to restore and reopen the previous app.
