//! Document files: document state, open routing, save pipeline, recents.
//!
//! One window holds one document (`03-behavior/06-files.md`).

pub mod encoding;
pub mod paths;
pub mod recents;

use gtk::prelude::*;
use gtk::gio;

use crate::app::window_registry;
use crate::window::AppWindow;

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
            .unwrap_or_else(|| "New File".into())
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
// Opening
// ---------------------------------------------------------------------------

/// Application-level open routing (`03-behavior/06-files.md` §2.1):
/// - a file already open in a window is presented, not duplicated;
/// - otherwise it loads into an existing empty unmodified window if any;
/// - otherwise a new window is created;
/// - files that do not exist are skipped with a log warning.
pub fn open_files(app: &gtk::Application, files: &[gio::File]) {
    for file in files {
        if let Some(existing) = window_showing(app, file) {
            existing.present_window();
            continue;
        }
        if file.query_exists(gio::Cancellable::NONE) {
            let win = empty_unmodified_window(app).unwrap_or_else(|| AppWindow::new(app));
            win.load_file(file);
            win.present_window();
        } else {
            glib::g_warning!("griffel", "skipping missing file: {}", file.uri());
        }
    }
}

/// The window already showing `file`, if any (G6).
fn window_showing(app: &gtk::Application, file: &gio::File) -> Option<AppWindow> {
    let this_uri = file.uri();
    window_registry()
        .into_iter()
        .find(|w| {
            w.document
                .borrow()
                .file
                .as_ref()
                .is_some_and(|f| f.uri() == this_uri)
        })
}

/// The first window that is empty (no backing file), unmodified, and has no
/// recovery banner.
fn empty_unmodified_window(app: &gtk::Application) -> Option<AppWindow> {
    window_registry().into_iter().find(|w| {
        let doc = w.document.borrow();
        doc.file.is_none() && !doc.dirty && !doc.recovered && w.editor.buffer().char_count() == 0
    })
}

/// Load `file` into the window's editor (§2.2). On failure an error dialog
/// is shown and the window resets to a new empty document.
pub fn load_into(window: &AppWindow, file: &gio::File) -> Result<(), String> {
    let bytes = read_file_bytes(file)?;
    let decoded = encoding::decode(&bytes).map_err(|e| {
        format!(
            "The file “{}” could not be opened as text:\n{}",
            file.basename()
                .map(|b| b.display().to_string())
                .unwrap_or_else(|| file.uri().to_string()),
            e
        )
    })?;

    window.apply_loaded(file, &decoded);
    Ok(())
}

fn read_file_bytes(file: &gio::File) -> Result<Vec<u8>, String> {
    let info = file
        .query_info(
            "standard::*",
            gio::FileQueryInfoFlags::NONE,
            gio::Cancellable::NONE,
        )
        .map_err(|e| format!("cannot read file: {}", e.message()))?;
    if info.file_type() == gio::FileType::Directory {
        return Err("cannot open a directory".into());
    }
    file.load_bytes(gio::Cancellable::NONE)
        .map(|(bytes, _)| bytes.to_vec())
        .map_err(|e| format!("cannot read file: {}", e.message()))
}

/// Open the file chooser and load the chosen file into this window.
pub fn open_dialog(window: &AppWindow) {
    let dialog = gtk::FileDialog::new();
    dialog.set_title("Open File");

    let md = gtk::FileFilter::new();
    md.set_name(Some("Markdown Files"));
    md.add_mime_type("text/markdown");
    md.add_mime_type("text/x-markdown");
    md.add_pattern("*.md");
    let txt = gtk::FileFilter::new();
    txt.set_name(Some("Plain Text Files"));
    txt.add_mime_type("text/plain");

    let filters = gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&md);
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
    .map_err(|e| format!("cannot save file: {}", e.message()))?;
    Ok(effective)
}

/// Save As: native dialog, then write and associate the backing file.
pub fn save_as_dialog(window: &AppWindow) {
    let doc = window.document.borrow();
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
    dialog.set_title("Save As");
    dialog.set_initial_name(Some(&initial_name));
    if let Some(folder) = initial_folder {
        dialog.set_initial_folder(Some(&folder));
    }

    let md = gtk::FileFilter::new();
    md.set_name(Some("Markdown"));
    md.add_mime_type("text/markdown");
    md.add_mime_type("text/x-markdown");
    md.add_pattern("*.md");
    let txt = gtk::FileFilter::new();
    txt.set_name(Some("Plain text"));
    txt.add_mime_type("text/plain");
    let filters = gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&md);
    filters.append(&txt);
    dialog.set_filters(Some(&filters));

    let win = window.clone();
    let parent = win.root.clone();
    dialog.save(
        Some(&parent),
        gio::Cancellable::NONE,
        move |result| match result {
            Ok(file) => {
                win.save_to(file);
            }
            Err(_) => {
                // Cancelled.
            }
        },
    );
}

fn default_folder() -> Option<gio::File> {
    gio::File::for_path(glib::home_dir()).into()
}

// ---------------------------------------------------------------------------
// Snapshots (stub until milestone 11)
// ---------------------------------------------------------------------------

pub fn recover_snapshots(_app: &gtk::Application) {
    // Implemented in milestone 11.
}

pub fn delete_snapshot(_window: &AppWindow) {
    // Implemented in milestone 11.
}

pub fn schedule_snapshot(_window: &AppWindow) {
    // Implemented in milestone 11.
}
