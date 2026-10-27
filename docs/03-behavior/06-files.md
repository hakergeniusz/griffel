# 03/06 — Files: open, save, external changes, recovery

See also `05-file-formats-and-persistence.md` for on-disk formats and
`06-edge-cases.md` for failure modes.

## 1. Document state

Each window holds exactly one document with:

- **Content** — the text in the editor.
- **Backing file** — optional; a URI (local file or remote). A document
  without one is a "New File".
- **Dirty flag** — set by any edit, cleared by a successful save.
- **Encoding** — the encoding the file was loaded with; used for saving
  (see `05-file-formats-and-persistence.md`).

The title shown in the headerbar is the file's display name (for a new
document, a placeholder name like "New File"), prefixed with a dirty
indicator (`• `) while unsaved changes exist. The subtitle is the parent
folder. While the document is dirty, the app requests the session manager
not to log out silently (inhibit logout).

## 2. Opening

Entry points:

- **Open dialog** (Ctrl+O or headerbar/menu): native open dialog with
  Markdown and plain-text filters (`02-ui-layout.md` §11). Choosing a file
  routes through the application-level open (§2.1).
- **Recents popover** (`02-ui-layout.md` §8): opens that file.
- **Command line / file manager**: files passed as arguments or activated via
  file association are routed to the running instance (see
  `07-integration.md`).
- **Tutorial**: opens a bundled sample document (read-write, in a window) —
  `03-behavior/12-windows.md` §3.

### 2.1 Application-level open routing

When one or more files are opened:

- If the file is already open in a window of this instance, that window is
  presented (focused) instead of opening a duplicate.
- Otherwise the file is loaded into an existing *empty, unmodified* window if
  one exists; if not, a new window is created for it.
- Files that don't exist are skipped with a log warning (no dialog).

### 2.2 Loading

1. The file content is read and decoded (UTF-8 first, then detected encoding
   — `06-edge-cases.md`).
2. The text replaces the editor content; the cursor is placed at the start of
   the document; undo history is cleared.
3. The per-file spellcheck language is applied if previously stored.
4. The file's preview security level is evaluated (`03-live-preview.md` §5).
5. The file is added to the recents list (unless it's an internal resource).
6. A file monitor is installed on the file (§4).
7. If loading fails, an error dialog is shown and the window resets to a new
   empty document.

## 3. Saving

- **Save** (Ctrl+S): if the document has a backing file, writes the full text
  to it (atomic replace). Otherwise behaves like Save As.
- **Save As** (Ctrl+Shift+S): native save dialog pre-filled with the document
  name (`.md` appended if missing), defaulting to the document's folder.
  Confirming assigns the chosen file as the backing file and saves.
- **Feedback**: a slim progress bar appears at the top of the window and
  animates; if the save completes quickly it fades out. It pauses if further
  edits happen mid-save, then completes. The bar's fill color may use a
  special accent palette on certain awareness days/months (decorative; see
  `09-open-questions.md`).
- **On success**: dirty flag cleared, title updated, the file is added to
  recents, the recovery snapshot is deleted (§5), the file monitor is
  re-armed (after a short grace delay so the app's own write doesn't trigger
  the external-change warning).
- **On failure**: an error dialog with the reason; the dirty flag stays set;
  the progress bar fades out.
- Files are added to the platform's recently-used list only after a
  successful load/save.

## 4. External changes

While a document has a backing file, the app watches it. If the file changes
on disk (any external modification):

- A warning infobar appears (`02-ui-layout.md` §7): "file changed on disk"
  with an explanation and one action button — **discard my version and
  reload** — plus a close button.
- Choosing reload re-reads the file from disk (full reload path, §2.2),
  replacing current content; the infobar closes.
- Choosing close (or saving) hides the infobar; saving cancels the watch
  until the save completes and re-arms it afterwards.

## 5. Autosave snapshots (crash recovery)

Unsaved work is periodically snapshotted to disk:

- **Trigger**: the first edit of a clean document starts a timer (period =
  autosave period preference, default 5 seconds). While the document remains
  dirty the timer repeats, refreshing the snapshot every period; when the
  document becomes clean (saved) the timer stops.
- **Location**: a `snapshots/` folder in the user's state directory
  (`XDG_STATE_HOME`-based), created with mode 0750; snapshot files get
  random unique names and contain the raw document text.
- Empty content is never snapshotted. If the text can't be encoded for the
  snapshot, the attempt is silently skipped.
- The snapshot records the backing file's path as extended metadata, so
  recovery can reconnect to the original file.
- **Recovery at startup**: on application start, any existing snapshots are
  loaded — each into an empty window if available, otherwise into a new
  window. The window shows the recovery banner (`02-ui-layout.md` §7)
  announcing that the file was recovered from an autosave. The document is
  marked dirty, and if the snapshot recorded an original path, the backing
  file is re-associated (saving writes to the original file).
- **Deletion**: the snapshot is deleted when the document is saved
  successfully, and when the window is closed or destroyed (including after
  the user confirms discarding unsaved changes in the close dialog). A
  pending autosave timer is cancelled at the same time. Consequently the
  snapshot survives only abnormal termination (crash, force-kill, session
  loss).
- **Autosave is a snapshot mechanism, not a silent save to the backing
  file**: the backing file is never written without an explicit Save.

## 6. New document

- The headerbar "New" button opens a **new window** with an empty document
  (it never reuses the current window).
- The in-window "new document" flow (used internally after load failures)
  resets the current window to an empty, clean document after the
  unsaved-changes check.

## 7. Closing windows and quitting

- **Close window** (Ctrl+W or window close): if the document is clean, the
  window closes. If dirty, the unsaved-changes dialog appears
  (`02-ui-layout.md` §11): Save (attempts save then closes; a failed save
  aborts the close), Discard (closes, destructive styling), Cancel (aborts).
- **Quit** (Ctrl+Q): every window runs the same close sequence; the app quits
  only if all windows close.
