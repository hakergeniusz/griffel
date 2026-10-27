# 03/13 — Auto-hide bars

The headerbar and the bottom bar (toolbar + stats) fade away while the user
types, and return on demand. This is the app's core distraction-free
behavior.

## 1. Hiding

Triggered when all of the following hold:

- The auto-hide preference is on (default).
- The user just typed (an edit was made) — hiding begins with the first
  keystroke that dirties a clean document and on subsequent edits.
- The window is in the **wide** layout (auto-hide never engages in the
  narrow layout, where space is already scarce).
- None of these is active/visible: the search bar, the file-changed infobar,
  the recovery banner after a snapshot restore.

Hiding fades both bars out (~450 ms crossfade).

## 2. Revealing

The bars reappear when any of these happens:

- The pointer enters the top region of the window (moving the mouse toward
  the top edge).
- The editor loses keyboard focus (e.g. the user interacted with another
  widget or window).
- A save starts.
- The search bar, file-changed infobar, or recovery banner becomes visible.
- Fullscreen is exited.

## 3. Bottom bar specifics

- The toolbar row and the stats button hide together with the headerbar and
  share the same reveal triggers.
- While the toolbar's expand toggle is active (row visible) the bottom bar's
  background is shown; when the row is hidden only the toggle and stats
  button remain.
- The bottom bar ignores pointer events in the empty middle area between
  toolbar and stats button so clicks pass through to the editor.
