//! Document files: document state, open routing, save pipeline, recents.
//!
//! One tab holds one document; a window holds one or more tabs
//! (`03-behavior/06-files.md`).

pub mod encoding;
pub mod metadata;
pub mod monitor;
pub mod paths;
pub mod recents;
pub mod snapshots;

use gtk::prelude::*;
use gtk::gio;
use std::rc::Rc;

use crate::app::window_registry;
use crate::window::{AppWindow, TabState};

/// A document's file state.
#[derive(Default)]
pub struct Document {
    /// Backing file; None for a "New File".
    pub file: Option<gio::File>,
    /// Encoding label the file was loaded with (used for saving).
    pub encoding: &'static str,
    pub dirty: bool,
    /// True when the document was restored from a crash-recovery snapshot;
    /// the window then shows the recovery banner until the next save.
    pub recovered: bool,
}

impl Document {
    pub fn new() -> Self {
        Self {
            file: None,
            encoding: "UTF-8",
            dirty: false,
            recovered: false,
        }
    }

    pub fn display_name(&self) -> String {
        self.file
            .as_ref()
            .and_then(|f| f.basename())
            .map(|b| b.display().to_string())
            .unwrap_or_else(|| crate::i18n::i18n("New File"))
    }

    pub fn parent_path(&self) -> Option<String> {
        self.file.as_ref().and_then(|f| {
            f.parent().and_then(|p| p.path()).map(|p| p.display().to_string())
        })
    }

    pub fn path_string(&self) -> Option<String> {
        self.file
            .as_ref()
            .and_then(|f| f.path())
            .map(|p| p.display().to_string())
    }
}

// ---------------------------------------------------------------------------
// Code formats
// ---------------------------------------------------------------------------

/// Extensions (case-insensitive) of the code formats Griffel knows: the
/// requested languages plus mainstream ones and config formats. A code
/// file opens with Code Mode on for its tab; Markdown preview and
/// conversion do not apply to it.
const CODE_EXTENSIONS: &[&str] = &[
    "py", "pyi",                            // Python
    "rs",                                   // Rust
    "c", "h",                               // C
    "cpp", "cc", "cxx", "hpp", "hh", "hxx", // C++
    "js", "mjs", "cjs", "ts", "tsx",        // JavaScript / TypeScript
    "java", "go", "rb", "php", "swift", "kt", "kts", "cs",
    "sh", "bash", "zsh", "lua", "pl", "pm", "sql", "css",
    "json", "yaml", "yml", "toml", "xml", "ini", "mk",
];

/// Basenames (case-insensitive) that are code without a useful extension.
const CODE_BASENAMES: &[&str] = &["dockerfile", "makefile"];

/// Mime types of the code formats, for the open filter and the desktop
/// entry. Only types defined by shared-mime-info are listed; extension
/// patterns cover the rest (Dockerfile, .ini).
const CODE_MIME_TYPES: &[&str] = &[
    "text/x-python",
    "text/rust",
    "text/x-csrc",
    "text/x-chdr",
    "text/x-c++src",
    "text/x-c++hdr",
    "text/javascript",
    "text/typescript",
    "text/x-java",
    "text/x-go",
    "text/x-ruby",
    "text/x-php",
    "text/x-swift",
    "text/x-kotlin",
    "text/x-csharp",
    "text/x-shellscript",
    "text/x-lua",
    "text/x-perl",
    "text/x-sql",
    "text/css",
    "application/json",
    "application/x-yaml",
    "application/toml",
    "text/xml",
    "text/x-makefile",
];

/// Whether `file` is a source-code file (`05-file-formats-and-persistence.md`):
/// opens with Code Mode on, without Markdown preview/conversion.
pub fn is_code_file(file: &gio::File) -> bool {
    file.basename()
        .map(|b| is_code_name(&b.display().to_string()))
        .unwrap_or(false)
}

/// Basename form of `is_code_file`, unit-testable without gio.
fn is_code_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    if CODE_BASENAMES.contains(&lower.as_str()) {
        return true;
    }
    match lower.rsplit_once('.') {
        Some((_, ext)) => CODE_EXTENSIONS.contains(&ext),
        None => false,
    }
}

// ---------------------------------------------------------------------------
// Opening
// ---------------------------------------------------------------------------

/// Application-level open routing (`03-behavior/06-files.md` §2.1):
/// - a file already open in a tab (of any window) is presented, not
///   duplicated;
/// - otherwise it loads into an existing empty unmodified tab if any;
/// - otherwise a new tab opens in the most recent window (or, when no
///   window is open, a new window);
/// - files that do not exist are skipped with a log warning.
pub fn open_files(app: &gtk::Application, files: &[gio::File]) {
    for file in files {
        if let Some((win, tab)) = tab_showing(file) {
            win.present_window();
            win.select_tab(&tab);
            continue;
        }
        if file.query_exists(gio::Cancellable::NONE) {
            if let Some((win, tab)) = empty_unmodified_tab() {
                win.load_file_into(&tab, file);
                win.present_window();
                win.select_tab(&tab);
                continue;
            }
            let win = window_registry()
                .last()
                .cloned()
                .unwrap_or_else(|| AppWindow::new(app));
            win.open_tab();
            let tab = win.active_tab();
            win.load_file_into(&tab, file);
            win.present_window();
        } else {
            glib::g_warning!("griffel", "skipping missing file: {}", file.uri());
        }
    }
}

