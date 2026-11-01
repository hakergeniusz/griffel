//! File change monitoring (`03-behavior/06-files.md` §4).

use gio::prelude::*;
use std::rc::Rc;

use crate::window::{AppWindow, TabState};

/// Install (or re-install) a file monitor on `file` for `tab`. The monitor
/// shows the "file changed on disk" banner when the backing file changes
/// externally.
pub fn install_tab(window: &AppWindow, tab: &Rc<TabState>, file: &gio::File) {
    // Cancel any previous monitor.
    if let Some(m) = tab.monitor.borrow_mut().take() {
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
    *tab.monitor.borrow_mut() = Some(monitor);
}

/// Cancel monitoring (used while saving so our own write doesn't fire).
pub fn pause_tab(tab: &Rc<TabState>) {
    if let Some(m) = tab.monitor.borrow_mut().take() {
        m.cancel();
    }
}
