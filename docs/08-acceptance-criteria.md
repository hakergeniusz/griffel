# 08 — Acceptance criteria

Testable, behavior-level criteria. "GIVEN/WHEN/THEN" phrasing; each is
verifiable by driving the UI or observing files. Grouped by area. Cross
references go to the behavior docs.

## A. Editing

- **A1** GIVEN a fresh window, WHEN the app starts, THEN an empty document
  has keyboard focus, the title shows the new-document placeholder without a
  dirty marker, and no dialogs appear.
- **A2** GIVEN the window at width W ≥ 360 px, WHEN text is present, THEN
  the text column is horizontally centered, spans the "characters per line" setting
  (default 66) character widths, and resizing the window keeps it centered
  with the font stepping through the discrete size scale (`01-editing.md` §6).
- **A3** WHEN the user types `(` before whitespace, THEN `)` is inserted and
  the cursor sits between; typing `)` then skips over it. Same for `[]`,
  `{}`, `""`, `<>` (`01-editing.md` §2.1).
- **A4** GIVEN a line `- item`, WHEN Enter is pressed, THEN the next line
  starts with `- `; GIVEN `2. item`, THEN `3. ` is inserted; GIVEN
  `- [x] done`, THEN `- [ ] ` is inserted (`01-editing.md` §2.2).
- **A5** GIVEN a line containing only `- `, WHEN Enter is pressed, THEN the
  marker line is removed (list exited) (`01-editing.md` §2.2).
- **A6** GIVEN an empty `- ` item, WHEN Tab is pressed, THEN the line
  becomes an indented `* ` (next bullet in - → * → +) (`01-editing.md` §2.3).
- **A7** GIVEN an image file dragged into the editor from the document's
  folder, WHEN dropped, THEN `![name](relative-path)` is inserted with the
  label selected (`01-editing.md` §3).
- **A8** GIVEN misspelled text, WHEN spellcheck is on, THEN words are
  underlined and the context menu offers replacements; WHEN the language is
  changed, THEN reopening the same file later restores that language
  (`01-editing.md` §4).
- **A9** WHEN Ctrl+Z is pressed after a formatting command, THEN the command
  reverts in one step; undo history does not span an opened file boundary.

## B. In-place highlighting

- **B1** GIVEN text `**bold** _i_ ~~s~~`, THEN the rendered editor shows the
  respective styles while the source text remains unchanged.
- **B2** GIVEN a heading `## Title` at the column start, THEN it renders
  bold and outdented; GIVEN a line starting `> `, THEN it outdents; GIVEN
  `---` as a horizontal rule, THEN it centers — and none of these applies
  inside fenced code blocks (`01-editing.md` §7).
- **B3** GIVEN YAML front matter at the top, THEN the `---` lines are not
  styled as headings.

## C. Formatting commands

- **C1** WHEN bold is invoked with "word" selected, THEN the text becomes
  `**word**` with "word" selected; invoking it again unwraps to `word`
  (`02-formatting.md` §1).
- **C2** WHEN the heading command is applied 4 times to a plain line, THEN
  the line successively becomes H1, H2, H3, then plain again.
- **C3** WHEN the table command picks 3×2 in the size picker, THEN a
  fixed-width table with header row, dash separator, and 2 data rows is
  inserted (`02-formatting.md` §7).
- **C4** WHEN link is invoked on selected URL text, THEN
  `[label](that-url)` is inserted with the label selected.

## D. Preview

- **D1** WHEN Ctrl+P is pressed, THEN the preview pane appears in the
  remembered layout and renders the document; pressing again hides it.
- **D2** WHEN the text is edited, THEN the preview updates without user
  action, and rapid typing produces one eventual re-render, not one per
  keystroke (`03-live-preview.md` §3).
- **D3** WHEN the editor is scrolled to fraction 0.5, THEN (sync on) the
  preview scrolls to ≈0.5, and vice versa.
- **D4** GIVEN a file containing `<script>` and policy "ask", WHEN preview
  opens, THEN the security warning page shows; choosing restricted renders
  the HTML as plain text; choosing unrestricted executes it. GIVEN policy
  "always unrestricted", no warning appears.
- **D5** WHEN a link is clicked in the preview, THEN the system browser
  opens and the preview does not navigate.