/// The tab already showing `file`, with its window, if any (G6).
fn tab_showing(file: &gio::File) -> Option<(AppWindow, Rc<TabState>)> {
    let this_uri = file.uri();
    for w in window_registry() {
        for t in w.tabs() {
            let showing = t
                .doc()
                .file
                .as_ref()
                .is_some_and(|f| f.uri() == this_uri);
            if showing {
                return Some((w, t));
            }
        }
    }
    None
}

/// The first tab in any open window that is empty (no backing file),
/// unmodified, and has no recovery banner.
fn empty_unmodified_tab() -> Option<(AppWindow, Rc<TabState>)> {
    for w in window_registry() {
        for t in w.tabs() {
            let free = {
                let doc = t.doc();
                doc.file.is_none()
                    && !doc.dirty
                    && !doc.recovered
                    && t.editor.buffer().char_count() == 0
            };
            if free {
                return Some((w, t));
            }
        }
    }
    None
}

/// Load `file` into `tab` (§2.2). On failure the caller shows an error
/// dialog and resets the tab to a new empty document.
pub fn load_into_tab(
    window: &AppWindow,
    tab: &Rc<TabState>,
    file: &gio::File,
) -> Result<(), String> {
    let bytes = read_file_bytes(file)?;
    let decoded = encoding::decode(&bytes).map_err(|e| {
        crate::i18n::i18n_f(
            "The file “{}” could not be opened as text:\n{}",
            &[
                &file
                    .basename()
                    .map(|b| b.display().to_string())
                    .unwrap_or_else(|| file.uri().to_string()),
                &e,
            ],
        )
    })?;

    window.apply_loaded(tab, file, &decoded);
    Ok(())
}

fn read_file_bytes(file: &gio::File) -> Result<Vec<u8>, String> {
    let info = file
        .query_info(
            "standard::*",
            gio::FileQueryInfoFlags::NONE,
            gio::Cancellable::NONE,
        )
        .map_err(|e| crate::i18n::i18n_f("cannot read file: {}", &[&e.message()]))?;
    if info.file_type() == gio::FileType::Directory {
        return Err(crate::i18n::i18n("cannot open a directory"));
    }
    file.load_bytes(gio::Cancellable::NONE)
        .map(|(bytes, _)| bytes.to_vec())
        .map_err(|e| crate::i18n::i18n_f("cannot read file: {}", &[&e.message()]))
}

/// Open the file chooser and load the chosen file into this window.
pub fn open_dialog(window: &AppWindow) {
    let dialog = gtk::FileDialog::new();
    dialog.set_title(&crate::i18n::i18n("Open File"));

    let md = gtk::FileFilter::new();
    md.set_name(Some(&crate::i18n::i18n("Markdown Files")));
    md.add_mime_type("text/markdown");
    md.add_mime_type("text/x-markdown");
    md.add_pattern("*.md");
    let txt = gtk::FileFilter::new();
    txt.set_name(Some(&crate::i18n::i18n("Plain Text Files")));
    txt.add_mime_type("text/plain");

    let code = gtk::FileFilter::new();
    code.set_name(Some(&crate::i18n::i18n("Source Code")));
    for mime in CODE_MIME_TYPES {
        code.add_mime_type(mime);
    }
    for ext in CODE_EXTENSIONS {
        code.add_pattern(&format!("*.{}", ext));
    }
    for base in CODE_BASENAMES {
        code.add_pattern(base);
    }

    let filters = gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&md);
    filters.append(&code);
    filters.append(&txt);
    dialog.set_filters(Some(&filters));

    let win = window.clone();
    let parent = win.root.clone();
    let parent_for_cb = parent.clone();
    dialog.open(
        Some(&parent),
        gio::Cancellable::NONE,
        move |result| match result {
            Ok(file) => {
                if let Some(app) = parent_for_cb.application() {
                    open_files(&app, &[file]);
                }
            }
            Err(_) => {
                // Cancelled.
            }
        },
    );
}

/// Open the bundled tutorial document in a new window. The tutorial is an
/// internal resource: not added to recents and starts unsaved
/// (`03-behavior/12-windows.md` §3).
pub fn open_tutorial(app: &gtk::Application) {
    let Ok(bytes) = gio::resources_lookup_data(
        "/io/github/hakergeniusz/Griffel/tutorial/tutorial.md",
        gio::ResourceLookupFlags::NONE,
    ) else {
        glib::g_warning!("griffel", "tutorial resource missing");
        return;
    };
    let win = AppWindow::new(app);
    win.load_internal_text(&String::from_utf8_lossy(&bytes));
    win.present_window();
}

// ---------------------------------------------------------------------------
// Saving
// ---------------------------------------------------------------------------

