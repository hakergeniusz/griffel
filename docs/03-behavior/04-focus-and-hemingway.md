# 03/04 — Focus Mode and Hemingway Mode

Two independent, toggleable writing modes. Both are exposed in the main menu
and via shortcuts (`04-keyboard-shortcuts.md`).

## 1. Focus Mode

A typewriter-style mode that keeps attention on the current sentence.

- Toggle: Ctrl+D, main menu checkmark.
- **Dimming**: all text outside the sentence containing the cursor is shown
  grayed out. The current sentence is shown in normal color. Sentence
  boundaries follow the platform's text iterator rules (sentence-ending
  punctuation followed by whitespace/newline).
- Re-evaluated continuously as the cursor moves or the text changes, so the
  lit region follows the cursor.
- **Typewriter scrolling**: the view smooth-scrolls so the cursor line sits
  vertically centered; applied on cursor movement, typing, clicks, and mode
  activation.
- **Margins**: the text area's top and bottom margins become half the
  viewport height, so the first/last line can also reach the center.
- **Spell check disabled**: underlines are suppressed while the mode is on
  and restored when it is turned off (the spellcheck preference itself is not
  changed).
- Switching the mode on grabs focus to the editor and immediately applies
  dimming and centering.

## 2. Hemingway Mode

A mode where text cannot be deleted — deletions are blocked, edits are
additive only.

- Toggle: Ctrl+T, main menu checkmark. The state is remembered across
  sessions (preference).
- **Blocked operations**: backspace, delete, deleting a selection (including
  replacing a selection by typing or pasting over it), and Shift+Tab
  outdenting are all blocked; the text never changes.
- **Feedback on a blocked attempt**:
  - The editor area plays a small horizontal "shake" animation.
  - A toast appears (subject to the frequency rule below) explaining that
    text cannot be deleted in this mode, with an action button that opens a
    short informational dialog about the mode. The toast auto-dismisses
    after ~3 seconds.
- **Toast frequency rule**: the toast is shown at most for the first 3
  blocked attempts per installation (a counter is persisted). Additionally,
  if the user attempts deletions 4 times within roughly a minute, the counter
  resets to zero and the toast is shown again — so a user who keeps hitting
  the wall gets reminded, while a habitual user is not nagged.
- Undo (Ctrl+Z) is not blocked by this mode; however since deletions never
  happen, undo in practice undoes prior insertions.
- Internal operations that must delete text (e.g. exiting an empty list line
  with Enter, list indentation rewrites) still work while the mode is on;
  they are not treated as user deletions.