- **D6** WHEN the layout switcher picks "windowed", THEN a separate window
  shows the preview, and closing it turns preview off in the main window.

## E. Modes

- **E1** WHEN Focus Mode is toggled on, THEN all text outside the current
  sentence dims, the cursor line scrolls to vertical center, and spell
  underlines disappear; toggling off restores them (`04-focus-and-hemingway.md`).
- **E2** GIVEN Hemingway Mode on, WHEN Backspace is pressed, THEN the text
  is unchanged, the editor shakes, and a toast appears (≤3 times per
  install, then only again after 4 blocked attempts within ~1 minute).
- **E3** GIVEN Hemingway Mode on, WHEN Enter ends an empty list line, THEN
  the marker is still removed (internal ops exempt).

## F. Statistics

- **F1** GIVEN a document with known counts (markup stripped per
  `05-statistics.md`), THEN each statistic matches a hand-computed value;
  WHEN text is selected, THEN the button shows "selected of total".
- **F2** WHEN a different statistic is picked in the popover, THEN the
  button shows it immediately and after an app restart.

## G. Files

- **G1** WHEN Ctrl+S is pressed on an unmodified opened file, THEN content
  is written byte-identically (same encoding) and the dirty marker clears.
- **G2** GIVEN a dirty document, WHEN the window is closed, THEN the
  unsaved-changes dialog appears with Save/Discard/Cancel; Save closes after
  writing; Cancel keeps the window open.
- **G3** GIVEN a file modified externally, WHEN the change is detected, THEN
  the file-changed infobar appears; reload replaces editor content with the
  disk version (`06-files.md` §4).
- **G4** GIVEN unsaved changes and autosave period P, THEN within P seconds
  a snapshot file exists under the state dir with the current text and
  original-path metadata; GIVEN the app is killed and restarted, THEN the
  content is restored into a window with the recovery banner, and the
  document is dirty with its original file re-associated (`06-files.md` §5).
- **G5** GIVEN a Latin-1 file edited to include an em dash, WHEN saved, THEN
  the file is rewritten as UTF-8 without error and reopens correctly.
- **G6** WHEN a file is opened that is already open in another window, THEN
  that window is focused and no duplicate opens.

## H. Search & replace

- **H1** WHEN Ctrl+F is pressed with "lorem" selected, THEN the search entry
  contains "lorem" and all occurrences are highlighted; next/previous
  navigate and wrap around.
- **H2** WHEN regex mode is on with term `\d+` and Replace All with `N`,
  THEN every number run is replaced in one undo step (`07-search-and-replace.md`).

## I. Export

- **I1** WHEN PDF quick-export is chosen and a target file confirmed, THEN a
  PDF exists at the target with A4 pages.
- **I2** WHEN HTML quick-export is chosen, THEN the HTML renders correctly
  in a browser with styles embedded (no external file dependencies).
- **I3** GIVEN TeX absent, WHEN LaTeX-PDF is selected in advanced export,
  THEN the missing-TeX guidance page replaces the options.
- **I4** WHEN Copy HTML is invoked, THEN the clipboard contains the HTML
  fragment of the document.

## J. Appearance & preferences

- **J1** WHEN the scheme is switched to Dark, THEN chrome, editor colors,
  and preview restyle immediately; after restart the choice persists
  (`09-appearance.md`).
- **J2** WHEN "Large text" is enabled, THEN the editor uses a larger base
  size and the thresholds for shrinking move up.
- **J3** WHEN the input flavor is changed to CommonMark, THEN previews
  re-render using CommonMark semantics.

## K. Windows & integration

- **K1** WHEN the app is launched twice with `file.md`, THEN one instance
  runs and the file opens in it (`07-integration.md` §4).
- **K2** WHEN a document is dirty, THEN logout is inhibited; clearing the
  dirty state releases it.
- **K3** WHEN window size is changed and the app restarted, THEN new windows
  open with the last size/maximized/fullscreen state.
- **K4** WHEN the recents popover is opened, THEN it lists Markdown files
  from the recently-used store, omits deleted files, and removal works.
- **K5** WHEN `--new-window` is passed to the running instance, THEN a new
  empty window appears.
