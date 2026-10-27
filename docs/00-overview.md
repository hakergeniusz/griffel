# 00 — Overview

## What this document set is

This is a clean-room behavioral specification of a distraction-free Markdown
editor for the GNOME desktop. It describes **what the application does and how
it looks and behaves from the user's perspective**. It is written so that a
developer who has never seen the reference application can build a
functionally equivalent app. It deliberately does not describe any internal
architecture, code, or technology choices of the reference implementation.

In this document set, **"the app"** means the application being specified (the
one to be built). **"The reference app"** means the original product whose
behavior this spec mirrors; it is mentioned only where a fact about it is
useful context (see `10-provenance.md`).

## Product purpose

The app is a plain-text editor for Markdown with two defining qualities:

1. **Distraction-free writing.** The editing surface is a single, centered
   column of monospace text with generous margins. Header, toolbar, and status
   elements fade away while the user types and reappear on demand. Two
   dedicated writing modes (Focus Mode and Hemingway Mode, see
   `03-behavior/04-focus-and-hemingway.md`) further reduce visual and editing
   distractions.

2. **Markdown with live rendering.** The editor itself stays plain text, but
   Markdown syntax is subtly highlighted in place (bold text appears bold,
   links are dimmed around the text, headings outdent, etc.), and a live HTML
   preview can be shown beside, above, or instead of the editor, or in a
   separate window. The app is not a WYSIWYG editor; the user always edits the
   underlying Markdown source.

## Target users

- Writers, bloggers, students, and technical authors who draft in Markdown.
- Users who want a calm, minimal writing environment on the Linux desktop,
  consistent with GNOME human-interface conventions.
- Users who want to turn Markdown into PDF, HTML, office documents, slides,
  and other formats for publishing (via an external conversion engine, see
  `03-behavior/08-export.md`).

## Feature summary

- **Editor**: plain-text editing of Markdown in a centered column; adaptive
  font size; auto-pairing of brackets and quotes; smart list continuation and
  indentation; smart bullet rotation; spell checking; drag-and-drop of files,
  images, and URLs as Markdown links; unlimited undo/redo.
- **In-place Markdown highlighting**: inline styles for bold/italic/
  strikethrough/code, dimmed link and image syntax, outdented and bolded
  headings, centered horizontal rules, code-block backgrounds.
- **Formatting toolbar**: buttons for headings, bold, italic, strikethrough,
  bulleted/numbered/checklist items, block quotes, code, links, images, and
  tables.
- **Live preview**: rendered HTML preview with four layout modes (half-width,
  full-width, half-height, separate window), animated layout transitions,
  synchronized proportional scrolling, and a security model for files that
  contain raw HTML (see `03-behavior/03-live-preview.md`).
- **Inline "peek" popovers**: preview of formulas, images, links, footnotes,
  and dictionary definitions of the word under the cursor
  (`03-behavior/11-inline-previews.md`).
- **Writing modes**: Focus Mode (dims everything but the current sentence,
  centers the cursor) and Hemingway Mode (deletion disabled)
  (`03-behavior/04-focus-and-hemingway.md`).
- **Statistics**: characters, words, sentences, paragraphs, and reading time,
  for the whole document or the current selection, in a status bar
  (`03-behavior/05-statistics.md`).
- **File handling**: open/save/save-as with Markdown and plain-text filters,
  a recents popover with search, external-change detection with
  reload-or-keep choice, per-file spellcheck-language memory, autosaved
  recovery snapshots, encoding detection (`03-behavior/06-files.md`,
  `05-file-formats-and-persistence.md`).
- **Search & replace**: find bar with next/previous match, optional regular
  expressions, case sensitivity, and replace/replace-all
  (`03-behavior/07-search-and-replace.md`).
- **Export**: quick export to PDF/HTML/ODT plus an advanced export dialog
  with many output formats and options; copy-as-HTML
  (`03-behavior/08-export.md`).
- **Appearance**: four color schemes (follow system, light, sepia, dark) that
  restyle both the UI chrome and the editor and preview
  (`03-behavior/09-appearance.md`).
- **Multi-window**: any number of independent document windows; a second
  window can host the preview (`03-behavior/12-windows.md`).

## Explicit non-goals

The app does **not** provide any of the following:

- WYSIWYG editing; the document is always shown as Markdown source.
- Tabbed or side-by-side document management inside one window; each window
  holds exactly one document.
- Project or folder management, file trees, or workspace concepts.
- A file manager, diffing, version control, or cloud sync.
- User-installable plugins or extension APIs.
- Collaborative or real-time editing.
- Editing file formats other than plain text (no .docx/.odt input).
- Rich configuration of fonts or custom font choice (only a single
  "larger text" preference exists).
- Custom themes or user-supplied CSS for the editor or preview.
- Built-in PDF/HTML rendering engines of its own for export — conversion is
  delegated to an external converter (see `03-behavior/08-export.md`).

## External standards and dependencies the behavior relies on

The spec references these by name; the Implementer chooses how to satisfy
them:

- **Markdown flavors** for preview and export: the selectable input formats
  are Pandoc's Markdown, CommonMark, GitHub-Flavored Markdown, MultiMarkdown,
  and the original Markdown (strict). See `03-behavior/03-live-preview.md`
  and `03-behavior/08-export.md`.
- **Document converter**: all HTML rendering and all exports are produced by
  invoking an external Pandoc-compatible converter. The app must surface the
  converter's output and errors to the user but performs no conversion itself.
- **LaTeX toolchain**: formula peek rendering and LaTeX-based PDF export use
  a locally installed LaTeX distribution when present.
- **freedesktop.org / GNOME platform conventions** for desktop integration
  (`07-integration.md`), settings storage (`05-file-formats-and-persistence.md`),
  and recents tracking.

## Document map

| Document | Contents |
|---|---|
| `00-overview.md` | This overview |
| `01-user-stories.md` | Capabilities grouped by feature |
| `02-ui-layout.md` | Windows, panes, bars, menus, dialogs |
| `03-behavior/` | One file per feature area |
| `04-keyboard-shortcuts.md` | Every shortcut and its effect |
| `05-file-formats-and-persistence.md` | Formats, settings, snapshots |
| `06-edge-cases.md` | Errors, empty states, unusual input |
| `07-integration.md` | Desktop integration and packaging |
| `08-acceptance-criteria.md` | Testable behavior-level criteria |
| `09-open-questions.md` | Ambiguities for the human to decide |
| `10-provenance.md` | How this spec was produced |
