# 03/10 — Preferences

An adaptive modal preferences dialog (`02-ui-layout.md` §11). All settings
take effect immediately and persist (`05-file-formats-and-persistence.md`).

| Setting | Control | Default | Effect |
|---|---|---|---|
| Check spelling | Switch | on | Toggles live spell checking in the editor (`01-editing.md` §4) |
| Auto-hide header bar | Switch | on | While typing, hide the headerbar and bottom bar; reveal on hover/needs (`13-autohide.md`) |
| Large text | Switch | off | Bigger editor font with two extra sizes and wider margins budget (`01-editing.md` §6) |
| Restore session | Switch | (no effect — see `09-open-questions.md`) | Present but non-functional in the reference behavior; the Implementer should either implement a session restore or omit the row (open question) |
| Input format | Dropdown | Pandoc's Markdown | Flavor used by preview and export; changing it reloads previews. Options, each with a help button opening the flavor's official documentation in the browser: Pandoc's Markdown, CommonMark, GitHub-Flavored Markdown, MultiMarkdown, plain original Markdown |
| Preview security | Dropdown | Always ask | Policy applied when previewing files containing raw HTML (`03-live-preview.md` §5): always ask / always use restricted / always use unrestricted |

Notes:

- Changing input format or security policy applies to all open windows
  immediately (previews reload).
- The dropdowns show a description subtitle; the input format dropdown also
  stores the flavor identifier used for converter invocations.
- The dialog uses the platform's adaptive preferences presentation.
