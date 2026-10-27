# 03/12 — Windows, recents, tutorial, about

## 1. Multiple windows

- The app is a single instance; any number of document windows may exist.
- Each window is fully independent: its own document, dirty state, preview,
  search bar, modes (Focus Mode is per window; Hemingway Mode is global via
  the preference).
- A new empty window opens via the headerbar New button, Ctrl+N, the
  command-line `--new-window` option, or the desktop "New Window" action.
- Windows of the same app do not steal each other's input; only the
  presented window takes focus.

## 2. Opening files into windows

Routing rules when files are opened (dialog, recents, CLI, associations):
`03-behavior/06-files.md` §2.1 — reuse an existing window showing that file;
else reuse an empty unmodified window; else open a new window.

## 3. Tutorial document

The main menu offers opening a built-in tutorial: a bundled Markdown
document describing the app's features, loaded into a window like any other
file (it is an internal resource: it is not added to recents and starts
unsaved).

## 4. Recents

- Every successfully loaded or saved backing file is added to the platform's
  recently-used list.
- The recents popover (`02-ui-layout.md` §8) filters that list to Markdown
  files, prunes entries whose files no longer exist, allows per-entry
  removal and case-insensitive search by name, and opens the file in this
  instance on activation.

## 5. About dialog

Standard GNOME about dialog with the app's identity and version, website,
upstream issue tracker link, developer and designer credits, plus two
external links: a donation page and the translation platform page. A
"problem details" expander shows diagnostic info: app version, whether
running sandboxed, versions of the platform toolkits and the document
converter, OS name/version, display backend and renderer, and the current
values of all settings.

## 6. Keyboard shortcuts window

Standard GNOME shortcuts window listing all shortcuts grouped as in
`04-keyboard-shortcuts.md`.

## 7. Session behavior

- Window geometry (size, maximized, fullscreen) is saved on change and
  restored for new windows (`05-file-formats-and-persistence.md`).
- The preview on/off default for new windows is the remembered preference.
- Autosaved snapshots are restored at startup (`06-files.md` §5).