/// Write `text` to `file` with atomic replace, in `encoding` (falling back
/// to UTF-8). Returns the effective encoding label.
pub fn write_to(file: &gio::File, text: &str, encoding: &str) -> Result<String, String> {
    let (bytes, effective) = encoding::encode(text, encoding).map_err(|e| e.to_string())?;
    file.replace_contents(
        &bytes,
        None,
        false,
        gio::FileCreateFlags::REPLACE_DESTINATION,
        gio::Cancellable::NONE,
    )
    .map_err(|e| crate::i18n::i18n_f("cannot save file: {}", &[&e.message()]))?;
    Ok(effective)
}

/// Save As: native dialog, then write and associate the backing file.
/// Operates on `tab`, which need not be the active one (the unsaved-changes
/// confirmation can ask for a Save As of a background tab).
pub fn save_as_dialog(window: &AppWindow, tab: &Rc<TabState>) {
    let doc = tab.doc();
    let initial_name = {
        let mut name = doc.display_name();
        if !name.ends_with(".md") && !name.contains('.') {
            name.push_str(".md");
        }
        name
    };
    let initial_folder = doc
        .parent_path()
        .map(gio::File::for_path)
        .or_else(default_folder);
    drop(doc);

    let dialog = gtk::FileDialog::new();
    dialog.set_title(&crate::i18n::i18n("Save As"));
    dialog.set_initial_name(Some(&initial_name));
    if let Some(folder) = initial_folder {
        dialog.set_initial_folder(Some(&folder));
    }

    let md = gtk::FileFilter::new();
    md.set_name(Some(&crate::i18n::i18n("Markdown")));
    md.add_mime_type("text/markdown");
    md.add_mime_type("text/x-markdown");
    md.add_pattern("*.md");
    let txt = gtk::FileFilter::new();
    txt.set_name(Some(&crate::i18n::i18n("Plain text")));
    txt.add_mime_type("text/plain");
    let filters = gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&md);
    filters.append(&txt);
    dialog.set_filters(Some(&filters));

    let win = window.clone();
    let tab = tab.clone();
    let parent = win.root.clone();
    dialog.save(
        Some(&parent),
        gio::Cancellable::NONE,
        move |result| match result {
            Ok(file) => {
                win.save_tab_to(tab, file);
            }
            Err(_) => {
                // Cancelled.
            }
        },
    );
}

pub(crate) fn default_folder() -> Option<gio::File> {
    gio::File::for_path(glib::home_dir()).into()
}

// ---------------------------------------------------------------------------
// Snapshots
// ---------------------------------------------------------------------------

pub fn recover_snapshots(app: &gtk::Application) {
    snapshots::recover_snapshots(app);
}

pub fn delete_snapshot_tab(window: &AppWindow, tab: &Rc<TabState>) {
    snapshots::delete(window, tab);
}

pub fn schedule_snapshot_tab(window: &AppWindow, tab: &Rc<TabState>) {
    snapshots::schedule(window, tab);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_display_names() {
        let doc = Document::new();
        assert_eq!(doc.display_name(), "New File");
        assert!(doc.path_string().is_none());
        assert!(doc.parent_path().is_none());

        let file = gio::File::for_path("/tmp/notes.md");
        let doc = Document {
            file: Some(file),
            ..Document::default()
        };
        assert_eq!(doc.display_name(), "notes.md");
        assert_eq!(doc.parent_path().as_deref(), Some("/tmp"));
        assert_eq!(doc.path_string().as_deref(), Some("/tmp/notes.md"));
    }

    #[test]
    fn code_name_detection() {
        // The requested languages and their variants, case-insensitively.
        for name in [
            "main.py", "LIB.PYI", "main.rs", "app.c", "foo.H", "main.cpp",
            "main.cc", "main.cxx", "header.hpp", "header.hh", "index.js",
            "module.mjs", "cli.cjs", "index.ts", "component.tsx",
        ] {
            assert!(is_code_name(name), "{}", name);
        }
        // Mainstream and config formats.
        for name in [
            "Main.java", "go.mod.go", "x.go", "app.rb", "index.php",
            "App.swift", "Main.kt", "script.kts", "Program.cs", "run.sh",
            "run.bash", ".zshrc.sh", "init.lua", "script.pl", "Module.pm",
            "query.sql", "style.css", "package.json", "ci.yaml", "ci.yml",
            "Cargo.toml", "config.xml", "settings.ini", "Makefile.mk",
        ] {
            assert!(is_code_name(name), "{}", name);
        }
        // Extensionless basenames.
        assert!(is_code_name("Dockerfile"));
        assert!(is_code_name("Makefile"));
        assert!(is_code_name("makefile"));
        // Not code.
        for name in [
            "notes.md", "README", "archive.tar.gz", "image.png", "file.md.bak",
        ] {
            assert!(!is_code_name(name), "{}", name);
        }
        // Extensionless dotfiles are not code either.
        assert!(!is_code_name(".zshrc"));
        assert!(!is_code_name("data.txt"));
    }
}
