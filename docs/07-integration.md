# 07 — Desktop integration

Platform-level integration requirements. Where the reference app used a
specific identifier, this spec deliberately does **not** reuse it: the new
project must choose its **own** application name, app ID, icon, and branding
(the Implementer/human decides; suggestions below). Only behavioral
integration matters for parity.

## 1. Application identity

- A unique reverse-DNS application ID (e.g. `org.<project>.<AppName>`; the
  new project must pick its own, not the reference app's ID).
- Its own name, icon (regular + symbolic + development variant), and about
  dialog content.
- A development profile variant (different ID suffix, e.g. `.Devel`) that
  enables verbose logging, shows a visual "development" indication, and
  enables web-inspection of the preview — optional but recommended.

## 2. Desktop entry

A freedesktop `.desktop` file providing:

- Name, generic description ("distraction-free Markdown editor" wording is
  the project's own), and search keywords (including "markdown", "editor").
- Categories: GNOME;GTK;Office (or equivalent).
- `Exec` accepting file URIs/paths (`%U`), single-instance with DBus
  activation (`DBusActivatable=true`).
- MimeType associations: `text/markdown`, `text/x-markdown`,
  `text/plain` — the app appears as a handler for Markdown files.
- A desktop action "New Window" launching a second window of the running
  instance.

## 3. AppStream metadata

A metainfo file (AppStream spec) with: name, summary, description, developer
name, license, screenshots, release notes, provides (ID, binary), content
rating, translation status, and links (homepage, issue tracker, donate,
translate). Required for software-center visibility.

## 4. Single instance and file activation

- Second launches of the app (including `app . file.md` from a file manager)
  must forward the files to the running instance instead of starting a second
  process (`06-files.md` §2.1).
- Files arrive as URIs; remote URIs (e.g. `http://`) should be readable via
  the platform's GIO-equivalent layer; behavior for unreadable URIs is the
  standard error path.

## 5. Command line

| Option | Effect |
|---|---|
| (files…) | Open the given files in the running instance |
| `--new-window` / `-w` | Open a new empty window (forwards to the running instance if one exists) |
| `--verbose` / `-v` | Enable verbose console logging |

## 6. Session and system integration

- **Logout inhibition**: while a document has unsaved changes, the app
  inhibits session logout (with a reason string) so the session manager can
  warn; the inhibition is released when all documents are clean.
- **Recents**: register opened/saved files with the platform's recently-used
  store (`05-file-formats-and-persistence.md` §3).
- **Per-file metadata**: store the spellcheck language as file metadata via
  the platform's gvfs metadata facility (or equivalent).
- **State directory**: snapshots live under the XDG state dir
  (`05-file-formats-and-persistence.md` §5).
- **Settings**: freedesktop-style settings storage, schema-validated
  (`05-file-formats-and-persistence.md` §2).

## 7. External dependencies

The app requires at runtime:

- A Pandoc-compatible document converter (any version supporting the used
  options); feature-detect and degrade gracefully when absent
  (`06-edge-cases.md` §3/§4).
- Optionally a LaTeX toolchain (for formula peek images and LaTeX-based
  export) — feature-detected.
- Optionally a reveal.js distribution for reveal.js exports (bundled or
  system-provided; the app copies it next to the exported HTML).

## 8. Packaging expectations

- Build must work for both sandboxed (Flatpak-like) and conventional
  (distro package) installs; the app may adapt messages (e.g. the TeX
  install hint) to the sandboxed vs. distro case.
- All bundled resources (tutorial, stylesheets, converter filter, export
  templates/reference docs, icons) must be locatable at runtime regardless
  of install prefix.
- Translations: user-facing strings should be translatable (gettext-style)
  even if the new project starts with one language.
