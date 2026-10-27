# 02 — UI layout

This document describes the visual arrangement of every screen element and
its states. Visual design (colors, exact spacing, icons, wording) is left to
the Implementer unless stated; the structure, presence, order, and behavior
of elements are requirements.

## 1. Main window

A single-window, headerbar-style (client-side decorated) application window.
Default size 1000×600 px; size, maximized state, and fullscreen state are
remembered between sessions (see `05-file-formats-and-persistence.md`).

```
┌────────────────────────────────────────────────────────────┐
│ [New] [Open▾] [Save▾]        Title           [-layout-] [⚙]│  ← headerbar (fades)
│      · subtitle (folder path) ·                            │
│ ┌────────────────────────────────────────────────────────┐ │
│ │ (search bar appears here when active)                  │ │
│ │ (recovery banner / file-changed infobar appear here)   │ │
│ │                                                        │ │
│ │            centered Markdown text column               │ │
│ │            ~66 characters wide, monospace              │ │
│ │                                                        │ │
│ │ [toolbar ▾]                        [stats button ▴]    │ │  ← bottom bar (fades)
│ └────────────────────────────────────────────────────────┘ │
└────────────────────────────────────────────────────────────┘
```

The window content is one vertical stack:

1. **Headerbar area** (see §2).
2. **Search bar** (see §6), directly under the headerbar.
3. **Banner/infobar strip** (see §7), under the search bar.
4. **Editor area** (see §3), which can be split to show the **preview
   panel** (see §4).
5. **Bottom bar** overlaying the bottom edge of the editor area (see §5).

A thin save-progress bar may appear as an overlay at the very top of the
window content, above everything else, while a save is in flight.

### Narrow layout

Below a window width of 500 px, the headerbar switches to a compact variant
and the preview layout is forced to "full-width" (preview replaces the
editor; see §4). The compact headerbar contains: a "new window" button, an
"Open" menu button, a preview toggle button, and the main-menu button; the
save/export functions move into the main menu. Below a smaller width again,
the toolbar collapses to a compact variant (fewer, scrollable buttons; the
"show extra controls" toggle disappears) and the stats button shows an
abbreviated label.

## 2. Headerbar

A standard GNOME headerbar (window controls at the edges, drag to move the
window, double-click to maximize). From left to right:

| Element | Type | Purpose |
|---|---|---|
| New | Button | Opens a new empty document window (`03-behavior/12-windows.md`) |
| Open | Menu button | Opens the recents popover (§8) |
| Save | Split button | Activates Save; its attached menu opens the export menu (§9) |
| Preview layout switcher | Popover button | Chooses preview layout (§4); visible only in wide layout |
| Find | Toggle button | Toggles the search bar (§6) |
| Main menu | Menu button | Opens the main menu (§9) |
| Exit fullscreen | Button (fullscreen only) | Leaves fullscreen |

The headerbar title shows the document title; an unsaved document is marked
by a leading dirty indicator (a bullet character prefix). The subtitle shows
the containing folder path (empty for never-saved documents). Hovering the
headerbar shows the path as a tooltip.

**States.** The entire headerbar area fades out (auto-hide) while typing when
the auto-hide preference is on, and fades back in on: pointer entry into its
area, editor losing focus, a save starting, or when any banner/infobar or the
search bar is visible. Auto-hide never engages in the narrow layout. In
fullscreen the headerbar behaves the same as windowed.

## 3. Editor area

A vertically scrolling, non-wrapping-horizontally text area showing the
document. Layout rules:

- The text column is horizontally centered; its width is the "characters per line" setting
  (preference, default 66) times the character width of the current font.
  Remaining space becomes equal left/right margins.
- The font is a monospace face. Its size adapts in discrete steps to the
  window width so that the column plus a small padding always fits: as the
  window narrows past each threshold, the size steps down; thresholds scale
  up when the "larger text" preference is on.
- Top margin (below the headerbar) ≈ 34 px plus headerbar height; bottom
  margin ≈ 64 px. In Focus Mode the top/bottom margins are half the viewport
  height so the cursor can be centered (see `03-behavior/04-focus-and-hemingway.md`).
- Tabs render 4 characters wide; Tab inserts a tab character.
- The text view accepts drag-and-drop (see `03-behavior/01-editing.md`) and
  has a context menu (§10).

## 4. Preview panel and layout modes

When preview is active, the window content is split into an **editor pane**
and a **preview pane** separated by a thin divider, arranged according to the
layout mode:

