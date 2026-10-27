# 06 — Edge cases and error handling

Behavior when things go wrong or input is unusual. Each item is a
requirement.

## 1. Files

| Situation | Behavior |
|---|---|
| Opened file doesn't exist (CLI/association) | Skipped with a log warning; no dialog |
| Opened file can't be read (permissions/IO) | Error dialog with the reason; window resets to a new empty document |
| File is not valid text in any detected encoding | Error dialog naming the file and the error; window resets to a new empty document |
| File is empty (0 bytes) | Opens normally as an empty document |
| File contains a BOM | Treated as text; BOM handled per platform text conventions |
| File has CRLF line endings | Loaded as-is (CR characters remain part of the text); no conversion |
| Huge file (multi-megabyte) | Loads; typing/highlighting/statistics/preview remain responsive via background processing and debouncing; no artificial size limit |
| File is a directory or special file passed to open | Treated as read failure (error path) |
| Save target can't be created (permissions, full disk) | Error dialog with reason; dirty state kept |
| Save target deleted between Save As and write | Error dialog; dirty state kept |
| Save encoding can't represent content (e.g. saving non-Latin text back to a Latin-1 file) | Automatic retry as UTF-8; the document's encoding switches to UTF-8; only if UTF-8 also fails (impossible in practice) does an error appear |
| File changes on disk while open | Warning infobar with reload action (`06-files.md` §4); no data is overwritten silently |
| File changes on disk while a save is in flight | The monitor is re-armed after the save with a grace delay so the app's own write doesn't trigger the warning |

## 2. Editing

| Situation | Behavior |
|---|---|
| Typing an opening bracket before whitespace/end | Closing bracket auto-inserted (`01-editing.md` §2.1) |
| Typing a closing bracket matching the next char | Cursor skips over it; no duplicate |
| Paste containing bracket-like text | Inserted verbatim; no auto-pairing |
| Enter on empty list item | List exited (marker line removed) — works even in Hemingway Mode |
| Tab on empty bulleted list item | Indent level +1, bullet cycles - → * → + |
| Shift+Tab at column 0 of a non-list line | Nothing to remove; no-op (in Hemingway Mode: blocked-deletion feedback) |
| Undo after open/new | Nothing to undo (history cleared) |
| Dropping text onto a selection | The selection is replaced once by the dropped text (the drop's auto-insertion is removed first) |
| Dropping an image from outside the document's folder | Absolute percent-encoded path in the image syntax |
| Dragging an image whose name contains spaces/unicode | Name URL-unquoted for the label, path percent-encoded |

## 3. Preview and conversion

| Situation | Behavior |
|---|---|
| Converter missing from PATH | Preview/export shows an error explaining conversion failed; app does not crash |
| Converter fails on the document (bad input for flavor) | Preview shows an error state / export shows a detailed error dialog |
| Preview toggle spammed | Only latest conversion result is rendered; intermediate loads are coalesced |
| Document edited during a preview render | Re-render queued with newest text after the current one |
| File contains HTML and policy is "ask" | Security warning page shown before any render (`03-live-preview.md` §5) |
| Scroll sync with empty/short document (nothing to scroll) | Sync is inert (scale stays 0); no errors |

## 4. Export

| Situation | Behavior |
|---|---|
| Export target not writable | Detailed error dialog (description + expandable technical details) |
| Chosen PDF via LaTeX but TeX missing | Options pane replaced by "TeX not installed" guidance with copyable install command (`08-export.md` §2.2) |
| reveal.js export without bundled library | Export proceeds without the library copy; slideshow may be non-functional (open question `09-open-questions.md`) |
| Export while document dirty | Exports the current editor content (not the last-saved file content) |

## 5. States

| Situation | Behavior |
|---|---|
| New empty document (never typed) | Not dirty; close without dialog; nothing auto-saved |
| Text typed then all deleted (document empty again but dirty) | Counts as dirty; snapshot not updated with empty content (`06-files.md` §5); title keeps dirty marker until saved |
| All windows closed (one by one, via close buttons) | Each close follows the unsaved-changes guard; whether the process keeps running with zero windows or exits is unspecified — see `09-open-questions.md` |
| Snapshot exists at startup for a file already open | The snapshot loads into its own window (no dedup) |
| Restored snapshot's original file was deleted | Document opens unsaved with no backing file; saving prompts Save As |
| Spellcheck language metadata set to an unavailable language | Falls back to the default language |
| Focus Mode on a one-line document | Everything visible; centering has no visible effect |
| Hemingway Mode + Select All + type | Typing is rejected (selection can't be replaced); feedback per mode rules |
| Search regex that matches the empty string (e.g. `a*`) | Platform search semantics; replace-all must not loop forever — implementation must terminate |
| Search term with invalid regex | Entry shows error state; no matches replaced |
| Very long single line (no newlines, megabytes) | Horizontal scrolling; no freeze of the main thread during highlight scan (background) |
| Non-Markdown plain-text file | Opens and edits fine; preview/highlight simply reflect the flavor's rendering of the text |
