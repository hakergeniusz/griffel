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
    font_provider: gtk::CssProvider,
    highlighter: highlight::Highlighter,
    /// Font size chosen by the last relayout (px).
    font_px: std::cell::Cell<i32>,
    /// Character width of the current font (px).
    char_px: std::cell::Cell<f64>,
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

        // Highlighting tags and their palette.
        highlight::ensure_tags(&buffer);
        highlight::update_palette(&buffer, &crate::appearance::effective_palette());
        let highlighter = highlight::Highlighter::new();

        // Keep the cursor just visible while typing/navigating (§8): a
        // small safety margin; mouse scrolling is not fought because this
        // only fires when the cursor mark moves.
        {
            let view = view.clone();
            buffer.connect_mark_set(move |_buf, location, mark| {
                let is_insert = mark
                    .name()
                    .map(|n| n == "insert")
                    .unwrap_or(false);
                if is_insert {
                    let mut loc = location.clone();
                    let _ = view.scroll_to_iter(&mut loc, 0.04, false, 0.0, 0.0);
                }
            });
        }

        let font_provider = gtk::CssProvider::new();
        view.style_context()
            .add_provider(&font_provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 3);
        editor_widget_css(&font_provider, &crate::appearance::effective_palette());

        let editor = Self {
            view,
            buffer,
            scroll,
            flags,
            blocked_callback,
            font_provider,
            highlighter,
            font_px: std::cell::Cell::new(16),
            char_px: std::cell::Cell::new(9.6),
        };
        editor.highlight_now();
        editor
    }

    /// Run a (debounced by the caller) highlighting scan.
    pub fn highlight_now(&self) {
        let text = self.full_text();
        self.highlighter.schedule(&self.buffer, text);
    }

    /// Re-apply tag colors and widget colors after a scheme change.
    pub fn recolor(&self) {
        highlight::update_palette(&self.buffer, &crate::appearance::effective_palette());
        editor_widget_css(&self.font_provider, &crate::appearance::effective_palette());
    }

    /// Adaptive typography (`01-editing.md` §6): choose the largest font
    /// step whose column (`chars_per_line` characters) fits `width` px
    /// with a small padding, then center the column with equal margins.
    /// Returns the chosen font size in px.
    pub fn relayout_column(&self, width: i32, chars_per_line: i32, bigger_text: bool) -> i32 {
        let mut sizes: Vec<i32> = if bigger_text {
            vec![26, 24, 22, 20, 18, 16, 15, 14, 13, 12]
        } else {
            vec![20, 18, 16, 15, 14, 13, 12]
        };
        let padding = 48.0f64;
        let mut chosen = *sizes.last().unwrap();
        for &size in &sizes {
            let char_w = self.measure_char_width(size);
            let column = chars_per_line as f64 * char_w;
            if column + padding <= width as f64 {
                chosen = size;
                break;
            }
        }
        sizes.clear();

        self.apply_font_size(chosen);
        let char_w = self.measure_char_width(chosen);
        let column = (chars_per_line as f64 * char_w).ceil() as i32;
        let margin = ((width - column) / 2).max(0);
        self.view.set_left_margin(margin);
        self.view.set_right_margin(margin);
        highlight::update_indents(&self.buffer, margin, char_w.round() as i32);
        self.font_px.set(chosen);
        self.char_px.set(char_w);
        chosen
    }

    fn apply_font_size(&self, px: i32) {
        // Rebuild the whole widget CSS so colors persist across size steps.
        editor_widget_css(&self.font_provider, &crate::appearance::effective_palette());
        self.font_provider
            .load_from_string(&format!(
                ".editor-view {{ font-family: 'Monospace'; font-size: {px}px; }}"
            ));
    }

    fn measure_char_width(&self, px: i32) -> f64 {
        let mut desc = pango::FontDescription::from_string("Monospace");
        desc.set_absolute_size(px as f64 * f64::from(pango::SCALE));
        let context = self.view.pango_context();
        let metrics = context.metrics(Some(&desc), None::<&pango::Language>);
        if metrics.approximate_char_width() > 0 {
            metrics.approximate_char_width() as f64 / f64::from(pango::SCALE)
        } else {
            px as f64 * 0.6
        }
    }

    /// Current font size (px) chosen by the last relayout.
    pub fn font_px(&self) -> i32 {
        self.font_px.get()
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

/// Widget-level CSS for the editor surface: paper background, text color,
/// selection, and monospace family. Widget-level so it reliably overrides
/// the theme regardless of the platform style.
fn editor_widget_css(provider: &gtk::CssProvider, p: &crate::appearance::EditorPalette) {
    provider.load_from_string(&format!(
        ".editor-view {{ background-color: {bg}; color: {fg}; }}\n\
         .editor-view selection {{ background-color: {sel}; }}",
        bg = p.background,
        fg = p.foreground,
        sel = p.selection,
    ));
}