| Mode | Arrangement | Editor | Preview |
|---|---|---|---|
| Full-width (default) | side by side | hidden (slides out) | full window |
| Half-width | side by side | left half | right half |
| Half-height | stacked | top half | bottom half |
| Windowed | editor only | full window | separate top-level window (§4.1) |

Layout is an enum preference remembered across sessions; switching modes is
animated (slide/fade, roughly 250 ms). In the narrow layout the mode is
forced to full-width. The switcher control in the headerbar shows the current
mode's icon and opens a popover listing the four modes with icons, names, and
a checkmark on the active one; choosing one closes the popover and switches
with animation.

The preview pane itself is a small state stack:

- **Spinner** — shown while the conversion is pending (also shown whenever a
  file's security level is not yet determined).
- **Rendered web view** — the HTML preview (see `03-behavior/03-live-preview.md`).
  While a reload is in flight, the last rendered frame is shown as a static
  image and crossfades into the new render to avoid flashing.
- **Security warning page** — shown when a file may contain unsafe HTML and
  the "ask" policy applies: a warning icon, explanatory title and text, and
  two pill buttons: "load unrestricted" and "load restricted" (restricted is
  the emphasized one). See `03-behavior/03-live-preview.md`.

The preview pane's top margin matches the headerbar height in the side-by-side
and full-width modes so content aligns with the editor column.

### 4.1 Windowed preview

In windowed mode a separate top-level window titled with the document title
plus "Preview" hosts the preview. Closing that window turns the main
window's preview off (equivalent to toggling preview off). Destroying the
main window destroys the preview window.

## 5. Bottom bar

An overlay strip pinned to the bottom edge of the editor area, composed of:

- **Toolbar** (left): a row of flat formatting buttons, one per action, in
  this order: bold, italic, strikethrough, heading, bullet list, numbered
  list, checklist, block quote, code, link, image, table. The table button
  opens a popover containing the size picker described in
  `03-behavior/02-formatting.md` §7. A trailing toggle button (chevron) shows
  or hides the whole button row; its state is remembered (hidden by default)
  and the button shows an active state while the row is visible. The row sits
  on a subtle shared background. When hidden, the toggle button remains
  available to bring it back.
- **Stats button** (right): a flat button showing the currently selected
  statistic (see `03-behavior/05-statistics.md`); clicking opens a popover
  menu to pick which statistic is the default.

The bottom bar fades with the headerbar under the same auto-hide rules. When
the window is too narrow to fit the full toolbar plus stats button, it
switches to the compact toolbar variant and the abbreviated stats button.

## 6. Search bar

A search bar that slides in below the headerbar when find is activated:

```
┌──────────────────────────────────────────────────────────┐
│ [search entry            ] [↑] [↓] │ [aA] [(.*)] [⇄]     │  replace row appears
│ [replace entry           ]        [Replace] [Replace All]│  when replace mode on
└──────────────────────────────────────────────────────────┘
```

- Search entry with placeholder; two arrow buttons for previous/next match.
- Toggle "aA" for case sensitivity; toggle "(.*)" for regular expressions.
- Toggle button to reveal the replace row: a replace entry plus "Replace"
  and "Replace All" buttons.
- Matches are highlighted in the editor (yellow background); the current
  match is selected.
- Buttons are disabled when there is no current match (replace) or no
  occurrences (navigation/replace-all).

Details: `03-behavior/07-search-and-replace.md`.

## 7. Banners and infobars

Directly below the search bar there are two transient notification strips
(each hidden unless active):

1. **Recovery banner** — an advisory banner shown after the app restored an
   unsaved snapshot at startup. It disappears as soon as the user saves.
2. **File-changed infobar** — a warning infobar with a close button and an
   action button ("discard my changes and reload"). Shown when the file on
   disk changed externally. Details: `03-behavior/06-files.md`.

While either strip is visible, the headerbar/bottom bar do not auto-hide.

## 8. Open/recents popover

A popover anchored to the "Open" headerbar button:

```
┌──────────────────────────────┐
│ [search entry            ]   │
├──────────────────────────────┤
│ ▸ filename.md            [×] │
│   /home/user/notes           │
│ ▸ other.md               [×] │
│   /home/user                 │
│ …                            │
└──────────────────────────────┘
```

- Lists recently opened documents (only Markdown-typed entries; entries whose
  files no longer exist are silently removed from recents), most-recently
  used ordering, showing display name and folder path.
- A search entry filters the list case-insensitively by name.
- Each row is activatable (opens that file in this window's app instance) and
  has a trailing remove button that deletes it from recents.
- When there are no recents, an empty-state page is shown instead of the list.
- The popover also offers opening the built-in tutorial document (§9).

## 9. Menus

### Main menu (headerbar, right)

A popover menu containing, top to bottom:

1. **Appearance selector**: an inline row of four selectable options —
   follow system, light, sepia, dark — with the active one highlighted
   (radio-style). The "system" option is hidden on systems that don't support
   color schemes. See `03-behavior/09-appearance.md`.
2. **Mode toggles**: Hemingway Mode (checkmark toggle) and Focus Mode
   (checkmark toggle).
3. **Find and Replace** (activates search bar with replace row).
4. **Preferences** (opens the preferences dialog, §11).
5. **Open Tutorial** (opens the built-in tutorial document in a new window),
   **Keyboard Shortcuts** (opens the shortcuts window), **About**
   (opens the about dialog, §12).

In the narrow headerbar the main menu additionally contains New, Open, Save,
Save As, the export items (PDF/HTML/ODT), Advanced Export, and Copy HTML.

### Export menu (attached to the Save split button)

1. **Save As…**
2. **Export to PDF** / **Export to HTML** / **Export to ODT** — one-click
   quick exports (see `03-behavior/08-export.md`).
3. **Advanced Export…** — opens the advanced export dialog.
4. **Copy HTML** — copies the document rendered as HTML to the clipboard.

## 10. Context menus

- **Editor context menu** (right-click): the platform's standard text
  editing menu (cut/copy/paste/select all, spelling suggestions when the
  cursor is on a misspelled word, spelling language selection), extended
  with a **Peek** item that opens the inline preview popover at the cursor
  (`03-behavior/11-inline-previews.md`).
- **Preview context menu**: the web view's standard menu minus navigation
  entries (reload, back, forward, stop are removed).

## 11. Dialogs

All dialogs are modal to their parent window.

### Unsaved-changes dialog (`03-behavior/06-files.md`)

Shown when closing/replacing a modified document. Contains: a title, a body
naming the document and warning that unsaved changes will be lost, and three
responses — Save (emphasized, default), Discard (destructive), Cancel
(close response). Esc activates Cancel.

### Save dialog

A native save file dialog, titled for saving, with a "Markdown" file filter
(markdown and plain-text MIME types), pre-filled file name (document name
plus `.md` extension if missing), and initial folder set to the document's
folder (or the default when never saved).

### Open dialog

A native open file dialog with two filters: "Markdown files" (markdown MIME
types and `.md` suffix) and "Plain text files".

### Preferences dialog (`03-behavior/10-preferences.md`)

An adaptive preferences dialog with a single page containing, in order:
spell check toggle; auto-hide header bar toggle; large text toggle; input
format dropdown (five Markdown flavors, each with a help button opening the
flavor's documentation website); preview security dropdown (always ask /
always restricted / always unrestricted).

### Export dialogs (`03-behavior/08-export.md`)

Quick export uses a native save dialog per format. Advanced export is an
in-app adaptive dialog with a two-pane layout: a formats list pane and an
options pane (on narrow screens the options pane covers the list; selecting a
format reveals the options). The options pane shows Export button plus only
the option groups relevant to the selected format.

### About dialog

A standard GNOME about dialog with the app's identity, version, website,
developer/artist credits, a donation link, a translations link, and a
collapsible "problem details" section containing diagnostic information
(version numbers of the app and its platform libraries, OS, renderer, and
current settings values).

### Hemingway help dialog (`03-behavior/04-focus-and-hemingway.md`)

A small informational dialog explaining Hemingway Mode, reachable from the
toast's action button.

### Keyboard shortcuts window

A standard GNOME shortcuts window listing all shortcuts in sections:
General, Modes, Find, Markdown, Copy and Paste, Undo and Redo, Selection
(see `04-keyboard-shortcuts.md`).

### Detailed error dialog

An expandable error dialog used for export failures: a short error
description plus an expander revealing technical details. Other errors use a
simple alert dialog with a message and a Close response.

## 12. Toasts

Toasts appear overlaid at the top of the editor area:

- **Hemingway toast** — shown when a deletion is attempted in Hemingway
  Mode (subject to the repetition rules in
  `03-behavior/04-focus-and-hemingway.md`); auto-dismisses after ~3 s; has an
  action button opening the Hemingway help dialog.
