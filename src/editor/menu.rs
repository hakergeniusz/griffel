//! The editor context menu (`02-ui-layout.md` §10): the platform's
//! standard text editing menu (cut/copy/paste/select all — provided by
//! GtkTextView itself) extended with spelling suggestions for a misspelled
//! word, the spell-language submenu, a spell-check toggle and a Peek item.
//! Ctrl+click with no selection also opens the Peek popover.

use gio::prelude::*;
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use super::Editor;

/// Where a spelling replacement applies (char offsets), captured when the
/// menu is opened.
struct MenuState {
    replace_range: RefCell<Option<(i32, i32)>>,
}

pub fn install(editor: &Editor) {
    let state = Rc::new(MenuState {
        replace_range: RefCell::new(None),
    });
    let actions = gio::SimpleActionGroup::new();
    editor.view.insert_action_group("ctx", Some(&actions));

    // Spelling suggestion replacement.
    let replace = gio::SimpleAction::new("replace", Some(glib::VariantTy::STRING));
    {
        let ed = editor.clone();
        let st = state.clone();
        replace.connect_activate(move |_, param| {
            let Some(text) = param.and_then(|p| p.str().map(str::to_owned)) else {
                return;
            };
            let Some((s, e)) = *st.replace_range.borrow() else {
                return;
            };
            let buf = &ed.buffer;
            let (mut start, mut end) = (buf.iter_at_offset(s), buf.iter_at_offset(e));
            buf.begin_user_action();
            buf.delete(&mut start, &mut end);
            let mut at = buf.iter_at_offset(s);
            buf.insert(&mut at, &text);
            buf.end_user_action();
        });
    }
    actions.add_action(&replace);

    // Spell language (empty string = system default). Persisted as file
    // metadata when the document has a backing file (`01-editing.md` §4).
    let language = gio::SimpleAction::new("language", Some(glib::VariantTy::STRING));
    {
        let ed = editor.clone();
        language.connect_activate(move |_, param| {
            let v = param.and_then(|p| p.str().map(str::to_owned)).unwrap_or_default();
            let lang = if v.is_empty() { None } else { Some(v.clone()) };
            ed.set_spell_language(lang);
            if let Some(file) = ed.spell_persist_target.borrow().clone() {
                crate::files::metadata::store_language(&file, &v);
            }
        });
    }
    actions.add_action(&language);

    // Spell-check toggle (checkbox; mirrors the global preference).
    let spell = gio::SimpleAction::new_stateful(
        "spell",
        None,
        &crate::settings::settings().boolean("spellcheck").to_variant(),
    );
    {
        spell.connect_activate(move |action, _| {
            let new = !action
                .state()
                .and_then(|v| v.get::<bool>())
                .unwrap_or(true);
            crate::settings::settings().set_boolean("spellcheck", new);
            action.set_state(&new.to_variant());
        });
    }
    actions.add_action(&spell);

    // Peek at the construct under the cursor.
    let peek = gio::SimpleAction::new("peek", None);
    {
        let ed = editor.clone();
        peek.connect_activate(move |_, _| {
            let buf = &ed.buffer;
            let (s, _e) = buf.selection_bounds().unwrap_or_else(|| {
                let i = buf.iter_at_mark(&buf.get_insert());
                (i, i)
            });
            crate::peek::open_at_iter(&ed, &s);
        });
    }
    actions.add_action(&peek);

    // Editing commands (the built-in menu also has these; keeping them
    // here costs nothing and works if the built-in menu is unavailable).
    macro_rules! buf_cmd {
        ($name:expr, $f:expr) => {{
            let action = gio::SimpleAction::new($name, None);
            let ed = editor.clone();
            action.connect_activate(move |_, _| {
                let buf = &ed.buffer;
                ($f)(buf, &ed.view.clipboard());
            });
            actions.add_action(&action);
        }};
    }
    buf_cmd!("cut", |b: &sourceview5::Buffer, c: &gdk::Clipboard| b
        .cut_clipboard(c, true));
    buf_cmd!("copy", |b: &sourceview5::Buffer, c: &gdk::Clipboard| b
        .copy_clipboard(c));
    buf_cmd!("paste", |b: &sourceview5::Buffer, c: &gdk::Clipboard| b
        .paste_clipboard(c, None::<&gtk::TextIter>, true));
    buf_cmd!("select-all", |b: &sourceview5::Buffer, _c: &gdk::Clipboard| {
        let (s, e) = (b.start_iter(), b.end_iter());
        b.select_range(&s, &e);
    });

    // The extra menu is rebuilt on every right-click press so the
    // suggestion list matches the word under the pointer; the built-in
    // text-view menu then opens with these items appended.
    {
        let ed = editor.clone();
        let st = state.clone();
        let lang_action = language.clone();
        let spell_action = spell.clone();
        let gesture = gtk::GestureClick::new();
        gesture.set_button(3);
        gesture.set_propagation_phase(gtk::PropagationPhase::Capture);
        gesture.connect_pressed(move |g, _n, x, y| {
            let (bx, by) = ed
                .view
                .window_to_buffer_coords(gtk::TextWindowType::Widget, x as i32, y as i32);
            let iter = ed.view.iter_at_location(bx, by).unwrap_or_else(|| {
                ed.buffer.iter_at_mark(&ed.buffer.get_insert())
            });
            rebuild_menu(&ed, &st, &lang_action, &spell_action, &iter);
            let _ = g;
        });
        editor.view.add_controller(gesture);
    }

    // Ctrl+click with no selection opens the peek popover (§1).
    {
        let ed = editor.clone();
        let gesture = gtk::GestureClick::new();
        gesture.set_button(1);
        gesture.set_propagation_phase(gtk::PropagationPhase::Capture);
        gesture.connect_pressed(move |g, _n, x, y| {
            let state = g.current_event_state();
            if !state.contains(gdk::ModifierType::CONTROL_MASK) {
                return;
            }
            if ed.buffer.has_selection() {
                return;
            }
            g.set_state(gtk::EventSequenceState::Claimed);
            let (bx, by) = ed
                .view
                .window_to_buffer_coords(gtk::TextWindowType::Widget, x as i32, y as i32);
            crate::peek::open_at_coords(&ed, bx, by);
        });
        editor.view.add_controller(gesture);
    }
}

