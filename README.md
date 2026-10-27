# Griffel

A distraction-free Markdown editor for the Linux desktop.

Griffel shows your document as a centered column of plain Markdown with a
live HTML preview, in-place markup highlighting, Focus and Hemingway writing
modes, and one-click export to PDF, HTML and ODT through an external Pandoc
converter.

## Features

- Centered, monospace writing column with adaptive font size
- Subtle in-place Markdown highlighting (bold, italics, code, links,
  headings, quotes, rules) without leaving the source
- Live preview beside, above, in place of, or in a separate window
- Focus Mode (current sentence lit) and Hemingway Mode (no deletions)
- Live statistics: characters, words, sentences, paragraphs, reading time
- Spell checking with per-file language memory
- Automatic crash-recovery snapshots
- Search & replace including regular expressions
- Export via Pandoc to PDF, HTML, ODT and many more formats
- Light, sepia, dark, or system appearance

## Requirements

- GTK 4 (≥ 4.10), libadwaita (≥ 1.4), GtkSourceView 5 (≥ 5.4, for spell
  checking), WebKitGTK 6.0
- A `pandoc` executable on `PATH` for preview and export (the app degrades
  gracefully when it is missing)
- Optional: `xelatex` (LaTeX exports), `typst` (Typst PDF export), a LaTeX
  distribution for formula peek rendering

## Building

```sh
cargo build --release
```

Run from the build tree:

```sh
cargo run -- [files…]
```

## License

MIT — see `LICENSE`.
