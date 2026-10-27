# 03/08 — Export

All exports convert the document with the external converter
(`00-overview.md`), using the input format flavor preference. Exported
output goes where the user chooses via a native file dialog; nothing is
written without that confirmation. Conversion runs synchronously per export
with the UI showing the platform's busy state; failures surface an error
dialog (`06-edge-cases.md`).

## 1. Quick export (export menu)

Three one-click exports, each opening a native save dialog pre-named
`<document name>.<ext>` and defaulting to the document's folder:

| Target | Format | Notes |
|---|---|---|
| PDF | PDF via the Typst engine | A4 paper size, app's bundled Typst template |
| HTML | HTML5 | standalone document with the app's preview stylesheet for the current color scheme embedded, MathJax math, resources embedded |
| ODT | OpenDocument Text | A4 paper size via a bundled reference document |

Relative image/link paths are rewritten to be relative to the document's
folder (same mechanism as preview).

## 2. Advanced export dialog

A modal dialog (`02-ui-layout.md` §11) with a scrollable list of output
formats on the left and the options pane on the right; choosing a format
reveals its options and updates the dialog title to "Export to <format>".
The Export button then opens the native save (or folder) dialog and performs
the conversion.

Format list (name — file extension):

1. PDF (Typst engine)
2. PDF (LaTeX/XeLaTeX engine)
3. LibreOffice Text Document (odt)
4. Microsoft Word (docx)
5. EPUB v3 (epub)
6. HTML5 Slideshow (reveal.js) (html)
7. LaTeX Beamer Slideshow, PDF (pdf via XeLaTeX)
8. LaTeX Beamer Slideshow, TeX source (tex)
9. HTML5 Slideshow (DZSlides) (html)
10. LaTeX (tex)
11. HTML (html5)
12. MediaWiki Markup (txt)
13. DokuWiki Markup (txt)
14. ConTeXt (tex)
15. Textile (txt)
16. reStructuredText (txt)
17. Texinfo (texi)
18. Rich Text Format (rtf)
19. Groff Man (man)

### 2.1 Options and when they appear

| Option | Applies to | Effect |
|---|---|---|
| Standalone | all non-slideshow text formats | include a header/footer (styles, metadata) |
| Relative→absolute paths | all | rewrite relative image/link paths against the document's folder |
| Table of contents | most text formats | generate a TOC |
| Number sections | most text formats | number top-level sections |
| Page size (A4 / Letter) | paged formats (PDF, LaTeX, ConTeXt, ODT, docx) | A4 uses a paper-size variable or a bundled A4 reference document for ODT/docx |
| Slide size (4:3 / 16:9) | Beamer | 16:9 sets the aspect ratio variable |
| Self-contained | HTML | embed all resources, standalone |
| Syntax highlighting (with style picker) | html, tex, docx, pdf (non-Typst) | enable and choose a highlight style from: pygments, kate, monochrome, espresso, zenburn, haddock, tango |
| Incremental bullets | presentations (reveal.js, DZSlides, Beamer) | reveal bullet points one at a time |

The options pane shows only relevant groups. Syntax highlighting defaults to
a collapsed expander.

### 2.2 Special behaviors

- **Missing TeX warning**: when the selected format requires a LaTeX
  toolchain (LaTeX PDF or Beamer) and no TeX installation is detected, the
  options pane is replaced by a warning page explaining what is missing and
  how to install it — with a variant for packaged vs sandboxed installs and
  a copy button that copies the install command to the clipboard.
- **reveal.js slideshow**: exports a folder (folder chooser instead of file
  chooser) containing the HTML file plus a bundled copy of the reveal.js
  library; the HTML references it via a relative path variable.

## 3. Copy HTML

Converts the current document to HTML (same flavor preference; no
standalone wrapper) and places it on the clipboard. No dialog.
