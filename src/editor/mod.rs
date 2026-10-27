//! The editor widget: a SourceView configured for distraction-free
//! Markdown editing inside a scrolled, centered column.

pub mod focus;
pub mod highlight;
pub mod input;
pub use input::InputFlags;

use gtk::prelude::*;
use sourceview5::prelude::*;

#[derive(Clone)]
pub struct Editor {
    pub view: sourceview5::View,
    pub buffer: sourceview5::Buffer,
    pub scroll: gtk::ScrolledWindow,
    /// Shared typing-behavior state (Hemingway mode, document folder).
    pub flags: std::rc::Rc<input::InputFlags>,
    /// Invoked when a user deletion is blocked (Hemingway Mode).
    pub blocked_callback: std::rc::Rc<input::BlockedCallback>,
}

impl Editor {
    pub fn new() -> Self {
        let buffer = sourceview5::Buffer::new(None::<&gtk::TextTagTable>);
        buffer.set_implicit_trailing_newline(false);

        let view = sourceview5::View::with_buffer(&buffer);
        view.set_editable(true);
        view.set_cursor_visible(true);
        view.set_wrap_mode(gtk::WrapMode::None);
        view.set_property("tab-width", 4u32);
        view.set_property("insert-spaces-instead-of-tabs", false);
        view.set_property("auto-indent", false);
        view.set_property("show-line-numbers", false);
        view.set_property("highlight-current-line", false);
        view.set_css_classes(&["editor-view"]);
        view.set_monospace(true);
        view.set_top_margin(34);
        view.set_bottom_margin(64);

        let scroll = gtk::ScrolledWindow::new();
        scroll.set_child(Some(&view));
        scroll.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
        scroll.set_hexpand(true);
        scroll.set_vexpand(true);

        let flags = std::rc::Rc::new(input::InputFlags::default());
        let blocked_callback = std::rc::Rc::new(input::BlockedCallback::new(None));
        input::install(&view, &buffer, flags.clone(), blocked_callback.clone());
        input::install_dnd(&view, &buffer, flags.clone());

        Self {
            view,
            buffer,
            scroll,
            flags,
            blocked_callback,
        }
    }

    /// Current vertical scroll position as a 0–1 fraction, or 0 when
    /// there is nothing to scroll.
    pub fn scroll_fraction(&self) -> f64 {
        let adj = self.scroll.vadjustment();
        let upper = adj.upper() - adj.page_size();
        if upper <= 0.0 {
            0.0
        } else {
            (adj.value() / upper).clamp(0.0, 1.0)
        }
    }

    pub fn set_scroll_fraction(&self, f: f64) {
        let adj = self.scroll.vadjustment();
        let upper = adj.upper() - adj.page_size();
        if upper > 0.0 {
            adj.set_value((f.clamp(0.0, 1.0)) * upper);
        }
    }

    pub fn buffer(&self) -> &sourceview5::Buffer {
        &self.buffer
    }

    pub fn full_text(&self) -> String {
        self.buffer
            .text(&self.buffer.start_iter(), &self.buffer.end_iter(), false)
            .into()
    }

    pub fn set_full_text(&self, text: &str) {
        self.buffer.begin_irreversible_action();
        self.buffer.set_text(text);
        self.buffer.end_irreversible_action();
    }
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}
