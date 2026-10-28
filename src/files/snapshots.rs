//! Autosave snapshots and crash recovery (`03-behavior/06-files.md` §5).

use crate::app;
use crate::files;
use crate::files::monitor;
use crate::settings;
use crate::window::AppWindow;
use gtk::gio;
use gtk::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};

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

pub fn schedule(window: &AppWindow) {
    if window.state.snapshot_timer.borrow().is_some() {
        return;
    }
    let period = settings::settings().integer("autosave-period").max(1) as u32;
    let win = window.clone();
    let id = glib::timeout_add_seconds_local(period, move || {
        let dirty = win.state.document.borrow().dirty;
        if !dirty {
            *win.state.snapshot_timer.borrow_mut() = None;
            return glib::ControlFlow::Break;
        }
        let text = win.editor.full_text();
        if text.trim().is_empty() {
            return glib::ControlFlow::Continue;
        }
        // Reuse an existing snapshot file for this document if possible.
        let path = if let Some(p) = win.state.snapshot_file.borrow().clone() {
            p
        } else {
            let p = snapshot_dir().join(glib::uuid_string_random().to_string());
            *win.state.snapshot_file.borrow_mut() = Some(p.clone());
            p
        };
        if fs::write(&path, text.as_bytes()).is_ok() {
            let orig = win
                .state
                .document
                .borrow()
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
    *window.state.snapshot_timer.borrow_mut() = Some(id);
}

pub fn delete(window: &AppWindow) {
    window.cancel_snapshot_timer();
    if let Some(path) = window.state.snapshot_file.borrow_mut().take() {
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(companion(&path));
    }
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

        let win = app::window_registry().into_iter().find(|w| {
            let doc = w.state.document.borrow();
            doc.file.is_none() && !doc.dirty && !doc.recovered && w.editor.buffer().char_count() == 0
        }).unwrap_or_else(|| AppWindow::new(app));

        *win.state.suppress_dirty.borrow_mut() = true;
        win.editor.set_full_text(&text);
        *win.state.suppress_dirty.borrow_mut() = false;
        {
            let mut doc = win.state.document.borrow_mut();
            doc.file = file.clone();
            doc.encoding = "UTF-8";
            doc.dirty = true;
            doc.recovered = true;
        }
        win.update_title();
        win.editor.highlight_now();
        win.show_recovery_banner();
        if let Some(f) = file {
            monitor::install(&win, &f);
        }
        win.present_window();
    }
}
