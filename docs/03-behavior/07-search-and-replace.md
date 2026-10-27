# 03/07 — Search and replace

Activated by Ctrl+F (find) or Ctrl+H (find & replace) or the headerbar search
toggle. The bar appears under the headerbar (`02-ui-layout.md` §6).

## 1. Find

- Opening find with a selection pre-fills the entry with the selected text.
- Typing updates matches live; all occurrences are highlighted (yellow
  background over the normal highlighting).
- The previous/next buttons jump to the previous/next occurrence, selecting
  it and smooth-scrolling it to the center of the view. Searching wraps
  around the document ends.
- Options: **case sensitive** toggle (default off) and **regular
  expression** toggle (default off). Regex uses standard regex syntax;
  a regex search term applies to matching only (replacement backreferences
  behave per the platform search-and-replace conventions).
- With no matches, navigation buttons do nothing (and are insensitive);
  the entry indicates no-match state per platform conventions.

## 2. Replace

- The replace toggle reveals the replace row (replace entry + "Replace" +
  "Replace All"). Activating replace mode also activates find mode; closing
  find closes replace mode.
- **Replace** replaces the currently selected match (only enabled when the
  current selection is exactly an occurrence) and advances to the next match.
- **Replace All** replaces every occurrence in the document in one undoable
  step.
- Replacements run through the same search options (case, regex).

## 3. Interaction with the rest of the UI

- Closing the search bar (Esc or toggling find off) removes highlighting,
  closes replace mode, and returns focus to the editor.
- While the search bar is visible, the headerbar/bottom-bar auto-hide is
  suppressed (`13-autohide.md`).
- Search state is per window: the option toggles persist while the window
  lives; the entry text is not restored on reopen — it is pre-filled only
  when find is opened with a selection (which always overwrites the entry).
