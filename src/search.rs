//! Find and replace bar (`03-behavior/07-search-and-replace.md`,
//! `02-ui-layout.md` §6). Search state is per window; matching uses the
//! platform search context (highlighting, regex, replacement, wrap-around).
//! The context is bound to one buffer for its lifetime, so the slot is
//! swapped for a fresh context whenever the search targets another tab's
//! document; all contexts share one `SearchSettings`, so the query and
//! options follow.
//!
//! The occurrence count is only known once the context's background scan
//! has covered the buffer — until then it reports -1 and the scan is queued
//! in an idle. Button and no-match state is therefore driven by the
//! context's `occurrences-count` notification, never read synchronously
//! right after the query changes.

use gtk::prelude::*;
use crate::i18n::i18n;
use sourceview5::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;

#[derive(Clone)]
pub struct SearchBar {
    pub bar: gtk::SearchBar,
    entry: gtk::SearchEntry,
    replace_row: gtk::Revealer,
    replace_entry: gtk::Entry,
    prev_button: gtk::Button,
    next_button: gtk::Button,
    replace_button: gtk::Button,
    replace_all_button: gtk::Button,
    case_toggle: gtk::ToggleButton,
    regex_toggle: gtk::ToggleButton,
    replace_toggle: gtk::ToggleButton,
    /// The context for the buffer the search currently targets. Shared by
    /// value across all clones of this struct.
    context: Rc<RefCell<sourceview5::SearchContext>>,
    settings: sourceview5::SearchSettings,
    /// The view of the tab the search currently targets (centered match
    /// scrolling and focus return). Shared mutable state: cloned widget
    /// handles would go stale on tab switch.
    view: Rc<RefCell<sourceview5::View>>,
    /// Buffers that already carry a cursor-position listener (see
    /// `watch_context`).
    watched_buffers: Rc<RefCell<HashSet<usize>>>,
    /// Cancels a running match-scroll animation when a new one starts.
    scroll_generation: Rc<Cell<u64>>,
}

fn make_context(
    buffer: &sourceview5::Buffer,
    settings: &sourceview5::SearchSettings,
) -> sourceview5::SearchContext {
    let context = sourceview5::SearchContext::new(buffer, Some(settings));
    context.set_highlight(true);
    context
}

impl SearchBar {
    pub fn new(view: &sourceview5::View, buffer: &sourceview5::Buffer) -> Self {
        let settings = sourceview5::SearchSettings::new();
        settings.set_case_sensitive(false);
        settings.set_regex_enabled(false);
        // Searching wraps around the document ends (07 §1).
        settings.set_wrap_around(true);

        let context = Rc::new(RefCell::new(make_context(buffer, &settings)));

        let bar = gtk::SearchBar::new();
        bar.set_show_close_button(true);

        let main = gtk::Box::new(gtk::Orientation::Vertical, 6);
        let row1 = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let row2 = gtk::Revealer::new();
        let replace_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);

        let entry = gtk::SearchEntry::new();
        entry.set_hexpand(true);
        entry.set_placeholder_text(Some(&i18n("Find")));
        row1.append(&entry);

        let prev_button = gtk::Button::from_icon_name("go-up-symbolic");
        prev_button.set_tooltip_text(Some(&i18n("Previous Match")));
        let next_button = gtk::Button::from_icon_name("go-down-symbolic");
        next_button.set_tooltip_text(Some(&i18n("Next Match")));
        row1.append(&prev_button);
        row1.append(&next_button);

        let case_toggle = gtk::ToggleButton::with_label("aA");
        case_toggle.set_tooltip_text(Some(&i18n("Match Case")));
        let regex_toggle = gtk::ToggleButton::with_label("(.*)");
        regex_toggle.set_tooltip_text(Some(&i18n("Regular Expressions")));
        row1.append(&case_toggle);
        row1.append(&regex_toggle);

        let replace_toggle = gtk::ToggleButton::with_label("⇄");
        replace_toggle.set_tooltip_text(Some(&i18n("Replace")));
        row1.append(&replace_toggle);

        let replace_entry = gtk::Entry::new();
        replace_entry.set_hexpand(true);
        replace_entry.set_placeholder_text(Some(&i18n("Replace with")));
        replace_row.append(&replace_entry);

        let replace_button = gtk::Button::with_label(&i18n("Replace"));
        let replace_all_button = gtk::Button::with_label(&i18n("Replace All"));
        replace_row.append(&replace_button);
        replace_row.append(&replace_all_button);

        row2.set_child(Some(&replace_row));
        row2.set_reveal_child(false);
        main.append(&row1);
        main.append(&row2);
        bar.set_child(Some(&main));

