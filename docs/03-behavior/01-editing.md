# 03/01 — Editing behavior

Scope: the text area's editing behavior — input handling, list continuation,
indentation, auto-pairing, drag-and-drop, spell checking, undo, adaptive
layout, and in-place Markdown highlighting. Formatting commands (toolbar/
shortcuts) are in `02-formatting.md`; the plain-text file semantics are in
`05-file-formats-and-persistence.md`.

## 1. Document model

- The document is plain text (UTF-8 in memory). No hidden formatting, no
  separate AST: the file on disk is exactly the text shown, byte for byte
  (modulo encoding conversion, see `05-file-formats-and-persistence.md`).
- Opening a file or starting a new document clears the undo history (the
  previous document's edits cannot be undone into the new one).
- Undo/redo groups edits the way standard text widgets do; whole paste
  operations and whole command insertions (formatting commands, list
  continuation) undo as single steps.

## 2. Typing behavior

### 2.1 Auto-pairing

When the user types an opening delimiter and the character immediately after
the cursor is whitespace or end-of-document, the matching closing delimiter
is inserted after it and the cursor stays between the pair. Pairs:

| Typed | Inserted |
|---|---|
| `(` | `()` |
| `[` | `[]` |
| `{` | `{}` |
| `"` | `""` |
| `<` | `<>` |

Typing a closing delimiter (`)`, `]`, `}`, `"`, `>`) when the character at
the cursor is exactly that delimiter does not insert a duplicate; it just
moves the cursor over the existing one. Auto-pairing is skipped for ordinary
pastes (multi-character insertions are never rewritten).

### 2.2 List continuation (Enter)

When Enter is pressed at the end of a line that starts a list item, the new
line is prefixed automatically:

- **Bulleted list** (`-`, `*`, `+` with any indent): the same bullet and
  indent are inserted.
- **Numbered list** (`1.`, `2)`, any number, any indent): the next number
  with the same delimiter and indent is inserted (e.g. `3. ` after `2. `).
- **Checklist** (`- [ ] ` / `- [x] `): the same bullet and indent with an
  unchecked box `- [ ] ` is inserted.
- If the list line contains no content (only the marker), pressing Enter
  removes the empty marker line entirely instead of adding another marker —
  this is how the user exits a list.
- Enter anywhere else inserts a plain newline with no magic.

### 2.3 Indentation (Tab / Shift+Tab)

- **Tab** inserts a literal tab character. Additionally, when the current
  line is an *empty* list item (marker with no text):
  - bulleted: the item is re-written one indent level deeper and the bullet
    cycles to the next of `-` → `*` → `+` → `-` …;
  - numbered: the item is re-written one indent level deeper and the number
    resets to `1`.
- **Shift+Tab** removes one level of indentation:
  - in a bulleted list item: one indent level is removed and the bullet is
    set to match the symbol found on the nearest preceding list line with
    less indentation (looking back up to 50 lines), keeping the item text;
  - in a numbered list item: one indent level is removed, delimiter kept;
  - otherwise: one tab is removed from the current position.
  - In Hemingway Mode, Shift+Tab is treated as a deletion attempt (see
    `04-focus-and-hemingway.md`).

## 3. Drag and drop

Dropping content onto the editor inserts Markdown at the drop position, with
the inserted label pre-selected so it can be typed over:

| Dropped content | Inserted |
|---|---|
| An image file | `![file name](path)` — path is relative to the document's folder when the image is inside it, otherwise the percent-encoded absolute path; file name is URL-unquoted |
| Any other local file | `[file name](path)` with the same relative-path rule |
| A URL (`http(s)`/`ftp`) | `[web page](url)` |
| Plain text | the text itself (replacing whatever the drop auto-inserted, exactly once) |

The path portion is percent-encoded so the Markdown stays valid.

## 4. Spell checking

- Spell checking is on by default (preference, see `10-preferences.md`) and
  highlights misspelled words with the platform's standard underline.
- While Focus Mode is active, spell checking is temporarily disabled; it is
  restored when Focus Mode is switched off.
- The context menu offers: spelling suggestions for the word under the
  cursor, a submenu to switch the spell-check language, and a toggle for
  spell checking (platform spell-checker conventions).
- The chosen language is remembered **per file**: it is stored as extended
  metadata on the file and re-applied when that file is opened again. When
  the user changes the language while an unsaved/new document is open, the
  choice applies to the session but is not persisted for that document.

## 5. Undo/redo

Standard undo/redo (Ctrl+Z / Ctrl+Shift+Z) covers all user edits and command
insertions. Operations that replace the entire document (open, new, reload)
are performed as irreversible actions so they cannot be partially undone and
clear the redo stack.

## 6. Adaptive typography

- Font size steps through discrete sizes depending on the window width so
  that the text column plus padding always fits: from largest at wide
  windows to smallest at narrow ones. With the "larger text" preference on,
  two additional larger sizes become available at the top of the range and
  thresholds widen. Font size changes are applied immediately on window
  resize.
- The column width in characters is fixed by the "characters per line" setting
  preference (default 66); margins absorb the remaining width so the column
  is always centered.
- Tab width is 4 characters.

## 7. In-place Markdown highlighting ("live markup")

As the user types (or moves the cursor, or resizes), the app re-scans the
document and applies text styling *in the editor* — the source remains
Markdown. Highlighting is computed off the main thread so typing stays
responsive, and results are applied only if the text hasn't changed since the
scan started.

| Markdown construct | In-editor appearance |
|---|---|
| `*italic*` / `_italic_` | italic |
| `**bold**` / `__bold__` | bold |
| `***bold italic***` (mixed) | bold + italic |
| `~~strike~~` | strikethrough |
| `` `code` `` / ``$$math$$`` | code style: normal weight, background tint |
| Fenced code blocks (``` or ~~~) | code style over the whole block, plus a paragraph indent |
| `[label](url)` | label in accent color; the brackets and URL dimmed (gray, italic) |
| `![alt](url)` | the surrounding syntax dimmed (gray) |
| `<https://…>` autolinks | dimmed (gray) |
| `---`-style horizontal rules | the rule characters centered on their line |
| `> blockquote` | the line is outdented so the `>` hangs into the margin, with hanging indent for wrapped lines |
| `# Heading` | the heading line bolded and outdented by (level+1) characters so text aligns in the column |
| Setext headings (`===`/`---` underline) | the heading line bolded (skipped inside YAML front matter) |

Rules:

- No construct is highlighted *inside* fenced code blocks (code blocks are
  detected first).
- Highlighting colors derive from the current color scheme and update live
  when the scheme changes (`09-appearance.md`).
- YAML front matter (`---` … `---` or `...`) is not highlighted as a setext
  heading.
- Purely presentational: highlighting never modifies the document text.

## 8. Scrolling

- Lines are never soft-wrapped: a line longer than the visible width scrolls
  horizontally instead.
- When the cursor moves (keyboard navigation, typing, command insertion,
  paste), the view smooth-scrolls just enough to keep the cursor visible with
  a ~32 px safety margin; dragging with the mouse does not fight the user.
- In Focus Mode the cursor line is instead smooth-scrolled to the vertical
  center of the view ("typewriter scrolling").
- The editor's scroll position as a 0–1 fraction ("scroll scale") is
  exposed for preview synchronization (`03-live-preview.md`).

## 9. Responsiveness requirements

- Typing latency must not visibly degrade on documents of at least several
  thousand lines. Highlighting, statistics, and preview conversion must run
  off the main thread, coalescing rapid input (a burst of keystrokes triggers
  one recomputation after it settles, not one per keystroke).
- Statistics debounce ~10 ms; preview conversion debounces similarly (see
  the respective docs).
