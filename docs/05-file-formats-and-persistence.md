# 05 — File formats and persistence

Everything the app reads or writes on disk, and the settings it keeps. The
Implementer chooses concrete storage technologies, but the observable
behavior must match.

## 1. Document files

- **Input**: plain text files. Open filters accept Markdown MIME types
  (`text/markdown`, `text/x-markdown`), the `.md` suffix, and plain text.
- **Output**: saved as plain text in the file's encoding (§1.1). Saving to a
  file whose name lacks `.md` does not force an extension (the save dialog
  pre-fills `.md` but the user may change it).
- **Encoding handling**:
  - Loading: try UTF-8 first; on failure detect the encoding automatically
    (charset detection) and retry; if detection fails or still errors, show
    an error dialog and reset to a new empty document.
  - Saving: encode using the encoding the file was loaded with; if some
    characters can't be represented, retry as UTF-8 and switch the document's
    encoding; if that also fails, show an error and abort the save (dirty
    state preserved).
- **Line endings**: content is written back exactly as edited (the editor
  preserves newlines as `\n`; no CRLF conversion is performed on load —
  a CRLF file keeps its `\r` characters as part of the text).

## 2. Settings

Persistent per-user settings (a freedesktop-style settings database; the
Implementer picks the mechanism, e.g. GSettings or equivalent). Semantics:

| Setting | Type | Default | Meaning |
|---|---|---|---|
| Window width / height | integer | 1000 / 600 | Restored as default size for new windows; updated when the user closes a non-maximized window |
| Window maximized | boolean | false | Restored on new windows |
| Window fullscreen | boolean | false | Restored on new windows |
| Color scheme | enum string | "system" | system / light / sepia / dark (`09-appearance.md`) |
| Spellcheck | boolean | true | Live spell checking on/off |
| Sync scroll | boolean | true | Editor↔preview scroll sync; toggling reloads previews |
| Input format | string | "markdown" | Converter flavor identifier: `markdown`, `commonmark`, `gfm`, `markdown_mmd`, `markdown_strict` |
| Autohide headerbar | boolean | true | Auto-hide bars while typing |
| Default statistic | enum | words | Which stat the status button shows: characters / words / sentences / paragraphs / read_time |
| Characters per line | integer | 66 | Editor column width in characters |
| Hemingway mode | boolean | false | Startup state of Hemingway Mode |
| Hemingway toast count | integer | 0 | How many times the Hemingway toast has been shown (max 3, resettable — `04-focus-and-hemingway.md`) |
| Preview mode | enum | full-width | Layout for the preview: full-width / half-width / half-height / windowed (half-width = side-by-side editor+preview) |
| Preview security | enum | ask | ask / always-restricted / always-unrestricted |
| Preview active | boolean | false | Whether new windows start with preview visible |
| Bigger text | boolean | false | Large text preference |
| Toolbar visible | boolean | false | Whether the formatting row is expanded (toolbar hidden by default) |
| Autosave period | integer (seconds) | 5 | Snapshot interval |

Settings changes take effect immediately in all open windows where relevant
(scheme, flavor, scroll-sync, default stat, preview toggles).

## 3. Recently-used files

Loaded/saved documents are registered in the freedesktop "recently used"
store, filtered to Markdown files in the recents popover; missing entries
are pruned on display.

## 4. Per-file extended metadata

- **Spellcheck language** is stored as extended attribute metadata on the
  document file and re-applied on load.
- Snapshot files carry the original document path as metadata (§5).

## 5. Recovery snapshots (on-disk effects)

- Directory: the user's **state** directory (XDG state dir), subfolder
  `snapshots/`, created with permissions 0750.
- Files: random unique names, content = raw document text (no wrapper).
- Metadata: the original file path, when the document has a backing file.
- Lifecycle: created/updated by the autosave timer while a document is dirty
  (`06-files.md` §5); deleted on successful save, on window close/destroy,
  and on normal app exit paths that close windows. Orphaned snapshots are
  restored into windows at next startup.

## 6. Bundled data the app ships

The app ships read-only resources it needs at runtime (the Implementer
packages equivalents as it sees fit):

- A Markdown tutorial document (opened from the menu).
- Stylesheets for the preview/export, one per color scheme, plus a base
  stylesheet.
- A Lua filter for the converter that rewrites relative paths.
- A Typst template (A4) and ODT/DOCX A4 reference documents for export.
- Editor syntax-highlight palettes for each color scheme.

## 7. Nothing else

The app writes no other state: no session file, no per-document caches, no
logging to disk by default (verbose logging to the console only via a flag
or a development profile).
