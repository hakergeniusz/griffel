# 03/11 — Inline previews ("peek" popovers)

A popover anchored at the cursor (`02-ui-layout.md` §10) that previews the
construct under the cursor. Opened via the editor context menu's "Peek" item
or Ctrl+click (with no selection).

## 1. What is previewed

The popover examines the **current line** around the cursor and matches, in
priority order:

1. **Math** — `$…$` or `$$…$$`: renders the formula to an image with the
   local LaTeX toolchain and shows it (scaled to fit a ~400×300 px popover,
   scrollable if larger).
   - If LaTeX is not installed: an error message stating so.
   - If the formula is invalid: an error message quoting the formula.
2. **Image** — `![alt](path)`: shows the image (local path resolved against
   the document's folder, percent-decoded; file/remote URL forms accepted).
   Remote URLs fall through to the web preview (3). Missing file: a
   "not found" state.
3. **Link** — `[label](url)` or `<url>` autolink: loads the target page in a
   small web view (scaled down so desktop pages fit). URL without scheme
   gets `http://` prepended.
4. **Footnote reference** — `[^id]`: shows the text of the footnote
   definition `[^id]: …` found elsewhere in the document (indented
   continuation lines de-indented for display). No matching definition: a
   "no matching footnote" message.
5. **Any single word**: shows a dictionary definition of the word, looked up
   from an online dictionary service (WordNet database via a DICT protocol
   server): word class (noun/verb/adjective/adverb) headings with numbered
   definitions. No entry or offline: "not found" state.

## 2. Popover behavior

- While content loads, a spinner shows (with a tiny delay to avoid flashing
  on instant results).
- Content replaces the spinner when ready; errors show the error page.
- Closing the popover discards the content; reopening re-computes.
- The popover is ~400×300 px; content scrolls within it.

## 3. Failure and privacy notes

- Network-dependent features (web page links, dictionary) fail gracefully to
  the "not found" state when offline; formula rendering and images work
  offline (given local LaTeX / files).
- The dictionary lookup sends the hovered word to the dictionary service —
  this is user-triggered only (never automatic).
