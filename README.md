# Griffel

[![CI](https://github.com/hakergeniusz/griffel/actions/workflows/ci.yml/badge.svg)](https://github.com/hakergeniusz/griffel/actions/workflows/ci.yml)

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
- Automatic crash-recovery snapshots and autosave
- Search & replace including regular expressions
- Export via Pandoc to PDF, HTML, ODT and many more formats
- Light, sepia, dark, or system appearance

## Installation

Prebuilt packages are attached to each
[GitHub release](https://github.com/hakergeniusz/griffel/releases):

- **Debian / Ubuntu** — `griffel_*.deb` (install with
  `sudo apt install ./griffel_*.deb`)
- **Fedora / RHEL** — `griffel-*.rpm` (install with
  `sudo dnf install ./griffel-*.rpm`)
- **Arch / Manjaro** — build from `packaging/arch/PKGBUILD` with `makepkg -si`
  (or install a `griffel` package from the AUR once published)
- **Any distro** — `griffel-*-x86_64.flatpak` (install with
  `flatpak install ./griffel-*.flatpak`), or the standalone
  `griffel-*-linux-x86_64.tar.xz` tarball

Live preview and export need a `pandoc` executable on `PATH`; the app
degrades gracefully without it. Pandoc is intentionally not bundled
(it is GPL-licensed and must remain an external process).

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

## Packaging & releasing

Distribution packaging lives in `debian/`, `packaging/fedora/`,
`packaging/arch/` and `packaging/flatpak/`. CI (`.github/workflows/`)
builds and tests every push and produces the deb, RPM, Flatpak bundle and
tarball on `v*` tags.

To cut a release, bump the version in `Cargo.toml` together with
`data/io.github.hakergeniusz.Griffel.metainfo.xml`, `debian/changelog`,
`packaging/fedora/griffel.spec` and `packaging/arch/PKGBUILD`, re-run
`packaging/flatpak/generate-sources.py` if dependencies changed, then tag
`v<version>` and push the tag.

## License

MIT — see `LICENSE`.
