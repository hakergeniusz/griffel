# 04 — Keyboard shortcuts

Shortcut values are requirements (users will rely on muscle memory). Where a
shortcut is listed as *(advertised)*, the reference behavior lists it in the
shortcuts window but the Implementer should verify it is actually registered
(see `09-open-questions.md`). Standard platform text-editing bindings (arrow
keys, Home/End, etc.) apply implicitly.

## General

| Shortcut | Action |
|---|---|
| Ctrl+N | New window (empty document) |
| Ctrl+O | Open file dialog |
| Ctrl+S | Save |
| Ctrl+Shift+S | Save As |
| Ctrl+W | Close document/window (with unsaved-changes guard) |
| Ctrl+Q | Quit application (guarding all windows) |
| Ctrl+, | Open preferences |
| F7 | Toggle spell checking *(advertised — see open questions)* |

## Modes

| Shortcut | Action |
|---|---|
| Ctrl+D | Toggle Focus Mode |
| Ctrl+T | Toggle Hemingway Mode |
| Ctrl+P | Toggle preview |
| F11 | Toggle fullscreen |

## Find

| Shortcut | Action |
|---|---|
| Ctrl+F | Open search bar |
| Ctrl+H | Open search bar with replace row |

## Markdown *(advertised — see open questions)*

| Shortcut | Action |
|---|---|
| Ctrl+R | Insert horizontal rule |
| Ctrl+1 … Ctrl+4 | Insert heading level 1–4 |
| Ctrl+U | Insert bullet list item |
| Ctrl+I | Wrap italic |
| Ctrl+B | Wrap bold |
| Ctrl+Shift+D | Wrap strikethrough |

## Editing (platform-standard)

| Shortcut | Action |
|---|---|
| Ctrl+C | Copy |
| Ctrl+X | Cut |
| Ctrl+V | Paste |
| Ctrl+A | Select all |
| Ctrl+Z | Undo |
| Ctrl+Shift+Z | Redo |
| Tab / Shift+Tab | Indent / outdent (smart list behavior, `01-editing.md` §2.3) |

## Mouse

| Input | Action |
|---|---|
| Ctrl+Click | Open inline preview popover at cursor |
| Right-click | Editor context menu (incl. "Peek", spelling) |
| Drag & drop | Insert Markdown link/image/text (`01-editing.md` §3) |
| Pointer to top edge | Reveal auto-hidden bars (`13-autohide.md`) |
