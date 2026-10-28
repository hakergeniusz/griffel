//! File change monitoring (`03-behavior/06-files.md` §4).

use gio::prelude::*;
use std::cell::RefCell;

use crate::window::AppWindow;

/// Install (or re-install) a file monitor on `file`. The monitor shows the
/// "file changed on disk" banner when the backing file changes externally.
pub fn install(window: &AppWindow, file: &gio::File) {
    // Cancel any previous monitor.
    if let Some(m) = window.state.monitor.borrow_mut().take() {
        m.cancel();
    }
    let monitor = match file.monitor_file(gio::FileMonitorFlags::NONE, gio::Cancellable::NONE) {
        Ok(m) => m,
        Err(_) => return,
    };
    let win = window.clone();
    monitor.connect_changed(move |_m, _file, _other, _event_type| {
        win.show_file_changed();
    });
    *window.state.monitor.borrow_mut() = Some(monitor);
}

/// Cancel monitoring (used while saving so our own write doesn't fire).
pub fn pause(window: &AppWindow) {
    if let Some(m) = window.state.monitor.borrow_mut().take() {
        m.cancel();
    }
}
