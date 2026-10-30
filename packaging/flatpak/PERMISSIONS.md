# Flatpak sandbox permissions

Each `finish-args` entry in `io.github.hakergeniusz.Griffel.json`, with the
feature that requires it:

| Permission | Why |
|---|---|
| `--share=ipc` | X11 fallback (`MIT-SHM` for the X11 socket below). |
| `--socket=wayland` | Native display protocol. |
| `--socket=fallback-x11` | X11 sessions; Wayland is tried first. |
| `--device=dri` | GPU rendering of the WebKit live preview and peek popovers. |
| `--share=network` | Dictionary peek queries a DICT server (`src/dict.rs`); link and remote-image peeks load web pages (`src/peek.rs`). |
| `--filesystem=home` | Open/save Markdown files anywhere in the home directory, resolve relative images and links next to the document (`src/files/`, `src/export.rs`), and write autosave/snapshot files (`docs/03-behavior/05-file-formats-and-persistence.md`). The GTK `FileDialog` goes through the portal, but the portal only grants access to the single picked file, not to the document's sibling resources. |
| `--filesystem=xdg-data/hunspell:ro` | Read user-installed Hunspell dictionaries for the spell checker (read-only). |

Nothing here grants network access at build time: dependencies are vendored
(`generated-sources.json`, `cargo-vendor-config.toml`) and the build runs
`cargo build --offline`.
