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
        if fs::write(&path, text.as_bytes()).is_ok() {
            let orig = timer_tab
                .doc()
                .file
                .clone()
                .and_then(|f| f.path().map(|p| p.display().to_string()));
            let comp = companion(&path);
            match orig {
                Some(s) => {
                    let _ = fs::write(&comp, s);
                }
                None => {
                    let _ = fs::remove_file(&comp);
                }
            }
        }
        glib::ControlFlow::Continue
    });
    *tab.snapshot_timer.borrow_mut() = Some(id);
}

pub fn delete(window: &AppWindow, tab: &Rc<TabState>) {
    window.cancel_snapshot_timer(tab);
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

pub fn recover_snapshots(app: &gtk::Application) {
    let dir = snapshot_dir();
    let Ok(entries) = fs::read_dir(&dir) else {
        return;
    };
    let mut snaps: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| is_snapshot_file(p))
        .collect();
    // Oldest first, for predictable ordering.
    snaps.sort_by_key(|p| p.metadata().and_then(|m| m.modified()).ok());

    for snap in snaps {
        let text = match fs::read_to_string(&snap) {
            Ok(t) if !t.trim().is_empty() => t,
            _ => continue,
        };
        let original = fs::read_to_string(companion(&snap))
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let file = original.map(|p| gio::File::for_path(&p));

        // Recover into an existing free tab when there is one; further
        // snapshots then open as tabs of that window.
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
    }
}
