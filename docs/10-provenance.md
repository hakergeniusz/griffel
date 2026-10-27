# 10 — Provenance

How this specification was produced (clean-room record).

- **Method**: A clean-room reverse-engineering process. Two roles worked in
  separate sessions with no shared context:
  - The **Specifier** (author of this document set) read the reference
    application's source code, UI definitions, and data files, and wrote
    behavioral documentation only.
  - The **Implementer** (a separate session) will receive *only* this
    `docs/` directory and must never read the reference source.
- **Reference studied**: the GNOME "Apostrophe" repository
  (`https://gitlab.gnome.org/World/apostrophe`), shallow-cloned to
  `/tmp/apostrophe-original` for study only; none of its files are included
  in this project.
- **Commit studied**: `84bdcade2483b48097b564f04138d96f1e839f03`
  (2026-10-25), studied on 2026-10-27.
- **License posture**: the reference is GPL-licensed. This spec contains no
  source code, pseudocode, internal identifiers, UI markup, stylesheets,
  artwork, translations, or marketing text from the reference — only
  factual, behavioral descriptions written in original wording. The
  Implementer's output is therefore not a derivative of the reference's
  code, though it may be behaviorally compatible.
- **What was studied** (categories, not contents): application lifecycle and
  window management; text editing input handling; formatting commands;
  in-editor markup highlighting; live preview conversion, layouts, security
  and scroll sync; inline peek popovers; statistics; file open/save/monitor/
  snapshot logic; search and replace; export flows; preferences and
  settings; color schemes; desktop integration files (desktop entry,
  AppStream, settings schema); keyboard shortcut registrations.
- **Known limitations of this spec**: visual design (exact spacing, colors,
  icons, artwork, wording) is intentionally unspecified; ambiguous or
  apparently-buggy behaviors in the reference are recorded as questions in
  `09-open-questions.md` rather than silently copied.
