//! Typing behavior wired to the editor: auto-pairing, list continuation,
//! Tab/Shift+Tab indentation, and drag-and-drop
//! (`03-behavior/01-editing.md` §2, §3).

use gtk::prelude::*;
use gtk::glib;
use gtk::gdk;
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use crate::markdown::{autopair, lists};

/// Mutable behavior state shared between the editor and the window.
#[derive(Default)]
pub struct InputFlags {
    /// Hemingway Mode: user deletions are blocked (milestone 9).
    pub hemingway: Cell<bool>,
    /// The document's folder, for relative link paths on drop.
    pub doc_dir: RefCell<Option<PathBuf>>,
}

/// Callback invoked when a user deletion is blocked in Hemingway Mode.
pub type BlockedCallback = RefCell<Option<Box<dyn Fn()>>>;

pub fn install(
    view: &sourceview5::View,
    buffer: &sourceview5::Buffer,
    flags: Rc<InputFlags>,
    blocked_callback: Rc<BlockedCallback>,
) {
    let controller = gtk::EventControllerKey::new();
    controller.set_propagation_phase(gtk::PropagationPhase::Capture);

    let v = view.clone();
    let b = buffer.clone();
    let flags_for_keys = flags.clone();
    let blocked_for_keys = blocked_callback.clone();
    controller.connect_key_pressed(move |_, keyval, _keycode, state| {
        handle_key(&v, &b, &flags_for_keys, &blocked_for_keys, keyval, state)
    });
    view.add_controller(controller);

    // Hemingway Mode: cutting deletes the selection; pasting over a
    // selection deletes it too (§2). Additive pastes stay allowed. The
    // signal emission is stopped so the view's default handler never
    // runs while blocked.
    {
        let v = view.clone();
        let b = buffer.clone();
        let flags = flags.clone();
        let blocked_callback = blocked_callback.clone();
        view.connect_cut_clipboard(move |_| {
            if flags.hemingway.get() && b.has_selection() {
                v.stop_signal_emission_by_name("cut-clipboard");
                notify_blocked(&blocked_callback);
            }
        });
    }
    {
        let v = view.clone();
        let b = buffer.clone();
        let flags = flags.clone();
        let blocked_callback = blocked_callback.clone();
        view.connect_paste_clipboard(move |_| {
            if flags.hemingway.get() && b.has_selection() {
                v.stop_signal_emission_by_name("paste-clipboard");
                notify_blocked(&blocked_callback);
            }
        });
    }
}

