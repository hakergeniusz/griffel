//! Autosave snapshots and crash recovery (`03-behavior/06-files.md` §5).

use crate::app;
use crate::files::monitor;
use crate::settings;
use crate::window::{AppWindow, TabState};
use gtk::gio;
use gtk::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

fn snapshot_dir() -> PathBuf {
    let base = std::env::var("XDG_STATE_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| glib::home_dir().join(".local").join("state"));
    let dir = base.join("griffel").join("snapshots");
    if let Ok(()) = fs::create_dir_all(&dir) {}
    // Best-effort mode 0750 on the directory.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o750));
    }
    dir
}

fn companion(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|n| format!("{}.orig", n.to_string_lossy()))
        .unwrap_or_else(|| "unknown.orig".into());
    path.with_file_name(name)
}

fn is_snapshot_file(path: &Path) -> bool {
    path.is_file() && !path.to_string_lossy().ends_with(".orig")
}

/// A periodic snapshot is due only after at least one edit since the last
/// successful write. Without this check a dirty document is rewritten in
/// full every period, which for a large file means megabytes per second of
/// pure SSD wear while the user is idle.
fn snapshot_due(edits: u64, last_written: u64) -> bool {
    edits != last_written
}

pub fn schedule(window: &AppWindow, tab: &Rc<TabState>) {
    if tab.snapshot_timer.borrow().is_some() {
        return;
    }
    let period = settings::settings().integer("autosave-period").max(1) as u32;
    let tab = tab.clone();
    let timer_tab = tab.clone();
    let id = glib::timeout_add_seconds_local(period, move || {
        let dirty = timer_tab.doc().dirty;
        if !dirty {
            *timer_tab.snapshot_timer.borrow_mut() = None;
            return glib::ControlFlow::Break;
        }
        // The timer keeps ticking while the document is dirty, but an idle
        // document must not be rewritten every period: for a multi-megabyte
        // file that is several MB/s of needless SSD wear. Only snapshot when
        // an edit actually happened since the last successful write.
        let edits = timer_tab.editor.edit_count();
        if !snapshot_due(edits, timer_tab.last_snapshot_edit.get()) {
            return glib::ControlFlow::Continue;
        }
        let text = timer_tab.editor.full_text();
        if text.trim().is_empty() {
            return glib::ControlFlow::Continue;
        }
        // Reuse an existing snapshot file for this document if possible.
        let path = if let Some(p) = timer_tab.snapshot_file.borrow().clone() {
            p
        } else {
            let p = snapshot_dir().join(glib::uuid_string_random().to_string());
            *timer_tab.snapshot_file.borrow_mut() = Some(p.clone());
            p
        };
        // Write off the main thread: a multi-megabyte snapshot must not
        // block typing (01-editing.md §9). Best-effort, as before.
        let orig = timer_tab
            .doc()
            .file
            .clone()
            .and_then(|f| f.path().map(|p| p.display().to_string()));
        let bytes = text.into_bytes();
        let snap_tab = timer_tab.clone();
        glib::spawn_future_local(async move {
            let wrote = gio::spawn_blocking(move || {
                if fs::write(&path, &bytes).is_err() {
                    return false;
                }
                let comp = companion(&path);
                match orig {
                    Some(s) => {
                        let _ = fs::write(&comp, s);
                    }
                    None => {
                        let _ = fs::remove_file(&comp);
                    }
                }
                true
            })
            .await;
            // Only mark the state as current after a successful write; a
            // failed write is retried on the next tick.
            if wrote.unwrap_or(false) {
                snap_tab.last_snapshot_edit.set(edits);
            }
        });
        glib::ControlFlow::Continue
    });
    *tab.snapshot_timer.borrow_mut() = Some(id);
}

pub fn delete(window: &AppWindow, tab: &Rc<TabState>) {
    window.cancel_snapshot_timer(tab);
    // A future snapshot for this tab must be written even if its edit count
    // happens to match the one we last wrote.
    tab.last_snapshot_edit.set(0);
    if let Some(path) = tab.snapshot_file.borrow_mut().take() {
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(companion(&path));
    }
}

/// The first tab in any open window that is empty and clean (no backing
/// file, no edits, no recovery banner yet) — recovery reuses it instead of
/// stacking up tabs.
fn empty_unmodified_tab() -> Option<(AppWindow, Rc<TabState>)> {
    for w in app::window_registry() {
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

/// One autosave snapshot waiting to be reviewed.
#[derive(Debug)]
pub struct Recovery {
    /// The snapshot file itself.
    pub path: PathBuf,
    /// Original document path, when the snapshot has a companion file.
    pub original: Option<PathBuf>,
    /// First non-empty line of the snapshot, truncated for display.
    pub preview: String,
}

fn snapshot_paths() -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(snapshot_dir()) else {
        return Vec::new();
    };
    let mut snaps: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| is_snapshot_file(p))
        .collect();
    // Oldest first, for predictable ordering.
    snaps.sort_by_key(|p| p.metadata().and_then(|m| m.modified()).ok());
    snaps
}