/// Build the extra context-menu model for a right-click at `iter`.
fn rebuild_menu(
    editor: &Editor,
    state: &MenuState,
    lang_action: &gio::SimpleAction,
    spell_action: &gio::SimpleAction,
    iter: &gtk::TextIter,
) {
    // Capture the misspelled word under the pointer, if any.
    let tag = editor.buffer.tag_table().lookup(crate::editor::spellcheck::TAG);
    let mut range = None;
    if let Some(tag) = tag.filter(|t| iter.has_tag(t)) {
        let mut s = *iter;
        if !s.starts_tag(Some(&tag)) {
            s.backward_to_tag_toggle(Some(&tag));
        }
        let mut e = *iter;
        if !e.ends_tag(Some(&tag)) {
            e.forward_to_tag_toggle(Some(&tag));
        }
        range = Some((s.offset(), e.offset()));
    }
    *state.replace_range.borrow_mut() = range;

    let menu = gio::Menu::new();

    // Spelling suggestions (up to six, top level).
    if let Some((s, e)) = range {
        let word: String = editor
            .buffer
            .iter_at_offset(s)
            .text(&editor.buffer.iter_at_offset(e))
            .into();
        let lang = editor.spell_language.borrow().clone();
        let suggestions = editor.spell.suggestions(lang.as_deref(), &word);
        if suggestions.is_empty() {
            let item = gio::MenuItem::new(Some("No Suggestions"), None);
            item.set_attribute_value("enabled", Some(&false.to_variant()));
            menu.append_item(&item);
        } else {
            for w in suggestions.iter().take(6) {
                menu.append(Some(w), Some(&format!("ctx.replace::{w}")));
            }
        }
        menu.append_section(None, &gio::Menu::new());
    }

    // Spell language submenu (radio via the action state).
    let langs = editor.spell.languages();
    if !langs.is_empty() {
        let current = editor
            .spell_language
            .borrow()
            .clone()
            .unwrap_or_default();
        lang_action.set_state(&current.to_variant());
        let submenu = gio::Menu::new();
        submenu.append(Some("System Default"), Some("ctx.language::"));
        for l in &langs {
            submenu.append(Some(l), Some(&format!("ctx.language::{l}")));
        }
        menu.append_submenu(Some("Spelling Language"), &submenu);
    }

    // Spell-check toggle.
    spell_action.set_state(&editor.spell_enabled.get().to_variant());
    menu.append(Some("Check Spelling"), Some("ctx.spell"));

    menu.append_section(None, &gio::Menu::new());
    menu.append(Some("Peek"), Some("ctx.peek"));

    editor.view.set_extra_menu(Some(&menu));
}