fn handle_key(
    view: &sourceview5::View,
    buffer: &sourceview5::Buffer,
    flags: &Rc<InputFlags>,
    blocked_callback: &Rc<BlockedCallback>,
    keyval: gdk::Key,
    state: gdk::ModifierType,
) -> glib::Propagation {
    match keyval {
        gdk::Key::Return | gdk::Key::KP_Enter => handle_enter(buffer, flags),
        gdk::Key::Tab => handle_tab(buffer, flags, blocked_callback, false),
        gdk::Key::ISO_Left_Tab => {
            // Shift+Tab arrives as ISO_Left_Tab (sometimes with the shift
            // mask stripped); treat it as outdent regardless.
            let _ = state;
            handle_tab(buffer, flags, blocked_callback, true)
        }
        // Hemingway Mode (03/04 §2): backspace and delete are blocked.
        gdk::Key::BackSpace | gdk::Key::Delete | gdk::Key::KP_Delete => {
            if flags.hemingway.get() {
                notify_blocked(blocked_callback);
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        }
        _ => handle_character(buffer, flags, blocked_callback, keyval),
    }
}

// ---------------------------------------------------------------------------
// Auto-pairing
// ---------------------------------------------------------------------------

fn handle_character(
    buffer: &sourceview5::Buffer,
    flags: &Rc<InputFlags>,
    blocked_callback: &Rc<BlockedCallback>,
    keyval: gdk::Key,
) -> glib::Propagation {
    let Some(ch) = keyval.to_unicode() else {
        return glib::Propagation::Proceed;
    };

    let has_selection = buffer.selection_bounds().is_some();

    // Hemingway Mode: replacing a selection by typing is a deletion (§2).
    if has_selection && flags.hemingway.get() {
        notify_blocked(blocked_callback);
        return glib::Propagation::Stop;
    }
    let insert = buffer.iter_at_mark(&buffer.get_insert());
    let next_char = insert.char();

    if !has_selection {
        if let Some(closing) = autopair::closing_for(ch) {
            // Insert the pair only before whitespace or end of document.
            if insert.is_end() || next_char.is_whitespace() {
                buffer.begin_user_action();
                buffer.insert_at_cursor(&format!("{ch}{closing}"));
                // Move the cursor back between the pair.
                let insert_mark = buffer.get_insert();
                let mut iter = buffer.iter_at_mark(&insert_mark);
                iter.backward_char();
                buffer.place_cursor(&iter);
                buffer.end_user_action();
                return glib::Propagation::Stop;
            }
        } else if autopair::is_closing(ch) && next_char == ch {
            // Skip over the existing closing delimiter.
            let insert_mark = buffer.get_insert();
            let mut iter = buffer.iter_at_mark(&insert_mark);
            iter.forward_char();
            buffer.place_cursor(&iter);
            return glib::Propagation::Stop;
        }
    }

    glib::Propagation::Proceed
}

// ---------------------------------------------------------------------------
// Enter: list continuation
// ---------------------------------------------------------------------------

fn handle_enter(
    buffer: &sourceview5::Buffer,
    flags: &Rc<InputFlags>,
) -> glib::Propagation {
    let insert = buffer.iter_at_mark(&buffer.get_insert());
    let has_selection = buffer.selection_bounds().is_some();

    // Hemingway Mode: replacing a selection with a newline is a
    // deletion (§2).
    if has_selection && flags.hemingway.get() {
        return glib::Propagation::Stop;
    }

    // Magic applies only at the end of a line with no selection.
    if has_selection || !insert.ends_line() {
        return insert_plain(buffer, "\n");
    }

    let line_no = insert.line();
    let line = line_text(buffer, line_no);

    match lists::continuation(&line) {
        Some(lists::Continuation::ExitList) => {
            // Remove the empty marker line entirely (internal op, allowed
            // even in Hemingway Mode).
            buffer.begin_user_action();
            let mut start = buffer.iter_at_line(line_no).unwrap_or_else(|| buffer.start_iter());
            let mut end = buffer
                .iter_at_line(line_no + 1)
                .unwrap_or_else(|| buffer.end_iter());
            buffer.delete(&mut start, &mut end);
            buffer.end_user_action();
            glib::Propagation::Stop
        }
        Some(lists::Continuation::Prefix(prefix)) => {
            insert_plain(buffer, &format!("\n{prefix}"))
        }
        None => insert_plain(buffer, "\n"),
    }
}

pub fn line_text_pub(buffer: &sourceview5::Buffer, line_no: i32) -> String {
    line_text(buffer, line_no)
}

fn line_text(buffer: &sourceview5::Buffer, line_no: i32) -> String {
    let Some(start) = buffer.iter_at_line(line_no) else {
        return String::new();
    };
    let mut end = start;
    end.forward_to_line_end();
    buffer
        .text(&start, &end, true)
        .trim_end_matches('\r')
        .to_string()
}

fn insert_plain(buffer: &sourceview5::Buffer, text: &str) -> glib::Propagation {
    buffer.begin_user_action();
    buffer.insert_at_cursor(text);
    buffer.end_user_action();
    glib::Propagation::Stop
}

// ---------------------------------------------------------------------------
// Tab / Shift+Tab
// ---------------------------------------------------------------------------

fn handle_tab(
    buffer: &sourceview5::Buffer,
    flags: &Rc<InputFlags>,
    blocked_callback: &Rc<BlockedCallback>,
    outdent: bool,
) -> glib::Propagation {
    let insert = buffer.iter_at_mark(&buffer.get_insert());
    let line_no = insert.line();
    let line = line_text(buffer, line_no);
    let cursor_col = insert.line_offset() as usize;

    if outdent {
        let mut previous: Vec<String> = Vec::new();
        for back in 1..=50i32 {
            if line_no < back {
                break;
            }
            previous.push(line_text(buffer, line_no - back));
        }
        let previous_refs: Vec<&str> = previous.iter().map(String::as_str).collect();
        match lists::outdent(&line, cursor_col, &previous_refs) {
            lists::Outdent::Rewritten(new_line, new_col) => {
                rewrite_line(buffer, line_no, &new_line);
                if let Some(mut iter) = buffer.iter_at_line(line_no) {
                    iter.forward_chars(new_col as i32);
                    buffer.place_cursor(&iter);
                }
                glib::Propagation::Stop
            }
            lists::Outdent::RemoveTabBefore => {
                if flags.hemingway.get() {
                    notify_blocked(blocked_callback);
                    return glib::Propagation::Stop;
                }
                buffer.begin_user_action();
                let mut end = buffer.iter_at_mark(&buffer.get_insert());
                let mut start = end;
                if start.backward_char() && start.char() == '\t' {
                    buffer.delete(&mut start, &mut end);
                }
                buffer.end_user_action();
                glib::Propagation::Stop
            }
            lists::Outdent::Noop => glib::Propagation::Stop,
        }
    } else {
        // Plain Tab inserts a tab; an empty list item is rewritten deeper.
        if let Some(new_line) = lists::indent_empty_item(&line) {
            rewrite_line(buffer, line_no, &new_line);
            if let Some(mut iter) = buffer.iter_at_line(line_no) {
                iter.forward_to_line_end();
                buffer.place_cursor(&iter);
            }
            return glib::Propagation::Stop;
        }
        buffer.begin_user_action();
        buffer.insert_at_cursor("\t");
        buffer.end_user_action();
        glib::Propagation::Stop
    }
}

fn rewrite_line(buffer: &sourceview5::Buffer, line_no: i32, new_text: &str) {
    buffer.begin_user_action();
    let Some(mut start) = buffer.iter_at_line(line_no) else {
        buffer.end_user_action();
        return;
    };
    let mut end = start;
    end.forward_to_line_end();
    buffer.delete(&mut start, &mut end);
    buffer.insert(&mut start, new_text);
    buffer.end_user_action();
}

fn notify_blocked(blocked_callback: &Rc<BlockedCallback>) {
    if let Some(cb) = blocked_callback.borrow().as_ref() {
        cb();
    }
}

// ---------------------------------------------------------------------------
// Drag and drop (§3)
// ---------------------------------------------------------------------------

/// Install drop handlers for files and remote URLs. Plain-text drops are
/// left to the text view's default handling (inserted verbatim).
pub fn install_dnd(
    view: &sourceview5::View,
    buffer: &sourceview5::Buffer,
    flags: Rc<InputFlags>,
) {
    let files_target = gtk::DropTarget::new(
        gdk::FileList::static_type(),
        gdk::DragAction::COPY | gdk::DragAction::MOVE,
    );
    {
        let buffer = buffer.clone();
        let flags = flags.clone();
        let view = view.clone();
        files_target.connect_drop(move |_, value, x, y| {
            let Ok(file_list) = value.get::<gdk::FileList>() else {
                return false;
            };
            let iter = view
                .iter_at_location(x as i32, y as i32)
                .unwrap_or_else(|| buffer.iter_at_mark(&buffer.get_insert()));
            let inserted = file_list
                .files()
                .iter()
                .map(|f| markdown_for_uri(&flags, f))
                .collect::<Vec<_>>()
                .join("\n");
            if inserted.is_empty() {
                return false;
            }
            buffer.begin_user_action();
            buffer.insert(&mut iter.clone(), &inserted);
            buffer.end_user_action();
            true
        });
    }
    view.add_controller(files_target);

    let uri_target = gtk::DropTarget::new(
        glib::types::Type::STRING,
        gdk::DragAction::COPY | gdk::DragAction::MOVE,
    );
    {
        let buffer = buffer.clone();
        let view = view.clone();
        uri_target.connect_drop(move |_, value, x, y| {
            let Ok(text) = value.get::<String>() else {
                return false;
            };
            let uri = text.trim();
            if !uri.starts_with("http://")
                && !uri.starts_with("https://")
                && !uri.starts_with("ftp://")
            {
                return false;
            }
            let iter = view
                .iter_at_location(x as i32, y as i32)
                .unwrap_or_else(|| buffer.iter_at_mark(&buffer.get_insert()));
            buffer.begin_user_action();
            buffer.insert(&mut iter.clone(), &format!("[web page]({uri})"));
            buffer.end_user_action();
            true
        });
    }
    view.add_controller(uri_target);
}

fn markdown_for_uri(flags: &Rc<InputFlags>, file: &gio::File) -> String {
    let uri = file.uri().to_string();
    if uri.starts_with("http://") || uri.starts_with("https://") || uri.starts_with("ftp://") {
        return format!("[web page]({uri})");
    }
    let Some(path) = file.path() else {
        return String::new();
    };
    let is_image = file
        .query_info(
            "standard::content-type",
            gio::FileQueryInfoFlags::NONE,
            gio::Cancellable::NONE,
        )
        .ok()
        .and_then(|i| i.content_type().map(|c| c.starts_with("image/")))
        .unwrap_or(false);

    let label = crate::files::paths::decode_label(
        &path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
    );
    let link = crate::files::paths::link_path(flags.doc_dir.borrow().as_deref(), &path);
    if is_image {
        format!("![{label}]({link})")
    } else {
        format!("[{label}]({link})")
    }
}