/// Snapshots still on disk, oldest first, for the Preferences list.
pub fn list_recoveries() -> Vec<Recovery> {
    snapshot_paths()
        .into_iter()
        .filter_map(|path| {
            let text = fs::read_to_string(&path).ok()?;
            if text.trim().is_empty() {
                return None;
            }
            let original = fs::read_to_string(companion(&path))
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .map(PathBuf::from);
            let preview: String = text
                .lines()
                .map(str::trim)
                .find(|line| !line.is_empty())
                .unwrap_or("")
                .chars()
                .take(80)
                .collect();
            Some(Recovery {
                path,
                original,
                preview,
            })
        })
        .collect()
}

/// Permanently remove a snapshot and its companion file.
pub fn discard(path: &Path) {
    let _ = fs::remove_file(path);
    let _ = fs::remove_file(companion(path));
}

pub fn recover_snapshots(app: &gtk::Application) {
    // With this preference on, startup does not pop recovered documents
    // into new windows: they are listed in Preferences for the user to
    // open or discard explicitly.
    if settings::settings().boolean("recovery-in-preferences") {
        return;
    }
    for snap in snapshot_paths() {
        recover_one(app, &snap);
    }
}

/// Load one snapshot into a free tab (or a new window), with the recovery
/// banner, so the user can keep or discard it. Used both by startup
/// recovery and by the Preferences "Open" action.
pub fn recover_one(app: &gtk::Application, snap: &Path) -> bool {
    let text = match fs::read_to_string(snap) {
        Ok(t) if !t.trim().is_empty() => t,
        _ => return false,
    };
    let original = fs::read_to_string(companion(snap))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let file = original.map(|p| gio::File::for_path(&p));

    // Recover into an existing free tab when there is one; otherwise open
    // a new window for this snapshot.
    let (win, tab) = empty_unmodified_tab().unwrap_or_else(|| {
        let win = AppWindow::new(app);
        let tab = win.active_tab();
        (win, tab)
    });

    *tab.suppress_dirty.borrow_mut() = true;
    tab.editor.set_full_text(&text);
    *tab.suppress_dirty.borrow_mut() = false;
    {
        let mut doc = tab.doc_mut();
        doc.file = file.clone();
        doc.encoding = "UTF-8";
        doc.dirty = true;
        doc.recovered = true;
    }
    // Tie the tab to its snapshot so saving or closing it removes the
    // snapshot — otherwise it would reappear at every startup.
    *tab.snapshot_file.borrow_mut() = Some(snap.to_owned());
    win.update_title();
    win.update_tab_title(&tab);
    tab.editor.highlight_now();
    // Recovered text replaces the buffer programmatically; refresh the
    // statistics button for it.
    win.schedule_stats();
    win.show_recovery_banner();
    if let Some(f) = file {
        monitor::install_tab(&win, &tab, &f);
    }
    win.present_window();
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // `XDG_STATE_HOME` is process-global; serialize tests that change it.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn snapshot_is_rewritten_only_after_an_edit() {
        assert!(snapshot_due(1, 0)); // first edit after a load
        assert!(!snapshot_due(1, 1)); // idle ticks must not rewrite
        assert!(!snapshot_due(1, 1));
        assert!(snapshot_due(2, 1)); // next edit makes it due again
    }

    #[test]
    fn lists_and_discards_recoveries() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("griffel-snap-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        unsafe { std::env::set_var("XDG_STATE_HOME", &dir) };

        let snaps = snapshot_dir();
        fs::write(snaps.join("a"), "  \nFirst line\nsecond line\n").unwrap();
        fs::write(snaps.join("a.orig"), "/tmp/doc.md\n").unwrap();
        // An orphan companion is not a recovery.
        fs::write(snaps.join("b.orig"), "/tmp/other.md\n").unwrap();

        let list = list_recoveries();
        assert_eq!(list.len(), 1, "{list:?}");
        assert_eq!(list[0].preview, "First line");
        assert_eq!(
            list[0].original.as_deref(),
            Some(Path::new("/tmp/doc.md"))
        );

        discard(&list[0].path);
        assert!(list_recoveries().is_empty());

        unsafe { std::env::remove_var("XDG_STATE_HOME") };
        let _ = fs::remove_dir_all(&dir);
    }
}
