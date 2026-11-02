//! Find and replace bar (`03-behavior/07-search-and-replace.md`,
//! `02-ui-layout.md` §6). Search state is per window; matching uses the
//! platform search context (highlighting, regex, replacement). The context
//! is bound to one buffer for its lifetime, so the slot is swapped for a
//! fresh context whenever the search targets another tab's document; all
//! contexts share one `SearchSettings`, so the query and options follow.

use gtk::prelude::*;
use crate::i18n::i18n;
use sourceview5::prelude::*;
use std::cell::RefCell;
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
        };
        sb.wire(view, buffer);
        sb
    }

    fn context(&self) -> sourceview5::SearchContext {
        self.context.borrow().clone()
    }

    /// Swap the context over to `buffer`'s document. The old context's
    /// highlighting is switched off first so background tabs do not keep
    /// scanning for matches.
    fn swap_context(&self, buffer: &sourceview5::Buffer) {
        if let Ok(mut slot) = self.context.try_borrow_mut() {
            slot.set_highlight(false);
            *slot = make_context(buffer, &self.settings);
        }
    }

    fn wire(&self, _view: &sourceview5::View, _buffer: &sourceview5::Buffer) {
        // Live search updates.
        {
            let sb = self.clone_handles();
            self.entry
                .connect_search_changed(move |_| sb.refresh_from_start());
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
        // Replace current match and advance.
        {
            let sb = self.clone_handles();
            self.replace_button.connect_clicked(move |_| {
                let context = sb.context();
                let buffer = context.buffer();
                let start = buffer.iter_at_mark(&buffer.get_insert());
                let Some((ms, me, _)) = context.forward(&start) else {
                    return;
                };
                let mut ms = ms;
                let mut me = me;
                let sel = buffer.selection_bounds();
                let exact = sel
                    .map(|(s, e)| s.offset() == ms.offset() && e.offset() == me.offset())
                    .unwrap_or(false);
                if exact {
                    let replacement = sb.replacement_for(&ms, &me);
                    if context.replace(&mut ms, &mut me, &replacement).is_ok() {
                        sb.jump_next();
                    }
                } else {
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
        // Escape closes the bar (SearchBar handles it when the entry has
        // focus) and returns focus to the editor.
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
        }
    }

    fn refresh_from_start(&self) {
        let text = self.entry.text().to_string();
        self.settings.set_search_text(if text.is_empty() { None } else { Some(text.as_str()) });
        // Update no-match state and button sensitivity.
        let count = if text.is_empty() {
            -1
        } else {
            self.context().occurrences_count()
        };
        let no_match = count == 0;
        self.entry
            .remove_css_class("error");
        if no_match {
            self.entry.add_css_class("error");
        }
        self.next_button.set_sensitive(!no_match && count > 0);
        self.prev_button.set_sensitive(!no_match && count > 0);
        self.replace_button.set_sensitive(!no_match && count > 0);
        self.replace_all_button.set_sensitive(!no_match && count > 0);
        if count > 0 {
            self.jump_next();
        }
    }

    fn selected_match_start(&self, _buffer: &sourceview5::Buffer) -> Option<gtk::TextIter> {
        let buffer = self.context().buffer();
        let insert = buffer.iter_at_mark(&buffer.get_insert());
        Some(insert)
    }

    /// Regex replacements support `\1`-style backreferences.
    fn replacement_for(
        &self,
        start: &gtk::TextIter,
        end: &gtk::TextIter,
    ) -> String {
        let raw = self.replace_entry.text().to_string();
        if !self.settings.is_regex_enabled() {
            return raw;
        }
        let matched: String = self
            .context()
            .buffer()
            .text(start, end, true)
            .into();
        let re = regex::Regex::new(&matched).ok();
        match re {
            Some(_re) => expand_backreferences(&raw, &matched),
            None => raw,
        }
    }

    pub fn jump_next(&self) {
        let context = self.context();
        let buffer = context.buffer();
        let insert = buffer.iter_at_mark(&buffer.get_insert());
        if let Some((ms, me, _wrap)) = context.forward(&insert) {
            buffer.select_range(&ms, &me);
            self.scroll_to(&me);
        }
    }

    pub fn jump_prev(&self) {
        let context = self.context();
        let buffer = context.buffer();
        let insert = buffer.iter_at_mark(&buffer.get_insert());
        if let Some((ms, me, _wrap)) = context.backward(&insert) {
            buffer.select_range(&ms, &me);
            self.scroll_to(&ms);
        }
    }

    fn scroll_to(&self, iter: &gtk::TextIter) {
        // The window wires the actual scrolling via the editor's cursor
        // handling; selecting the match is enough here.
        let _ = iter;
    }

    /// Open find (optionally with the replace row revealed), pre-filling
    /// the entry with the selection when present. The search context is
    /// retargeted to `buffer` (the active tab's document).
    pub fn open(&self, with_replace: bool, buffer: &sourceview5::Buffer) {
        self.swap_context(buffer);
        self.replace_row.set_reveal_child(with_replace);
        self.replace_toggle.set_active(with_replace);
        self.bar.set_search_mode(true);
        if let Some((s, e)) = buffer.selection_bounds() {
            let text: String = buffer.text(&s, &e, true).into();
            if !text.contains('\n') && !text.is_empty() {
                self.entry.set_text(&text);
            }
            self.refresh_from_start();
        }
        self.entry.grab_focus();
    }

    /// Point the search at another buffer (tab switch) without opening the
    /// bar; matches re-resolve lazily against the new document.
    pub fn retarget(&self, buffer: &sourceview5::Buffer) {
        if self.bar.is_search_mode() {
            self.swap_context(buffer);
        }
    }

    pub fn close(&self) {
        self.bar.set_search_mode(false);
    }

    pub fn is_open(&self) -> bool {
        self.bar.is_search_mode()
    }
}

/// Expand `\1` … `\9` backreferences against the matched text.
fn expand_backreferences(pattern: &str, matched: &str) -> String {
    if let Ok(re) = regex::Regex::new(pattern) {
        if let Some(caps) = re.captures(matched) {
            let mut out = String::new();
            let mut chars = pattern.chars().peekable();
            while let Some(c) = chars.next() {
                if c == '\\' {
                    if let Some(d) = chars.peek() {
                        if let Some(n) = d.to_digit(10) {
                            let idx = n as usize;
                            if idx < caps.len() {
                                if let Some(m) = caps.get(idx) {
                                    out.push_str(m.as_str());
                                }
                            }
                            chars.next();
                            continue;
                        }
                    }
                    out.push('\\');
                } else {
                    out.push(c);
                }
            }
            return out;
        }
    }
    pattern.to_owned()
}
