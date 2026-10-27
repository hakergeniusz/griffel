# 09 — Open questions

Decisions left to the human. Each notes the spec locations affected.

1. **Advertised-but-unregistered shortcuts.** The shortcuts window in the
   reference advertises Markdown shortcuts (Ctrl+R, Ctrl+1–4, Ctrl+U, Ctrl+I,
   Ctrl+B, Ctrl+Shift+D) and F7 (spellcheck), but the reference does not
   appear to register all of them as active accelerators. Should the new app
   (a) register all advertised shortcuts, (b) only list working ones in its
   shortcuts window? Recommendation: (a) — implement all of `04-keyboard-shortcuts.md`.
2. **"Restore session" preference row.** The preferences UI contains a
   Restore Session switch that has no backing setting and no effect in the
   reference. Implement a real session restore (reopen previously open
   files), or drop the row? Recommendation: drop it for v1; note it as a
   future feature.
3. **Exit behavior with zero windows.** When the last window is closed via
   its close button (not Ctrl+Q), should the process exit? Pick and encode in
   `06-edge-cases.md` §5.
4. **Preview layout default.** The stored default is "full-width" (preview
   replaces the editor). Confirm this is the intended first-run experience
   rather than side-by-side; `02-ui-layout.md` §4 mirrors the reference.
5. **reveal.js without a bundled library.** Should reveal.js export be
   hidden when the library is unavailable, or export anyway (current spec)?
   See `08-export.md` §2.2 and `06-edge-cases.md` §4.
6. **Dictionary service.** The peek dictionary uses a public DICT server
   (WordNet). Confirm privacy acceptability and whether the server/host
   should be configurable (`11-inline-previews.md`).
7. **Formula rendering without LaTeX.** Reference behavior: LaTeX toolchain
   required; without it, an error message. Consider a pure-Rust math
   renderer as an alternative; spec currently requires only the
   error-message behavior.
8. **Line endings on save.** The reference preserves whatever newline
   characters exist in the buffer (no CRLF round-tripping). Confirm the new
   app should do the same rather than normalizing to the file's dominant
   ending (`05-file-formats-and-persistence.md` §1).
9. **Per-file spellcheck language on remote files.** File metadata is only
   writable for native files; behavior for remote (e.g. smb) documents is
   unspecified — propose: skip metadata silently.
10. **Table "fix" affordance.** The reference contains a table re-alignment
    routine that is not visibly wired to any UI element. The spec includes
    fix-table behavior (`02-formatting.md` §7) pending a decision on how it
    is exposed (context menu item? automatic on insert?).
11. **Sepia scheme scope.** Sepia restyles chrome via an override stylesheet
    in the reference. The new implementation may treat sepia as a full
    theme; visual design is the Implementer's (`09-appearance.md`).
12. **Naming.** New app name, ID, icon, and whether it keeps compatibility
    with the reference's file associations is open (`07-integration.md` §1).
13. **Ctrl+1…4 semantics.** Advertised as heading shortcuts; the heading
    command's parameter supports levels 1–6. If registered, do Ctrl+5/6
    exist too? Recommendation: register Ctrl+1–6.
14. **Windowed preview geometry.** The separate preview window's size/position
    is not persisted in the reference. Keep (spec currently silent) or
    persist? Default: keep unpersisted.
15. **Seasonal progress-bar accent.** The reference tints the save progress
    bar with awareness-period color palettes on certain dates. Replicate,
    generalize, or omit? (`06-files.md` §3)
16. **Spellcheck action.** F7 is advertised for toggling spell checking but
    may not be wired in the reference; spec lists it in
    `04-keyboard-shortcuts.md` — implement and register it.