        let sb = Self {
            bar,
            entry,
            replace_row: row2,
            replace_entry,
            prev_button,
            next_button,
            replace_button,
            replace_all_button,
            case_toggle,
            regex_toggle,
            replace_toggle,
            context,
            settings,
            view: Rc::new(RefCell::new(view.clone())),
            watched_buffers: Rc::new(RefCell::new(HashSet::new())),
            scroll_generation: Rc::new(Cell::new(0)),
        };
        sb.watch_context(&sb.context());
        sb.wire();
        sb
    }

    fn context(&self) -> sourceview5::SearchContext {
        self.context.borrow().clone()
    }

    /// Attach the per-context listeners. The occurrence count drives the
    /// no-match state (it arrives asynchronously); cursor moves re-check
    /// the Replace button, whose sensitivity is tied to the current
    /// selection being exactly an occurrence.
    fn watch_context(&self, context: &sourceview5::SearchContext) {
        let sb = self.clone_handles();
        context.connect_notify_local(Some("occurrences-count"), move |_, _| {
            sb.update_match_ui();
        });
        // One listener per buffer ever: each swap creates a fresh context,
        // but a tab's buffer lives on. The handlers always act on the
        // current context, so duplicates from earlier swaps would only be
        // redundant work on every cursor move.
        let buffer = context.buffer();
        let key = buffer.as_ptr() as usize;
        if self.watched_buffers.borrow_mut().insert(key) {
            let sb = self.clone_handles();
            buffer.connect_notify_local(Some("cursor-position"), move |_, _| {
                sb.update_replace_sensitivity();
            });
        }
    }

    /// Point the search at `view`/`buffer`. The old context's highlighting
    /// is switched off first so background tabs do not keep scanning for
    /// matches. Swapping to the buffer already targeted is a no-op — the
    /// context keeps its scan progress.
    fn swap_context(&self, view: &sourceview5::View, buffer: &sourceview5::Buffer) {
        *self.view.borrow_mut() = view.clone();
        let mut slot = self.context.borrow_mut();
        if slot.buffer().as_ptr() == buffer.as_ptr() {
            return;
        }
        slot.set_highlight(false);
        let context = make_context(buffer, &self.settings);
        self.watch_context(&context);
        *slot = context;
    }

    fn wire(&self) {
        // Live search updates.
        {
            let sb = self.clone_handles();
            self.entry
                .connect_search_changed(move |_| sb.refresh_from_start());
        }
        // Enter jumps to the next match; Ctrl+G / Ctrl+Shift+G are the
        // platform bindings for next/previous match.
        {
            let sb = self.clone_handles();
            self.entry.connect_activate(move |_| sb.jump_next());
        }
        {
            let sb = self.clone_handles();
            self.entry.connect_next_match(move |_| sb.jump_next());
        }
        {
            let sb = self.clone_handles();
            self.entry.connect_previous_match(move |_| sb.jump_prev());
        }
        // Options.
        {
            let sb = self.clone_handles();
            self.case_toggle.connect_toggled(move |t| {
                sb.settings.set_case_sensitive(t.is_active());
                sb.refresh_from_start();
            });
        }
        {
            let sb = self.clone_handles();
            self.regex_toggle.connect_toggled(move |t| {
                sb.settings.set_regex_enabled(t.is_active());
                sb.refresh_from_start();
            });
        }
        {
            let sb = self.clone_handles();
            self.replace_toggle.connect_toggled(move |t| {
                sb.replace_row.set_reveal_child(t.is_active());
                if t.is_active() {
                    sb.replace_entry.grab_focus();
                }
            });
        }
        // Navigation.
        {
            let sb = self.clone_handles();
            self.next_button.connect_clicked(move |_| sb.jump_next());
        }
        {
            let sb = self.clone_handles();
            self.prev_button.connect_clicked(move |_| sb.jump_prev());
        }
        // Replace the selected match and advance (07 §2). The button is
        // only sensitive while the selection is exactly an occurrence;
        // replace() revalidates the bounds and expands regex
        // backreferences itself, so the replacement text is passed as-is.
        {
            let sb = self.clone_handles();
            self.replace_button.connect_clicked(move |_| {
                let context = sb.context();
                let buffer = context.buffer();
                let Some((s, e)) = buffer.selection_bounds() else {
                    return;
                };
                let (mut s, mut e) = (s, e);
                let replacement = sb.replace_entry.text().to_string();
                if context.replace(&mut s, &mut e, &replacement).is_ok() {
                    sb.jump_next();
                }
            });
        }
        // Replace all in one undoable step.
        {
            let sb = self.clone_handles();
            self.replace_all_button.connect_clicked(move |_| {
                let replacement = sb.replace_entry.text().to_string();
                let _count = sb.context().replace_all(&replacement);
                sb.refresh_from_start();
            });
        }
        // Esc emits stop-search on the entry; close the bar with it
        // (`04-keyboard-shortcuts.md`, 17-accessibility.md §1).
        {
            let sb = self.clone_handles();
            self.entry.connect_stop_search(move |_| {
                sb.bar.set_search_mode(false);
            });
        }
        // Closing the bar removes highlighting, closes replace mode and
        // returns focus to the editor (07 §3).
        {
            let sb = self.clone_handles();
            self.bar.connect_notify_local(
                Some("search-mode-enabled"),
                move |bar, _pspec| {
                    let visible = bar.is_search_mode();
                    let context = sb.context();
                    if !visible {
                        context.set_highlight(false);
                        sb.replace_row.set_reveal_child(false);
                        sb.replace_toggle.set_active(false);
                        let view = sb.view.borrow();
                        if view.is_mapped() {
                            view.grab_focus();
                        }
                    } else {
                        context.set_highlight(true);
                    }
                },
            );
        }
    }

    fn clone_handles(&self) -> Self {
        Self {
            bar: self.bar.clone(),
            entry: self.entry.clone(),
            replace_row: self.replace_row.clone(),
            replace_entry: self.replace_entry.clone(),
            prev_button: self.prev_button.clone(),
            next_button: self.next_button.clone(),
            replace_button: self.replace_button.clone(),
            replace_all_button: self.replace_all_button.clone(),
            case_toggle: self.case_toggle.clone(),
            regex_toggle: self.regex_toggle.clone(),
            replace_toggle: self.replace_toggle.clone(),
            context: self.context.clone(),
            settings: self.settings.clone(),
            view: self.view.clone(),
            watched_buffers: self.watched_buffers.clone(),
            scroll_generation: self.scroll_generation.clone(),
        }
    }

    /// Apply the new query and select the match at the cursor. The match
    /// count lands asynchronously (the context watcher finishes the UI
    /// state); navigation itself works immediately, since `forward()`
    /// scans on demand.
    fn refresh_from_start(&self) {
        let text = self.entry.text().to_string();
        self.settings
            .set_search_text(if text.is_empty() { None } else { Some(text.as_str()) });
        if !text.is_empty() {
            self.select_query_at_cursor();
        }
        self.update_match_ui();
    }

    /// Buttons and the no-match indicator from the live occurrence count.
    /// A count of -1 means the background scan has not covered the buffer
    /// yet — treated as "matches possible", since navigation works
    /// regardless. An empty query is not a no-match state.
    fn update_match_ui(&self) {
        if !self.bar.is_search_mode() {
            return;
        }
        let empty = self.entry.text().is_empty();
        let count = if empty {
            0
        } else {
            self.context().occurrences_count()
        };
        let none = count == 0;
        self.entry.remove_css_class("error");
        if none {
            self.entry.add_css_class("error");
        }
        self.next_button.set_sensitive(!none);
        self.prev_button.set_sensitive(!none);
        self.replace_all_button.set_sensitive(!none);
        self.replace_button.set_sensitive(self.selection_is_match());
    }

    /// The selection moved: re-check whether it is exactly an occurrence.
    fn update_replace_sensitivity(&self) {
        if !self.bar.is_search_mode() {
            return;
        }
        self.replace_button.set_sensitive(self.selection_is_match());
    }

    /// Whether the current selection is exactly one occurrence. `forward()`
    /// returns a match starting exactly at the given position, so searching
    /// from the selection start identifies the selected match.
    fn selection_is_match(&self) -> bool {
        let context = self.context();
        let buffer = context.buffer();
        let Some((s, e)) = buffer.selection_bounds() else {
            return false;
        };
        if s.offset() >= e.offset() {
            return false;
        }
        match context.forward(&s) {
            Some((ms, me, _)) => ms.offset() == s.offset() && me.offset() == e.offset(),
            None => false,
        }
    }

    /// After the query changed: select the match starting at the current
    /// selection or cursor position, when it still matches; otherwise the
    /// next one. Growing a query keeps the match at the cursor instead of
    /// marching forward through the document.
    fn select_query_at_cursor(&self) {
        let context = self.context();
        let buffer = context.buffer();
        let start = match buffer.selection_bounds() {
            Some((s, _)) => s,
            None => buffer.iter_at_mark(&buffer.get_insert()),
        };
        if let Some((ms, me, _)) = context.forward(&start) {
            if ms.offset() == start.offset() {
                // The match at the cursor still matches — reselect it in
                // place (cursor on the match end, like jump_next).
                buffer.select_range(&me, &ms);
                self.scroll_to_center(&me);
                return;
            }
        }
        self.jump_next();
    }

    pub fn jump_next(&self) {
        let context = self.context();
        let buffer = context.buffer();
        let insert = buffer.iter_at_mark(&buffer.get_insert());
        // Search from the selection end: forward() re-finds a match
        // starting exactly at the search position, so starting from a
        // selected match's start (where the cursor may sit) would select
        // it again forever.
        let from = match buffer.selection_bounds() {
            Some((_s, e)) if e.offset() > insert.offset() => e,
            _ => insert,
        };
        if let Some((ms, me, _wrap)) = context.forward(&from) {
            // select_range(ins, bound): the cursor goes to the FIRST
            // argument — put it on the match end so navigation advances.
            buffer.select_range(&me, &ms);
            self.scroll_to_center(&me);
        }
    }

    pub fn jump_prev(&self) {
        let context = self.context();
        let buffer = context.buffer();
        let insert = buffer.iter_at_mark(&buffer.get_insert());
        // Search backward from the selection start so the current
        // selection (a match or not) is skipped, not re-found.
        let from = match buffer.selection_bounds() {
            Some((s, _)) => s,
            None => insert,
        };
        if let Some((ms, me, _wrap)) = context.backward(&from) {
            // Cursor on the match start: the next backward jump starts
            // from there and keeps advancing.
            buffer.select_range(&ms, &me);
            self.scroll_to_center(&ms);
        }
    }

    /// Smooth-scroll `iter` to the vertical center of the view (07 §1).
    /// GTK has no animated scroll API: capture the target by scrolling
    /// once, restore, then glide the adjustment there. The editor's own
    /// just-visible cursor scroll (which fires on the selection) has
    /// already settled by the time this runs.
    fn scroll_to_center(&self, iter: &gtk::TextIter) {
        let view = self.view.borrow();
        if !view.is_mapped() {
            return; // nothing visible to scroll; it settles when mapped
        }
        let Some(adj) = view.vadjustment() else {
            return;
        };
        let before = adj.value();
        let mut it = iter.clone();
        if !view.scroll_to_iter(&mut it, 0.0, true, 0.5, 0.5) {
            return; // the just-visible scroll stands
        }
        let target = adj.value();
        if (target - before).abs() < 1.0 {
            return; // already centered
        }
        adj.set_value(before);
        let gen_rc = self.scroll_generation.clone();
        let gen_no = gen_rc.get() + 1;
        gen_rc.set(gen_no);
        glib::timeout_add_local(std::time::Duration::from_millis(16), move || {
            if gen_rc.get() != gen_no {
                return glib::ControlFlow::Break;
            }
            let remaining = target - adj.value();
            // Ease-out: cover 30% of the remaining distance per frame.
            let step = remaining * 0.3;
            if step.abs() < 1.0 {
                adj.set_value(target);
                return glib::ControlFlow::Break;
            }
            adj.set_value(adj.value() + step);
            glib::ControlFlow::Continue
        });
    }

    /// Open find (optionally with the replace row revealed), retargeting
    /// the search to `view`/`buffer` (the active tab's document). The
    /// entry is not restored on reopen (07 §3): it starts empty and is
    /// pre-filled only by the current selection, when present.
    pub fn open(&self, with_replace: bool, view: &sourceview5::View, buffer: &sourceview5::Buffer) {
        let was_open = self.bar.is_search_mode();
        self.swap_context(view, buffer);
        self.replace_row.set_reveal_child(with_replace);
        self.replace_toggle.set_active(with_replace);
        self.bar.set_search_mode(true);
        if !was_open {
            let mut prefilled = false;
            if let Some((s, e)) = buffer.selection_bounds() {
                let text: String = buffer.text(&s, &e, true).into();
                if !text.contains('\n') && !text.is_empty() {
                    self.entry.set_text(&text);
                    prefilled = true;
                }
            }
            if !prefilled {
                self.entry.set_text("");
            }
            self.refresh_from_start();
        }
        self.entry.grab_focus();
    }

    /// Point the search at another tab's document (tab switch) without
    /// opening the bar; matches re-resolve lazily against the new
    /// document.
    pub fn retarget(&self, view: &sourceview5::View, buffer: &sourceview5::Buffer) {
        if self.bar.is_search_mode() {
            self.swap_context(view, buffer);
        }
    }

    pub fn close(&self) {
        self.bar.set_search_mode(false);
    }

    pub fn is_open(&self) -> bool {
        self.bar.is_search_mode()
    }
}
