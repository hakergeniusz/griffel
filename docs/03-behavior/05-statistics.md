# 03/05 — Statistics

The bottom bar's stats button (`02-ui-layout.md` §5) shows one live statistic
and opens a menu to choose which one is shown.

## 1. Counted statistics

Five statistics are computed continuously from the document text:

| Statistic | Definition |
|---|---|
| Characters | Non-whitespace characters, counting runs of whitespace (except newlines) as one |
| Words | Whitespace-delimited words; each CJK character counts as one word |
| Sentences | Text segments ending in sentence-terminal punctuation (`.`, `?`, `!`, and their fullwidth/international variants), line breaks terminate a potential sentence |
| Paragraphs | Blocks separated by one or more blank lines |
| Read time | Words ÷ 200 words per minute, displayed as `H:MM:SS` |

**Markup is not counted**: before counting, Markdown syntax is stripped —
emphasis/bold/strikethrough delimiters, link and image syntax (only the
human-readable label is counted), list markers, block-quote markers, heading
markers, fenced code block delimiters, table delimiters, math delimiters, and
footnote markers are removed; horizontal rules are removed entirely. Code
block *content* is counted as text; the fence lines are not.

Counting runs off the main thread, coalescing bursts of edits (only the
latest text is counted). Values update on every text change and on every
selection change.

## 2. Display

- The button shows the statistic selected as default (a remembered
  preference; default is words), e.g. "512 Words" or "4:12 Read Time".
- **With an active selection**, the button shows the selection count
  relative to the whole document: "X of Y Words" (selection and total for the
  same statistic; read time shown as two `H:MM:SS` values).
- Clicking the button opens an upward-pointing popover menu listing all five
  statistics; choosing one makes it the displayed default (and persists).
- The popover closes when a choice is made; focus returns to the editor when
  it closes.

## 3. Narrow layout

When the bottom bar is too narrow for both toolbar and full stats label, the
button switches to an abbreviated variant (same menu).
