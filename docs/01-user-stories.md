# 01 — User stories

Capabilities grouped by feature. Each story is phrased from the user's
perspective; detailed behavior lives in the referenced documents.

## Writing and editing

- As a writer, I open the app and immediately get an empty document focused
  for typing, with no dialogs in my way. (`03-behavior/01-editing.md`,
  `02-ui-layout.md`)
- As a writer, I see my text as a centered column of limited line length that
  stays centered when I resize the window. (`03-behavior/01-editing.md`)
- As a writer on a small window, I see the font shrink gracefully so the
  column still fits. (`03-behavior/01-editing.md`)
- As a writer, I can increase the text size with a single preference when my
  screen is far away. (`03-behavior/10-preferences.md`)
- As a touch typist, when I press Enter in a list the next list marker is
  inserted for me, numbers increment, and checkboxes reset to unchecked.
  (`03-behavior/01-editing.md`)
- As a list writer, I can indent and outdent list items with Tab /
  Shift+Tab and cycle bullet characters. (`03-behavior/01-editing.md`)
- As a fast typist, when I type an opening bracket or quote the closing one
  is inserted for me, and typing the closing one skips over it.
  (`03-behavior/01-editing.md`)
- As a writer, I can undo and redo all my typing. (`03-behavior/01-editing.md`)
- As a writer, I can drag an image file, another file, or a URL onto the
  editor and get correct Markdown syntax inserted. (`03-behavior/01-editing.md`)
- As a multilingual writer, my spelling errors are underlined and I can pick
  corrections and languages from the context menu; the language is remembered
  per file. (`03-behavior/01-editing.md`)
- As a writer, I can turn spell checking on or off. (`03-behavior/01-editing.md`)

## Formatting

- As a Markdown author, I can apply bold, italic, strikethrough, headings,
  block quotes, inline code/code blocks, the three list kinds, links, images,
  and tables from a toolbar or shortcuts; the commands work on my selection
  or insert placeholder text I can type over. (`03-behavior/02-formatting.md`)
- As a Markdown author, pressing the same heading command repeatedly cycles
  the heading level. (`03-behavior/02-formatting.md`)
- As a Markdown author, applying a command to text that already carries that
  formatting removes it (toggle). (`03-behavior/02-formatting.md`)
- As a Markdown author, I can insert a table of a chosen size by dragging
  over a size picker. (`03-behavior/02-formatting.md`)

## Reading my own text (in-place highlighting)

- As a writer, I see Markdown semantics rendered subtly in the source: bold
  text bold, italics italic, struck text struck, code on a tinted background,
  link/image syntax dimmed with the label highlighted, headings bolded and
  outdented to hang into the margin, horizontal rules centered, fenced code
  blocks on a tinted background. (`03-behavior/01-editing.md`)

## Preview

- As an author, I can toggle a live HTML preview of my document.
  (`03-behavior/03-live-preview.md`)
- As an author, I can arrange the preview beside, below, in place of, or in a
  separate window relative to the editor, with smooth transitions, and my
  choice is remembered. (`03-behavior/03-live-preview.md`, `03-behavior/12-windows.md`)
- As an author, as I type, the preview updates automatically.
  (`03-behavior/03-live-preview.md`)
- As an author, scrolling the editor scrolls the preview proportionally and
  vice versa. (`03-behavior/03-live-preview.md`)
- As a cautious author, when I open a file containing raw HTML the app asks
  me whether to render it fully or restrict scripts/HTML, and remembers my
  choice as a global preference if I want. (`03-behavior/03-live-preview.md`)
- As an author, clicking a web link in the preview opens my browser, not the
  preview. (`03-behavior/03-live-preview.md`)
- As a mathematician, I can peek at rendered formulas; as a reader I can peek
  at images, link targets, footnotes, and dictionary definitions without
  leaving the editor. (`03-behavior/11-inline-previews.md`)

## Focus and flow

- As a distracted writer, I can switch on Focus Mode so everything but the
  current sentence fades and my cursor line stays vertically centered.
  (`03-behavior/04-focus-and-hemingway.md`)
- As an impulsive writer, I can switch on Hemingway Mode so text cannot be
  deleted, helping me keep moving forward. (`03-behavior/04-focus-and-hemingway.md`)
- As any writer, the header and bottom bars fade out while I type and come
  back when I move the mouse toward them, when they're needed, or when I
  save. (`03-behavior/13-autohide.md`)

## Statistics

- As a writer with a word goal, I can see live characters/words/sentences/
  paragraphs/reading-time counts and choose which one is always displayed.
  (`03-behavior/05-statistics.md`)
- As an editor, when I select text I see counts for the selection versus the
  whole document. (`03-behavior/05-statistics.md`)

## Files

- As a user, I can open Markdown or plain-text files via dialog, recents
  popover, command line, or file manager association. (`03-behavior/06-files.md`)
- As a user, I can save, save as, and see save progress. (`03-behavior/06-files.md`)
- As a user, if the file changes on disk while I have it open, I'm told and
  can reload or keep my version. (`03-behavior/06-files.md`)
- As a clumsy user, if the app or machine crashes, my unsaved work is
  recovered from automatic snapshots the next time I start the app.
  (`03-behavior/06-files.md`, `05-file-formats-and-persistence.md`)
- As a user with files in odd encodings, they are detected, displayed, and
  saved back without corruption when possible. (`06-edge-cases.md`)

## Searching

- As a reviser, I can find text (optionally as a regular expression, with
  case toggle), jump between matches, and replace or replace-all, with
  matches highlighted. (`03-behavior/07-search-and-replace.md`)

## Exporting and sharing

- As an author, I can export to PDF, HTML, or ODT in one click.
  (`03-behavior/08-export.md`)
- As a power user, I can export to many more formats with options such as
  table of contents, numbered sections, page/slide size, self-contained HTML,
  syntax highlighting style, and incremental bullets. (`03-behavior/08-export.md`)
- As a blogger, I can copy my document as HTML to the clipboard.
  (`03-behavior/08-export.md`)

## Appearance and preferences

- As a user, I can pick light, sepia, dark, or follow-the-system appearance;
  the whole app, editor, and preview restyle together. (`03-behavior/09-appearance.md`)
- As a user, I can toggle spell checking, auto-hide bars, large text, choose
  the Markdown flavor used for preview/export, and choose the preview
  security policy. (`03-behavior/10-preferences.md`)

## Windows

- As a multitasker, I can open any number of document windows, including a
  brand-new empty one, and close them independently with an unsaved-changes
  safeguard. (`03-behavior/12-windows.md`)
- As a learner, I can open a built-in tutorial document from the menu.
  (`03-behavior/12-windows.md`)
