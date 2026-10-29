//! The main document window: headerbar, editor area, bottom bar, search
//! bar and banners. One window holds exactly one document.

use adw::prelude::*;
use gtk::gio;
use gtk::glib;
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;

use crate::editor::Editor;
use crate::files::{self, recents, Document};
use crate::format;
use crate::settings::{settings, ColorScheme, PreviewMode};

pub struct AppWindow {
    pub root: adw::ApplicationWindow,
    headerbar: adw::HeaderBar,
    title_label: gtk::Label,
    subtitle_label: gtk::Label,
    pub editor: Editor,
    bottom_bar: gtk::Box,
    find_button: gtk::ToggleButton,
    layout_button: gtk::MenuButton,
    save_progress: gtk::Revealer,
    preview: crate::preview::PreviewPane,
    content_stack: gtk::Stack,
    editor_page: gtk::Box,
    hpaned: gtk::Paned,
    vpaned: gtk::Paned,
    editor_overlay_handle: gtk::Overlay,
    pub search: crate::search::SearchBar,
    stats_button: gtk::MenuButton,
    exit_fullscreen: gtk::Button,
    toolbar_view: adw::ToolbarView,
    /// Auto-hide wrappers for the header and bottom bars.
    headerbar_revealer: gtk::Revealer,
    bottom_revealer: gtk::Revealer,
    recovery_bar: gtk::InfoBar,
    file_changed_bar: gtk::InfoBar,
    file_changed_reload: gtk::Button,
    toast_overlay: adw::ToastOverlay,
    /// Shared mutable state; all clones of AppWindow see the same state.
    pub state: std::rc::Rc<WindowState>,
}

/// Per-window mutable state shared between all `AppWindow` clones.
pub struct WindowState {
    pub document: RefCell<Document>,
    /// Suppress dirty marking while the program replaces the buffer
    /// (load, reload).
    pub suppress_dirty: RefCell<bool>,
    highlight_generation: std::cell::Cell<u64>,
    last_layout_width: std::cell::Cell<i32>,
    pub preview_active: std::cell::Cell<bool>,
    preview_window: RefCell<Option<gtk::Window>>,
    preview_generation: std::cell::Cell<u64>,
    stats_generation: std::cell::Cell<u64>,
    spell_generation: std::cell::Cell<u64>,
    /// Bars currently faded out (auto-hide).
    pub bars_hidden: Cell<bool>,
    /// Height of the top bar while revealed (bar-shift compensation).
    bar_height: Cell<i32>,
    /// Cancels a running bar-compensation animation.
    comp_generation: Cell<u64>,
    /// A blocked-deletion shake animation is running.
    shaking: Cell<bool>,
    /// Recent blocked-deletion instants (Hemingway toast frequency rule).
    blocked_attempts: RefCell<VecDeque<std::time::Instant>>,
    pub monitor: RefCell<Option<gio::FileMonitor>>,
    pub snapshot_timer: RefCell<Option<glib::SourceId>>,
    pub snapshot_file: RefCell<Option<std::path::PathBuf>>,
}

impl Clone for AppWindow {
    fn clone(&self) -> Self {
        Self {
            root: self.root.clone(),
            headerbar: self.headerbar.clone(),
            title_label: self.title_label.clone(),
            subtitle_label: self.subtitle_label.clone(),
            editor: self.editor.clone(),
            bottom_bar: self.bottom_bar.clone(),
            find_button: self.find_button.clone(),
            layout_button: self.layout_button.clone(),
            save_progress: self.save_progress.clone(),
            content_stack: self.content_stack.clone(),
            editor_page: self.editor_page.clone(),
            hpaned: self.hpaned.clone(),
            vpaned: self.vpaned.clone(),
            editor_overlay_handle: self.editor_overlay_handle.clone(),
            search: self.search.clone(),
            stats_button: self.stats_button.clone(),
            exit_fullscreen: self.exit_fullscreen.clone(),
            toolbar_view: self.toolbar_view.clone(),
            headerbar_revealer: self.headerbar_revealer.clone(),
            bottom_revealer: self.bottom_revealer.clone(),
            recovery_bar: self.recovery_bar.clone(),
            file_changed_bar: self.file_changed_bar.clone(),
            file_changed_reload: self.file_changed_reload.clone(),
            toast_overlay: self.toast_overlay.clone(),
            state: self.state.clone(),
            preview: self.preview.clone(),
        }
    }
}

// NOTE: Clone lets closures hold window handles; the RefCell fields are NOT
// shared between clones — real per-document state must be kept in the
// widgets/actions of the original instance. `same_window` compares the
// underlying window object.
impl AppWindow {
    pub fn same_window(&self, other: &AppWindow) -> bool {
        self.root.as_ptr() == other.root.as_ptr()
    }

    pub fn new(app: &gtk::Application) -> Self {
        let root = adw::ApplicationWindow::new(app);
        root.set_default_size(
            settings().integer("window-width").max(360),
            settings().integer("window-height").max(240),
        );
        if settings().boolean("window-maximized") {
            root.maximize();
        }
        if settings().boolean("window-fullscreen") {
            root.fullscreen();
        }

        let editor = Editor::new();

        // --- headerbar -----------------------------------------------------
        let headerbar = adw::HeaderBar::new();

        let new_button = gtk::Button::new();
        new_button.set_icon_name("document-new-symbolic");
        new_button.set_tooltip_text(Some("New Window"));
        new_button.set_action_name(Some("app.new-window"));

        let open_button = gtk::MenuButton::new();
        open_button.set_icon_name("document-open-symbolic");
        open_button.set_tooltip_text(Some("Open"));

        let save_button = gtk::Button::new();
        save_button.set_label("Save");
        save_button.set_tooltip_text(Some("Save"));
        save_button.add_css_class("flat");
        save_button.set_action_name(Some("win.save"));
        let save_menu = gtk::MenuButton::new();
        save_menu.set_icon_name("pan-down-symbolic");
        save_menu.set_tooltip_text(Some("Save and Export"));
        save_menu.set_menu_model(Some(&save_menu_model()));
        let save_group = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        save_group.add_css_class("linked");
        save_group.append(&save_button);
        save_group.append(&save_menu);

        headerbar.pack_start(&new_button);
        headerbar.pack_start(&open_button);
        headerbar.pack_start(&save_group);

        let find_button = gtk::ToggleButton::new();
        find_button.set_icon_name("edit-find-symbolic");
        find_button.set_tooltip_text(Some("Find"));

        let layout_button = preview_layout_button();
        let exit_fullscreen = gtk::Button::from_icon_name("view-restore-symbolic");
        exit_fullscreen.set_tooltip_text(Some("Leave fullscreen"));
        exit_fullscreen.set_visible(false);
        {
            let root = root.clone();
            exit_fullscreen.connect_clicked(move |_| root.unfullscreen());
        }
        headerbar.pack_end(&main_menu_button());
        headerbar.pack_end(&find_button);
        headerbar.pack_end(&layout_button);
        headerbar.pack_end(&exit_fullscreen);

        let title_box = gtk::Box::new(gtk::Orientation::Vertical, 1);
        title_box.set_valign(gtk::Align::Center);
        let title_label = gtk::Label::new(Some("New File"));
        title_label.set_ellipsize(pango::EllipsizeMode::End);
        let subtitle_label = gtk::Label::new(None);
        subtitle_label.add_css_class("subtitle");
        subtitle_label.add_css_class("dim-label");
        subtitle_label.set_ellipsize(pango::EllipsizeMode::Start);
        title_box.append(&title_label);
        title_box.append(&subtitle_label);
        headerbar.set_title_widget(Some(&title_box));

        // --- save progress bar ----------------------------------------------
        let save_progress = gtk::Revealer::new();
        let progress = gtk::ProgressBar::new();
        progress.set_hexpand(true);
        save_progress.set_child(Some(&progress));
        save_progress.set_transition_type(gtk::RevealerTransitionType::SlideDown);
        save_progress.set_reveal_child(false);

        // --- auto-hide revealer for the header (~450 ms crossfade) ---------
        let headerbar_revealer = gtk::Revealer::new();
        headerbar_revealer.set_transition_type(gtk::RevealerTransitionType::Crossfade);
        headerbar_revealer.set_transition_duration(450);
        headerbar_revealer.set_child(Some(&headerbar));
        // Revealers start with the child unmapped until a reveal transition
        // runs; state it explicitly so the bars are visible from startup.
        headerbar_revealer.set_reveal_child(true);

        // --- bottom bar ----------------------------------------------------
        let bottom_bar = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        bottom_bar.add_css_class("bottom-bar");
        bottom_bar.set_halign(gtk::Align::Fill);
        bottom_bar.set_valign(gtk::Align::End);
        bottom_bar.set_margin_start(6);
        bottom_bar.set_margin_end(6);
        bottom_bar.set_margin_bottom(6);

        let toolbar_row = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        toolbar_row.add_css_class("toolbar-row");
        toolbar_row.set_hexpand(true);
        let toolbar_toggle = gtk::ToggleButton::new();
        toolbar_toggle.set_icon_name("pan-down-symbolic");
        toolbar_toggle.set_tooltip_text(Some("Formatting"));
        bottom_bar.append(&toolbar_row);
        bottom_bar.append(&toolbar_toggle);

        let stats_button = gtk::MenuButton::new();
        stats_button.set_label("0 Words");
        stats_button.add_css_class("flat");
        stats_button.set_halign(gtk::Align::End);
        stats_button.set_valign(gtk::Align::Center);
        bottom_bar.append(&stats_button);

        // --- auto-hide revealer for the bottom bar --------------------------
        let bottom_revealer = gtk::Revealer::new();
        bottom_revealer.set_transition_type(gtk::RevealerTransitionType::Crossfade);
        bottom_revealer.set_transition_duration(450);
        bottom_revealer.set_child(Some(&bottom_bar));
        bottom_revealer.set_reveal_child(true);
        bottom_revealer.set_halign(gtk::Align::Fill);
        bottom_revealer.set_valign(gtk::Align::End);

        // --- recovery & file-changed banners (infobars) ---------------------
        let recovery_bar = gtk::InfoBar::new();
        recovery_bar.set_message_type(gtk::MessageType::Info);
        recovery_bar.set_show_close_button(true);
        let recovery_label = gtk::Label::new(Some("This document was recovered from an autosave."));
        recovery_bar.add_child(&recovery_label);
        recovery_bar.set_revealed(false);

        let file_changed_bar = gtk::InfoBar::new();
        file_changed_bar.set_message_type(gtk::MessageType::Warning);
        file_changed_bar.set_show_close_button(true);
        let fc_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let fc_label = gtk::Label::new(Some("This file changed on disk."));
        fc_box.append(&fc_label);
        let file_changed_reload = gtk::Button::with_label("Reload");
        file_changed_reload.add_css_class("flat");
        fc_box.append(&file_changed_reload);
        file_changed_bar.add_child(&fc_box);
        file_changed_bar.set_revealed(false);

        // --- content -------------------------------------------------------
        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(&editor.scroll));
        overlay.add_overlay(&bottom_revealer);
        overlay.set_measure_overlay(&bottom_revealer, true);

        // Preview layouts: the editor page hosts the editor overlay; the
        // panes host it together with the preview pane. Widgets are moved
        // between parents on layout switches.
        let content_stack = gtk::Stack::new();
        content_stack.set_transition_type(gtk::StackTransitionType::Crossfade);
        content_stack.set_transition_duration(250);
        content_stack.set_hexpand(true);
        content_stack.set_vexpand(true);
        let editor_page = gtk::Box::new(gtk::Orientation::Vertical, 0);
        editor_page.append(&overlay);
        content_stack.add_named(&editor_page, Some("editor"));
        let hpaned = gtk::Paned::new(gtk::Orientation::Horizontal);
        let vpaned = gtk::Paned::new(gtk::Orientation::Vertical);
        content_stack.add_named(&hpaned, Some("half-width"));
        content_stack.add_named(&vpaned, Some("half-height"));

        let search = crate::search::SearchBar::new(&editor.view, &editor.buffer);

        let toolbar_view = adw::ToolbarView::new();
        toolbar_view.add_top_bar(&save_progress);
        toolbar_view.add_top_bar(&recovery_bar);
        toolbar_view.add_top_bar(&file_changed_bar);
        toolbar_view.add_top_bar(&headerbar_revealer);
        toolbar_view.add_top_bar(&search.bar);
        toolbar_view.set_content(Some(&content_stack));

        // Toasts surface here (blocked-deletion feedback and friends).
        let toast_overlay = adw::ToastOverlay::new();
        toast_overlay.set_child(Some(&toolbar_view));

        root.set_content(Some(&toast_overlay));

        let win = Self {
            root,
            headerbar,
            title_label,
            subtitle_label,
            editor,
            bottom_bar,
            find_button,
            layout_button,
            save_progress,
            content_stack,
            editor_page,
            hpaned,
            vpaned,
            editor_overlay_handle: overlay.clone(),
            search,
            stats_button: stats_button.clone(),
            headerbar_revealer,
            bottom_revealer,
            recovery_bar,
            file_changed_bar,
            file_changed_reload,
            exit_fullscreen,
            toolbar_view,
            toast_overlay,
            state: std::rc::Rc::new(WindowState {
                document: RefCell::new(Document::new()),
                suppress_dirty: RefCell::new(false),
                highlight_generation: std::cell::Cell::new(0),
                last_layout_width: std::cell::Cell::new(0),
                preview_active: std::cell::Cell::new(settings().boolean("preview-active")),
                preview_window: RefCell::new(None),
                preview_generation: std::cell::Cell::new(0),
                stats_generation: std::cell::Cell::new(0),
                spell_generation: std::cell::Cell::new(0),
                bars_hidden: Cell::new(false),
                bar_height: Cell::new(0),
                comp_generation: Cell::new(0),
                shaking: Cell::new(false),
                blocked_attempts: RefCell::new(VecDeque::new()),
                monitor: RefCell::new(None),
                snapshot_timer: RefCell::new(None),
                snapshot_file: RefCell::new(None),
            }),
            preview: crate::preview::PreviewPane::new(),
        };
        // Hemingway Mode startup state comes from the preference (03/04 §2).
        win.editor
            .flags
            .hemingway
            .set(settings().boolean("hemingway-mode"));
        // Blocked-deletion feedback: shake + toast.
        {
            let win2 = win.clone();
            win.editor
                .blocked_callback
                .borrow_mut()
                .replace(Box::new(move || win2.on_delete_blocked()));
        }

        // Find toggle button ↔ search bar visibility.
        {
            let win2 = win.clone();
            let find_btn = win.find_button.clone();
            find_btn.connect_toggled(move |t| {
                if t.is_active() {
                    win2.search.open(false, &win2.editor.buffer);
                } else {
                    win2.search.close();
                }
            });
            let fb = win.find_button.clone();
            let win3 = win.clone();
            win.search.bar.connect_notify_local(
                Some("search-mode-enabled"),
                move |bar, _pspec| {
                    let visible = bar.is_search_mode();
                    if fb.is_active() != visible {
                        fb.set_active(visible);
                    }
                    // The search bar being visible reveals the bars
                    // (03/13 §2) and keeps them revealed.
                    if visible {
                        win3.reveal_bars();
                    }
                },
            );
        }

        // Open button shows the recents popover.
        open_button.set_popover(Some(&recents::build_popover({
            let win = win.clone();
            move || files::open_dialog(&win)
        })));

        build_toolbar(&win, &toolbar_row, &toolbar_toggle);

        win.install_actions();
        win.connect_signals();
        win.start_layout_polling();
        crate::app::register_window(win.clone());
        win
    }

    /// Adaptive column layout: re-run when the window size changes. GTK4
    /// has no size-allocate signal, so poll cheaply and skip no-ops.
    fn start_layout_polling(&self) {
        let win = self.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(200), move || {
            let width = win.root.width();
            if width > 0 && width != win.state.last_layout_width.get() {
                win.state.last_layout_width.set(width);
                win.editor.relayout_column(
                    width,
                    settings().integer("characters-per-line"),
                    settings().boolean("bigger-text"),
                );
                // Keep Focus Mode margins at half the viewport.
                win.editor.apply_focus_margins();
            }
            // Persist window geometry (`05-file-formats-and-persistence.md`).
            if win.root.width() > 0 {
                settings().set_integer("window-width", win.root.width());
                settings().set_integer("window-height", win.root.height());
                settings().set_boolean("window-maximized", win.root.is_maximized());
                settings().set_boolean("window-fullscreen", win.root.is_fullscreen());
            }
            glib::ControlFlow::Continue
        });
    }

    /// Debounced highlighting rescan (~150 ms after the last edit).
    fn schedule_highlight(&self) {
        let generation = self.state.highlight_generation.get() + 1;
        self.state.highlight_generation.set(generation);
        let win = self.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(150), move || {
            if win.state.highlight_generation.get() == generation {
                win.editor.highlight_now();
            }
            glib::ControlFlow::Break
        });
    }

    /// Debounced spell rescan (~250 ms after the last edit).
    fn schedule_spell(&self) {
        let generation = self.state.spell_generation.get() + 1;
        self.state.spell_generation.set(generation);
        let win = self.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(250), move || {
            if win.state.spell_generation.get() == generation {
                win.editor.spell_now();
            }
            glib::ControlFlow::Break
        });
    }

    fn install_actions(&self) {
        let actions = gio::SimpleActionGroup::new();
        let win_ref = self.clone();
        let win_ref2 = self.clone();

        let scheme = gio::SimpleAction::new_stateful(
            "color-scheme",
            Some(glib::VariantTy::STRING),
            &settings().string("color-scheme").to_variant(),
        );
        {
            let scheme_state = scheme.clone();
            scheme.connect_activate(move |_, param| {
                let v = param
                    .and_then(|p| p.str().map(str::to_owned))
                    .unwrap_or_else(|| "system".into());
                let scheme_v = ColorScheme::parse(&v).unwrap_or(ColorScheme::System);
                settings().set_color_scheme(scheme_v);
                scheme_state.set_state(&v.to_variant());
                crate::appearance::apply();
            });
        }
        actions.add_action(&scheme);

        let preview_mode = gio::SimpleAction::new_stateful(
            "preview-mode",
            Some(glib::VariantTy::STRING),
            &settings().string("preview-mode").to_variant(),
        );
        {
            let preview_mode_state = preview_mode.clone();
            preview_mode.connect_activate(move |_, param| {
                let v = param
                    .and_then(|p| p.str().map(str::to_owned))
                    .unwrap_or_else(|| "full-width".into());
                if let Some(mode) = PreviewMode::parse(&v) {
                    settings().set_preview_mode(mode);
                    preview_mode_state.set_state(&v.to_variant());
                    // Re-arrange immediately when the preview is visible.
                    let win2 = win_ref.clone();
                    if win2.state.preview_active.get() {
                        win2.apply_preview_mode(mode);
                    }
                }
            });
        }
        actions.add_action(&preview_mode);

        macro_rules! bind {
            ($name:expr, $f:expr) => {{
                let action = gio::SimpleAction::new($name, None);
                let win = self.clone();
                action.connect_activate(move |_, _| ($f)(&win));
                actions.add_action(&action);
            }};
        }

        bind!("open", files::open_dialog);
        bind!("save", AppWindow::save);
        bind!("save-as", files::save_as_dialog);

        // Formatting commands (`04-keyboard-shortcuts.md`).
        bind!("format-bold", |w: &AppWindow| format::wrap(&w.editor.buffer, "**", "bold text"));
        bind!("format-italic", |w: &AppWindow| format::wrap(&w.editor.buffer, "_", "italic text"));
        bind!("format-strike", |w: &AppWindow| format::wrap(&w.editor.buffer, "~~", "strikethrough text"));
        bind!("format-code", |w: &AppWindow| format::code(&w.editor.buffer));
        bind!("format-heading", |w: &AppWindow| format::heading(&w.editor.buffer, None));
        for n in 1..=6u8 {
            let action = gio::SimpleAction::new(&format!("format-heading-{n}"), None);
            let win = self.clone();
            action.connect_activate(move |_, _| format::heading(&win.editor.buffer, Some(n)));
            actions.add_action(&action);
        }
        bind!("format-bullet", |w: &AppWindow| format::list(&w.editor.buffer, format::ListCommand::Bullet));
        bind!("format-numbered", |w: &AppWindow| format::list(&w.editor.buffer, format::ListCommand::Numbered));
        bind!("format-checklist", |w: &AppWindow| format::list(&w.editor.buffer, format::ListCommand::Checklist));
        bind!("format-quote", |w: &AppWindow| format::block_quote(&w.editor.buffer));
        bind!("format-link", |w: &AppWindow| format::link(&w.editor.buffer));
        bind!("format-image", |w: &AppWindow| {
            let dir = w.editor.flags.doc_dir.borrow().clone();
            format::image(&w.editor.buffer, dir, &w.root);
        });
        bind!("format-rule", |w: &AppWindow| format::horizontal_rule(&w.editor.buffer));
        bind!("format-fix-table", |w: &AppWindow| {
            if !format::fix_table(&w.editor.buffer) {
                w.show_error("No table", "The cursor is not inside a table.");
            }
        });
        bind!("toggle-preview", AppWindow::toggle_preview);
        bind!("export-pdf", crate::export::quick_pdf);
        bind!("export-html", crate::export::quick_html);
        bind!("export-odt", crate::export::quick_odt);
        bind!("export-advanced", crate::export::advanced_dialog);
        bind!("copy-html", crate::export::copy_html);

        // Fullscreen (F11 or the exit-fullscreen control, 03/14 §3).
        bind!("toggle-fullscreen", |w: &AppWindow| {
            if w.root.is_fullscreen() {
                w.root.unfullscreen();
            } else {
                w.root.fullscreen();
            }
        });
        bind!("preview-close", AppWindow::preview_off);
        bind!("find", |w: &AppWindow| w.search.open(false, &w.editor.buffer));
        bind!("find-replace", |w: &AppWindow| w.search.open(true, &w.editor.buffer));

        // Default statistic selection (persisted, F2).
        let statistic = gio::SimpleAction::new_stateful(
            "statistic",
            Some(glib::VariantTy::STRING),
            &settings().string("default-statistic").to_variant(),
        );
        {
            let statistic_state = statistic.clone();
            statistic.connect_activate(move |_, param| {
                let v = param
                    .and_then(|p| p.str().map(str::to_owned))
                    .unwrap_or_else(|| "words".into());
                if crate::markdown::stats::Statistic::parse_name(&v).is_some() {
                    settings().set_string("default-statistic", &v);
                    statistic_state.set_state(&v.to_variant());
                }
                let win2 = win_ref2.clone();
                win2.update_stats_now();
            });
        }
        actions.add_action(&statistic);
        {
            let menu = gio::Menu::new();
            for st in [crate::markdown::stats::Statistic::Characters, crate::markdown::stats::Statistic::Words, crate::markdown::stats::Statistic::Sentences, crate::markdown::stats::Statistic::Paragraphs, crate::markdown::stats::Statistic::ReadTime] {
                menu.append(
                    Some(st.display_name()),
                    Some(&format!("win.statistic::{}", st.as_name())),
                );
            }
            self.stats_button.set_menu_model(Some(&menu));
            self.stats_button.set_always_show_arrow(false);
        }
        self.update_stats_now();

        let close = gio::SimpleAction::new("close", None);
        {
            let root = self.root.clone();
            close.connect_activate(move |_, _| {
                root.close();
            });
        }
        actions.add_action(&close);

        // `find`, `find-replace`, `toggle-preview` are already bound above.

        let preferences = gio::SimpleAction::new("preferences", None);
        {
            let root = self.root.clone();
            preferences.connect_activate(move |_, _| {
                crate::dialogs::show_preferences(&root);
            });
        }
        actions.add_action(&preferences);

        let shortcuts = gio::SimpleAction::new("show-shortcuts", None);
        {
            let root = self.root.clone();
            shortcuts.connect_activate(move |_, _| {
                crate::dialogs::show_shortcuts(&root);
            });
        }
        actions.add_action(&shortcuts);

        // Focus Mode (03/04 §1): dim + typewriter scrolling; the state
        // is not persisted.
        let focus_mode =
            gio::SimpleAction::new_stateful("focus-mode", None, &false.to_variant());
        {
            let win_ref = self.clone();
            focus_mode.connect_activate(move |action, _| {
                let new = !action
                    .state()
                    .and_then(|v| v.get::<bool>())
                    .unwrap_or(false);
                action.set_state(&new.to_variant());
                win_ref.editor.set_focus_mode(new);
                // Focus Mode temporarily disables spell checking (§4).
                win_ref.editor.refresh_spell();
            });
        }
        actions.add_action(&focus_mode);

        // Hemingway Mode (03/04 §2): deletions blocked; the state is
        // remembered across sessions.
        let hemingway = gio::SimpleAction::new_stateful(
            "hemingway-mode",
            None,
            &settings().boolean("hemingway-mode").to_variant(),
        );
        {
            let win_ref = self.clone();
            hemingway.connect_activate(move |action, _| {
                let new = !action
                    .state()
                    .and_then(|v| v.get::<bool>())
                    .unwrap_or(false);
                action.set_state(&new.to_variant());
                settings().set_boolean("hemingway-mode", new);
                win_ref.editor.flags.hemingway.set(new);
            });
        }
        actions.add_action(&hemingway);

        let about = gio::SimpleAction::new("about", None);
        {
            let root = self.root.clone();
            about.connect_activate(move |_, _| {
                crate::dialogs::show_about(&root);
            });
        }
        actions.add_action(&about);

        // Spell-check toggle (F7 and the context-menu checkbox): the
        // preference is global; every window's editor follows it.
        let toggle_spell = gio::SimpleAction::new_stateful(
            "toggle-spellcheck",
            None,
            &settings().boolean("spellcheck").to_variant(),
        );
        {
            toggle_spell.connect_activate(move |_, _| {
                let cur = settings().boolean("spellcheck");
                let new = !cur;
                settings().set_boolean("spellcheck", new);
                for w in crate::app::window_registry() {
                    if let Some(a) = w
                        .root
                        .lookup_action("toggle-spellcheck")
                        .and_then(|o| o.downcast::<gio::SimpleAction>().ok())
                    {
                        a.set_state(&new.to_variant());
                    }
                }
            });
        }
        actions.add_action(&toggle_spell);

        self.root.insert_action_group("win", Some(&actions));
    }

    fn connect_signals(&self) {
        // Dirty tracking: any buffer change marks the document dirty
        // (suppressed while the program replaces content).
        {
            let win = self.clone();
            self.editor
                .buffer
                .connect_changed(move |_| {
                    win.set_dirty_from_edit();
                    win.schedule_highlight();
                    win.schedule_spell();
                    win.schedule_preview();
                    win.maybe_autohide_bars();
                    win.editor.focus_update();
                });
        }

        // Statistics update on selection change.
        {
            let win = self.clone();
            self.editor
                .buffer
                .connect_mark_set(move |_buf, _location, _mark| {
                    win.schedule_stats();
                });
        }

        // Scroll synchronization (editor → preview).
        {
            let win = self.clone();
            self.editor.scroll.vadjustment().connect_value_changed(move |_| {
                let win = &win;
                if win.state.preview_active.get() {
                    let f = win.editor.scroll_fraction();
                    win.preview.sync_from_editor(f);
                }
            });
        }

        // File-changed reload button and banner dismissal.
        {
            let win = self.clone();
            self.file_changed_reload.connect_clicked(move |_| {
                win.reload_from_disk();
            });
            let bar = self.file_changed_bar.clone();
            let bar2 = bar.clone();
            bar.connect_response(move |_b, _| bar2.set_revealed(false));
            let rbar = self.recovery_bar.clone();
            let rbar2 = rbar.clone();
            rbar.connect_response(move |_b, _| rbar2.set_revealed(false));
        }

        // Auto-hide reveal triggers (03/13 §2): pointer entering the top
        // region of the window, and fullscreen exit.
        {
            let win = self.clone();
            let motion = gtk::EventControllerMotion::new();
            motion.connect_motion(move |_c, _x, y| {
                if win.state.bars_hidden.get() && y < 56.0 {
                    win.reveal_bars();
                }
            });
            self.root.add_controller(motion);
        }
        {
            let win = self.clone();
            self.root.connect_notify_local(Some("fullscreened"), move |w, _| {
                win.exit_fullscreen.set_visible(w.is_fullscreen());
                if w.is_fullscreen() {
                    win.hide_bars_for_fullscreen();
                } else {
                    win.reveal_bars();
                    win.editor.view.grab_focus();
                }
            });
        }

        // Auto-hide reveal: the editor losing keyboard focus (03/13 §2).
        {
            let win = self.clone();
            let focus_ctl = gtk::EventControllerFocus::new();
            focus_ctl.connect_leave(move |_| win.reveal_bars());
            self.editor.view.add_controller(focus_ctl);
        }

        // Scroll synchronization (preview → editor) via injected script.
        {
            let win = self.clone();
            self.preview.install_scroll_script(move |f| {
                let win = &win;
                if win.state.preview_active.get() {
                    win.preview.sync_from_preview(f, |v| win.editor.set_scroll_fraction(v));
                }
            });
        }

        // Warning page choice wiring.
        {
            let win = self.clone();
            self.preview.setup_warning_page(std::rc::Rc::new(move |level| {
                let win = &win;
                win.preview.choose_level(level);
                win.schedule_preview();
            }));
        }

        // Window close: unsaved-changes guard, then unregister. When the
        // last window closes the application exits (open question 3
        // decision: exit with zero windows).
        {
            let win = self.clone();
            self.root.connect_close_request(move |window| {
                let win = win.clone();
                let window = window.clone();
                let app = window.application();
                glib::spawn_future_local(async move {
                    if win.confirm_close().await {
                        win.cancel_snapshot_timer();
                        if let Some(m) = win.state.monitor.borrow_mut().take() {
                            m.cancel();
                        }
                        crate::app::unregister_window(&win);
                        files::delete_snapshot(&win);
                        window.destroy();
                        if let Some(app) = app {
                            if app.windows().is_empty() {
                                app.quit();
                            }
                        }
                    }
                });
                glib::Propagation::Stop
            });
        }
    }

    fn set_dirty_from_edit(&self) {
        if *self.state.suppress_dirty.borrow() {
            return;
        }
        if !self.state.document.borrow().dirty {
            self.state.document.borrow_mut().dirty = true;
            self.update_title();
            files::schedule_snapshot(self);
            crate::app::update_logout_inhibition();
        }
    }

    pub fn set_document_dirty(&self, dirty: bool) {
        self.state.document.borrow_mut().dirty = dirty;
        self.update_title();
        crate::app::update_logout_inhibition();
    }

    pub fn update_title(&self) {
        let doc = self.state.document.borrow();
        let name = doc.display_name();
        let marker = if doc.dirty { "• " } else { "" };
        self.title_label.set_text(&format!("{marker}{name}"));
        let parent = doc.parent_path().unwrap_or_default();
        self.subtitle_label.set_text(&parent);
        let path = doc.path_string();
        self.root.set_tooltip_text(path.as_deref());
    }

    pub fn reload_from_disk(&self) {
        self.file_changed_bar.set_revealed(false);
        if let Some(file) = self.state.document.borrow().file.clone() {
            self.load_file(&file);
        }
        self.file_changed_bar.set_revealed(false);
    }

    pub fn show_file_changed(&self) {
        self.file_changed_bar.set_revealed(true);
    }

    pub fn show_recovery_banner(&self) {
        self.recovery_bar.set_revealed(true);
    }

    pub fn cancel_snapshot_timer(&self) {
        if let Some(id) = self.state.snapshot_timer.borrow_mut().take() {
            id.remove();
        }
    }

    /// Replace the editor content and document state after a successful
    /// load (§2.2).
    pub fn apply_loaded(&self, file: &gio::File, decoded: &files::encoding::Decoded) {
        *self.state.suppress_dirty.borrow_mut() = true;
        self.editor.set_full_text(&decoded.text);
        *self.state.suppress_dirty.borrow_mut() = false;
        {
            let mut doc = self.state.document.borrow_mut();
            doc.file = Some(file.clone());
            doc.encoding = decoded.encoding;
            doc.dirty = false;
            doc.recovered = false;
        }
        self.update_title();
        recents::add(file);
        files::monitor::install(self, file);
        *self.editor.flags.doc_dir.borrow_mut() =
            file.parent().and_then(|p| p.path());
        // Per-file spell language (`05-…persistence.md` §4).
        *self.editor.spell_persist_target.borrow_mut() = Some(file.clone());
        let lang = files::metadata::load_language(file);
        self.editor.set_spell_language(lang);
        self.editor.highlight_now();
        if self.state.preview_active.get() {
            self.preview.evaluate_security(&decoded.text, false);
            self.schedule_preview();
        }
        self.editor.view.grab_focus();
    }

    /// Internal resource (tutorial): loads text, not added to recents,
    /// starts unsaved (`03-behavior/12-windows.md` §3).
    pub fn load_internal_text(&self, text: &str) {
        *self.state.suppress_dirty.borrow_mut() = true;
        self.editor.set_full_text(text);
        *self.state.suppress_dirty.borrow_mut() = false;
        {
            let mut doc = self.state.document.borrow_mut();
            doc.file = None;
            doc.dirty = true;
        }
        self.update_title();
        *self.editor.spell_persist_target.borrow_mut() = None;
        self.editor.set_spell_language(None);
        self.editor.highlight_now();
    }

    /// Load a file into this window (used by routing).
    pub fn load_file(&self, file: &gio::File) {
        if let Err(e) = files::load_into(self, file) {
            self.show_error("Cannot open file", &e);
            self.reset_to_new_document();
        }
    }

    /// Reset the window to a new empty, clean document (load-failure path).
    fn reset_to_new_document(&self) {
        *self.state.suppress_dirty.borrow_mut() = true;
        self.editor.set_full_text("");
        *self.state.suppress_dirty.borrow_mut() = false;
        {
            let mut doc = self.state.document.borrow_mut();
            doc.file = None;
            doc.encoding = "UTF-8";
            doc.dirty = false;
            doc.recovered = false;
        }
        *self.editor.flags.doc_dir.borrow_mut() = None;
        *self.editor.spell_persist_target.borrow_mut() = None;
        self.editor.set_spell_language(None);
        self.update_title();
    }

    /// Save: to the backing file, or via Save As when there is none.
    pub fn save(&self) {
        let file = self.state.document.borrow().file.clone();
        match file {
            Some(file) => self.save_to(file),
            None => files::save_as_dialog(self),
        }
    }

    /// Write to `file`, associate it as the backing file, clear dirty,
    /// register recents, drop the recovery snapshot.
    pub fn save_to(&self, file: gio::File) {
        // A save starting reveals the auto-hidden bars (03/13 §2).
        self.reveal_bars();
        files::monitor::pause(self);
        self.save_progress.set_reveal_child(true);
        let text = self.editor.full_text();
        let encoding = self.state.document.borrow().encoding.to_owned();
        let result = files::write_to(&file, &text, &encoding);
        self.save_progress.set_reveal_child(false);

        match result {
            Ok(effective) => {
                {
                    let mut doc = self.state.document.borrow_mut();
                    doc.file = Some(file.clone());
                    doc.encoding = Box::leak(effective.into_boxed_str());
                    doc.dirty = false;
                }
                *self.editor.flags.doc_dir.borrow_mut() =
                    file.parent().and_then(|p| p.path());
                // A session-chosen spell language persists on first save.
                if let Some(lang) = self.editor.spell_language.borrow().clone() {
                    files::metadata::store_language(&file, &lang);
                }
                *self.editor.spell_persist_target.borrow_mut() = Some(file.clone());
                self.update_title();
                recents::add(&file);
                files::delete_snapshot(self);
                self.cancel_snapshot_timer();
                // Re-arm the file monitor after a grace delay so our own
                // write does not trigger the external-change banner.
                {
                    let win = self.clone();
                    let f = file.clone();
                    glib::timeout_add_seconds_local(1, move || {
                        files::monitor::install(&win, &f);
                        glib::ControlFlow::Break
                    });
                }
                crate::app::update_logout_inhibition();
            }
            Err(e) => {
                self.show_error("Cannot save file", &e);
                // Dirty state stays set; the progress bar already faded.
            }
        }
    }

    /// Unsaved-changes confirmation (`02-ui-layout.md` §11). Returns true
    /// when the window may close.
    pub async fn confirm_close(&self) -> bool {
        if !self.state.document.borrow().dirty {
            return true;
        }
        let dialog = adw::MessageDialog::new(
            Some(&self.root),
            Some("Save changes?"),
            Some(&format!(
                "“{}” contains unsaved changes. If you close it, the changes will be lost.",
                self.state.document.borrow().display_name()
            )),
        );
        dialog.add_response("cancel", "Cancel");
        dialog.set_close_response("cancel");
        dialog.add_response("discard", "Discard");
        dialog.set_response_appearance("discard", adw::ResponseAppearance::Destructive);
        dialog.add_response("save", "Save");
        dialog.set_response_appearance("save", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("save"));

        match dialog.choose_future().await.as_str() {
            "save" => {
                self.save();
                // A failed save aborts the close (dirty stays set).
                !self.state.document.borrow().dirty
            }
            "discard" => true,
            _ => false,
        }
    }

    pub fn show_error(&self, title: &str, body: &str) {
        let dialog = adw::AlertDialog::new(Some(title), Some(body));
        dialog.add_response("close", "Close");
        dialog.set_default_response(Some("close"));
        dialog.present(Some(&self.root));
    }

    pub fn present_window(&self) {
        self.root.present();
        if self.state.preview_active.get() {
            self.preview_on();
        }
        self.editor.view.grab_focus();
    }
}

fn save_menu_model() -> gio::Menu {
    let menu = gio::Menu::new();
    menu.append(Some("Save As…"), Some("win.save-as"));
    let exports = gio::Menu::new();
    exports.append(Some("Export as PDF"), Some("win.export-pdf"));
    exports.append(Some("Export as HTML"), Some("win.export-html"));
    exports.append(Some("Export as ODT"), Some("win.export-odt"));
    exports.append(Some("Copy HTML"), Some("win.copy-html"));
    menu.append(Some("Advanced Export…"), Some("win.export-advanced"));    menu.append_section(Some("Export"), &exports);
    menu
}

fn preview_layout_button() -> gtk::MenuButton {
    let menu = gio::Menu::new();
    let section = gio::Menu::new();
    for mode in PreviewMode::ALL {
        section.append(
            Some(match mode {
                PreviewMode::FullWidth => "Preview Instead of Editor",
                PreviewMode::HalfWidth => "Preview Beside the Editor",
                PreviewMode::HalfHeight => "Preview Below the Editor",
                PreviewMode::Windowed => "Preview in Separate Window",
            }),
            Some(&format!("win.preview-mode::{}", mode.as_str())),
        );
    }
    menu.append_section(None, &section);

    let button = gtk::MenuButton::new();
    button.set_icon_name("view-right-pane-symbolic");
    button.set_tooltip_text(Some("Preview Layout"));
    button.set_menu_model(Some(&menu));
    button
}

fn main_menu_button() -> gtk::MenuButton {
    let menu = gio::Menu::new();

    let appearance = gio::Menu::new();
    for scheme in ColorScheme::ALL {
        let label = match scheme {
            ColorScheme::System => "System",
            ColorScheme::Light => "Light",
            ColorScheme::Sepia => "Sepia",
            ColorScheme::Dark => "Dark",
        };
        appearance.append(
            Some(label),
            Some(&format!("win.color-scheme::{}", scheme.as_str())),
        );
    }
    menu.append_section(Some("Appearance"), &appearance);

    let modes = gio::Menu::new();
    modes.append(Some("Hemingway Mode"), Some("win.hemingway-mode"));
    modes.append(Some("Focus Mode"), Some("win.focus-mode"));
    menu.append_section(None, &modes);

    let actions = gio::Menu::new();
    actions.append(Some("Find and Replace"), Some("win.find-replace"));
    actions.append(Some("Preferences"), Some("win.preferences"));
    menu.append_section(None, &actions);

    let help = gio::Menu::new();
    help.append(Some("Open Tutorial"), Some("app.open-tutorial"));
    help.append(Some("Keyboard Shortcuts"), Some("win.show-shortcuts"));
    help.append(Some("About Griffel"), Some("win.about"));
    menu.append_section(None, &help);

    let button = gtk::MenuButton::new();
    button.set_icon_name("open-menu-symbolic");
    button.set_tooltip_text(Some("Main Menu"));
    button.set_menu_model(Some(&menu));
    button
}



// ---------------------------------------------------------------------------
// Formatting toolbar (`02-ui-layout.md` §5)
// ---------------------------------------------------------------------------

fn tool_button(win: &AppWindow, row: &gtk::Box, icon_or_label: &str, tooltip: &str, f: impl Fn(&AppWindow) + 'static) {
    let button = if icon_or_label.starts_with("<") || icon_or_label.starts_with("❝") || icon_or_label.starts_with("▦") || icon_or_label.starts_with('H') {
        let b = gtk::Button::with_label(icon_or_label);
        b.add_css_class("flat");
        b
    } else {
        let b = gtk::Button::from_icon_name(icon_or_label);
        b.add_css_class("flat");
        b
    };
    button.set_tooltip_text(Some(tooltip));
    let win = win.clone();
    button.connect_clicked(move |_| f(&win));
    row.append(&button);
}

fn build_toolbar(win: &AppWindow, row: &gtk::Box, toggle: &gtk::ToggleButton) {
    let buffer = win.editor.buffer.clone();

    tool_button(win, row, "format-text-bold-symbolic", "Bold", |w| {
        format::wrap(&w.editor.buffer, "**", "bold text")
    });
    tool_button(win, row, "format-text-italic-symbolic", "Italic", |w| {
        format::wrap(&w.editor.buffer, "_", "italic text")
    });
    tool_button(win, row, "format-text-strikethrough-symbolic", "Strikethrough", |w| {
        format::wrap(&w.editor.buffer, "~~", "strikethrough text")
    });
    tool_button(win, row, "H", "Heading", |w| {
        format::heading(&w.editor.buffer, None)
    });
    tool_button(win, row, "view-list-bullet-symbolic", "Bulleted List", |w| {
        format::list(&w.editor.buffer, format::ListCommand::Bullet)
    });
    tool_button(win, row, "view-list-ordered-symbolic", "Numbered List", |w| {
        format::list(&w.editor.buffer, format::ListCommand::Numbered)
    });
    tool_button(win, row, "checkbox-symbolic", "Checklist", |w| {
        format::list(&w.editor.buffer, format::ListCommand::Checklist)
    });
    tool_button(win, row, "❝", "Block Quote", |w| {
        format::block_quote(&w.editor.buffer)
    });
    tool_button(win, row, "</>", "Code", |w| {
        format::code(&w.editor.buffer)
    });
    tool_button(win, row, "insert-link-symbolic", "Link", |w| {
        format::link(&w.editor.buffer)
    });
    tool_button(win, row, "insert-image-symbolic", "Image", |w| {
        let dir = w.editor.flags.doc_dir.borrow().clone();
        format::image(&w.editor.buffer, dir, &w.root);
    });
    tool_button(win, row, "▦", "Table", |w| {
        // The table button's popover is attached to this button; open it.
    });
    // Horizontal rule and fix-table trailing buttons.
    tool_button(win, row, "configure-line-symbolic", "Horizontal Rule", |w| {
        format::horizontal_rule(&w.editor.buffer)
    });
    tool_button(win, row, "view-grid-symbolic", "Fix Table Alignment", |w| {
        if !format::fix_table(&w.editor.buffer) {
            w.show_error("No table", "The cursor is not inside a table.");
        }
    });

    // The table button opens the size picker popover.
    let table_button = gtk::MenuButton::new();
    table_button.set_label("▦");
    table_button.set_tooltip_text(Some("Table"));
    table_button.set_popover(Some(&table_size_popover(&buffer)));
    table_button.add_css_class("flat");
    row.append(&table_button);

    // Chevron toggle: show/hide the row; state persisted.
    toggle.set_active(settings().boolean("toolbar-visible"));
    row.set_visible(toggle.is_active());
    {
        let row = row.clone();
        toggle.connect_toggled(move |t| {
            row.set_visible(t.is_active());
            settings().set_boolean("toolbar-visible", t.is_active());
        });
    }
}

/// Size picker (up to 4 rows × 6 columns); hovering highlights the
/// selected rows×columns region and a label shows "R x C"
/// (`02-formatting.md` §7).
fn table_size_popover(buffer: &sourceview5::Buffer) -> gtk::Popover {
    const MAX_ROWS: usize = 4;
    const MAX_COLS: usize = 6;
    let popover = gtk::Popover::new();
    let box_ = gtk::Box::new(gtk::Orientation::Vertical, 6);
    box_.set_margin_top(8);
    box_.set_margin_bottom(8);
    box_.set_margin_start(8);
    box_.set_margin_end(8);

    let label = gtk::Label::new(Some("Insert Table"));
    label.add_css_class("dim-label");
    box_.append(&label);

    let grid = gtk::Grid::new();
    grid.set_row_spacing(2);
    grid.set_column_spacing(2);

    let mut cells: Vec<Vec<gtk::Button>> = Vec::new();
    for r in 1..=MAX_ROWS {
        let mut row = Vec::new();
        for c in 1..=MAX_COLS {
            let cell = gtk::Button::new();
            cell.set_size_request(20, 20);
            cell.add_css_class("flat");
            let buffer = buffer.clone();
            let popover = popover.clone();
            let (rr, cc) = (r, c);
            cell.connect_clicked(move |_| {
                popover.popdown();
                format::insert_table(
                    &buffer,
                    rr,
                    cc,
                    settings().integer("characters-per-line"),
                );
            });
            grid.attach(&cell, c as i32 - 1, r as i32 - 1, 1, 1);
            row.push(cell);
        }
        cells.push(row);
    }
    // Hover highlights the rows×columns region and the label shows "R x C".
    for (r, row) in cells.iter().enumerate() {
        for (c, cell) in row.iter().enumerate() {
            let cells = cells.clone();
            let label = label.clone();
            let motion = gtk::EventControllerMotion::new();
            motion.connect_enter(move |_, _x, _y| {
                for (ri, hrow) in cells.iter().enumerate() {
                    for (ci, b) in hrow.iter().enumerate() {
                        if ri <= r && ci <= c {
                            b.add_css_class("suggested-action");
                        } else {
                            b.remove_css_class("suggested-action");
                        }
                    }
                }
                label.set_text(&format!("{} x {}", r + 1, c + 1));
            });
            cell.add_controller(motion);
        }
    }
    {
        let cells = cells.clone();
        let label = label.clone();
        let leave = gtk::EventControllerMotion::new();
        leave.connect_leave(move |_| {
            for row in cells.iter() {
                for b in row.iter() {
                    b.remove_css_class("suggested-action");
                }
            }
            label.set_text("Insert Table");
        });
        grid.add_controller(leave);
    }
    box_.append(&grid);
    popover.set_child(Some(&box_));
    popover
}

// ---------------------------------------------------------------------------
// Preview integration (`03-behavior/03-live-preview.md` §2)
// ---------------------------------------------------------------------------

impl AppWindow {
    /// Toggle the preview on/off (Ctrl+P).
    pub fn toggle_preview(&self) {
        if self.state.preview_active.get() {
            self.preview_off();
        } else {
            self.preview_on();
        }
    }

    pub fn preview_on(&self) {
        // Evaluate the document's security level for the current text.
        let text = self.editor.full_text();
        let is_new = self.state.document.borrow().file.is_none();
        self.preview.evaluate_security(&text, is_new);
        self.state.preview_active.set(true);
        self.apply_preview_mode(settings().preview_mode());
        self.schedule_preview();
    }

    pub fn preview_off(&self) {
        self.state.preview_active.set(false);
        self.detach_preview_window();
        self.detach(&self.preview.root);
        self.show_editor_page();
    }

    fn show_editor_page(&self) {
        self.detach(&self.editor_overlay_handle);
        self.editor_page.append(&self.editor_overlay_handle);
        self.content_stack.set_visible_child_name("editor");
    }

    fn detach_preview_window(&self) {
        if let Some(win) = self.state.preview_window.borrow_mut().take() {
            win.close();
        }
    }

    fn detach(&self, w: &impl IsA<gtk::Widget>) {
        if let Some(parent) = w.parent() {
            if let Some(b) = parent.downcast_ref::<gtk::Box>() {
                b.remove(w);
            } else if let Some(s) = parent.downcast_ref::<gtk::Stack>() {
                s.remove(w);
            } else if let Some(pane) = parent.downcast_ref::<gtk::Paned>() {
                let same = |c: Option<gtk::Widget>| {
                    c.map(|c| c.as_ptr() == w.upcast_ref::<gtk::Widget>().as_ptr()).unwrap_or(false)
                };
                if same(pane.start_child()) {
                    pane.set_start_child(None::<&gtk::Widget>);
                } else if same(pane.end_child()) {
                    pane.set_end_child(None::<&gtk::Widget>);
                }
            } else if let Some(win) = parent.downcast_ref::<gtk::Window>() {
                win.set_child(None::<&gtk::Widget>);
            }
        }
    }

    /// Arrange editor and preview per `mode` (§2, `02-ui-layout.md` §4).
    pub fn apply_preview_mode(&self, mode: PreviewMode) {
        self.detach(&self.editor_overlay_handle);
        self.detach(&self.preview.root);
        self.detach_preview_window();

        let width = self.root.width().max(600);
        let height = self.root.height().max(400);
        match mode {
            PreviewMode::HalfWidth => {
                self.hpaned.set_start_child(Some(&self.editor_overlay_handle));
                self.hpaned.set_end_child(Some(&self.preview.root));
                self.hpaned.set_position(width / 2);
                self.content_stack.set_visible_child_name("half-width");
            }
            PreviewMode::HalfHeight => {
                self.vpaned.set_start_child(Some(&self.editor_overlay_handle));
                self.vpaned.set_end_child(Some(&self.preview.root));
                self.vpaned.set_position(height / 2);
                self.content_stack.set_visible_child_name("half-height");
            }
            PreviewMode::FullWidth => {
                self.editor_page.append(&self.preview.root);
                self.content_stack.set_visible_child_name("editor");
            }
            PreviewMode::Windowed => {
                self.editor_page.append(&self.editor_overlay_handle);
                self.content_stack.set_visible_child_name("editor");
                let win = gtk::Window::new();
                win.set_title(Some(&format!(
                    "{} — Preview",
                    self.state.document.borrow().display_name()
                )));
                win.set_default_size(700, 700);
                win.set_child(Some(&self.preview.root));
                win.set_transient_for(Some(&self.root));
                {
                    let win_self = self.clone();
                    let preview_win = win.clone();
                    win.connect_close_request(move |_| {
                        if win_self.state.preview_active.get() {
                            win_self.preview_off();
                        }
                        let _ = &preview_win;
                        glib::Propagation::Proceed
                    });
                }
                win.present();
                *self.state.preview_window.borrow_mut() = Some(win);
            }
        }
    }

    /// Debounced preview refresh (~300 ms after the last edit).
    pub fn schedule_preview(&self) {
        if !self.state.preview_active.get() {
            return;
        }
        let generation = self.state.preview_generation.get() + 1;
        self.state.preview_generation.set(generation);
        let win = self.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(300), move || {
            if win.state.preview_generation.get() == generation {
                let text = win.editor.full_text();
                let doc_dir = win.editor.flags.doc_dir.borrow().clone();
                win.preview.update(&text, doc_dir);
            }
            glib::ControlFlow::Break
        });
    }
}

// ---------------------------------------------------------------------------
// Auto-hiding bars (`03-behavior/13-autohide.md`)
// ---------------------------------------------------------------------------

impl AppWindow {
    /// Hide both bars if all the auto-hide conditions hold: the
    /// preference is on, the layout is wide, no bar/banner is visible,
    /// and an edit was just made with the editor focused.
    fn maybe_autohide_bars(&self) {
        if !settings().boolean("autohide-headerbar") {
            return;
        }
        // Auto-hide never engages in the narrow layout (§1).
        if self.root.width() < 500 {
            return;
        }
        if self.search.bar.is_search_mode() {
            return;
        }
        if !self.editor.view.has_focus() {
            return;
        }
        if self.state.bars_hidden.get() {
            return;
        }
        self.state.bars_hidden.set(true);
        self.animate_bar_comp(true);
    }

    /// Fade both bars back in.
    pub fn reveal_bars(&self) {
        if !self.state.bars_hidden.get() {
            return;
        }
        self.state.bars_hidden.set(false);
        self.animate_bar_comp(false);
    }

    /// Fullscreen entry: hide the bars immediately unless a search bar or
    /// banner needs them; exit always reveals and refocuses the editor
    /// (`14-presentation-details.md` §3).
    fn hide_bars_for_fullscreen(&self) {
        if self.state.bars_hidden.get()
            || self.search.bar.is_search_mode()
            || self.recovery_bar.is_visible()
            || self.file_changed_bar.is_visible()
        {
            return;
        }
        self.state.bars_hidden.set(true);
        self.animate_bar_comp(true);
    }

    /// Track the space the top bars free while they animate so the first
    /// text line and the preview content do not shift vertically
    /// (`14-presentation-details.md` §2). The top-bar area collapses via
    /// the toolbar view; the overlay-hosted bottom bar fades manually
    /// (its revealer does not re-map reliably after a collapse).
    fn animate_bar_comp(&self, hiding: bool) {
        // The freed space is everything above the content (headerbar plus
        // any other visible top bars); measure before the collapse starts.
        let h = if hiding {
            let h = self
                .content_stack
                .translate_coordinates(&self.root, 0.0, 0.0)
                .map(|(_, y)| y.round() as i32)
                .unwrap_or(0)
                .max(0);
            self.state.bar_height.set(h);
            h
        } else {
            self.state.bar_height.get()
        };
        self.toolbar_view.set_reveal_top_bars(!hiding);
        self.bottom_bar.set_visible(true);
        let (start, end) = if hiding { (0, h) } else { (h, 0) };
        let win = self.clone();
        let generation = self.state.comp_generation.get() + 1;
        self.state.comp_generation.set(generation);
        let steps = 12;
        let mut i = 0u32;
        glib::timeout_add_local(std::time::Duration::from_millis(25), move || {
            if win.state.comp_generation.get() != generation {
                return glib::ControlFlow::Break;
            }
            i += 1;
            let t = i as f64 / steps as f64;
            let eased = 1.0 - (1.0 - t) * (1.0 - t);
            let v = (f64::from(start) + f64::from(end - start) * eased).round() as i32;
            let o = if hiding { 1.0 - eased } else { eased };
            win.bottom_bar.set_opacity(o.clamp(0.0, 1.0));
            win.editor.set_bar_comp(v);
            win.preview.root.set_margin_top(v);
            if i >= steps {
                if hiding {
                    win.bottom_bar.set_visible(false);
                    win.bottom_bar.set_opacity(1.0);
                }
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
    }
}

// ---------------------------------------------------------------------------
// Hemingway Mode feedback (`03-behavior/04-focus-and-hemingway.md` §2)
// ---------------------------------------------------------------------------

impl AppWindow {
    /// A user deletion was blocked: shake the editor area and show the
    /// explanation toast, subject to the frequency rule.
    fn on_delete_blocked(&self) {
        self.shake_editor();

        // Frequency rule: the toast shows at most for the first 3 blocked
        // attempts per installation; 4 attempts within roughly a minute
        // reset the counter so a user who keeps hitting the wall is
        // reminded again.
        let now = std::time::Instant::now();
        {
            let mut attempts = self.state.blocked_attempts.borrow_mut();
            attempts.push_back(now);
            while let Some(front) = attempts.front() {
                if now.duration_since(*front).as_secs() >= 60 {
                    attempts.pop_front();
                } else {
                    break;
                }
            }
            if attempts.len() >= 4 {
                attempts.clear();
                settings().set_integer("hemingway-toast-count", 0);
            }
        }
        let count = settings().integer("hemingway-toast-count");
        if count < 3 {
            settings().set_integer("hemingway-toast-count", count + 1);
            self.hemingway_toast();
        }
    }

    /// Small horizontal shake of the editor area.
    fn shake_editor(&self) {
        if self.state.shaking.get() {
            return;
        }
        self.state.shaking.set(true);
        let win = self.clone();
        const STEPS: [i32; 9] = [12, -12, 9, -9, 5, -5, 2, -2, 0];
        let mut i = 0usize;
        glib::timeout_add_local(std::time::Duration::from_millis(25), move || {
            let dx = STEPS[i];
            i += 1;
            win.editor
                .scroll
                .set_margin_start(dx.max(0));
            win.editor
                .scroll
                .set_margin_end((-dx).max(0));
            if i >= STEPS.len() {
                win.state.shaking.set(false);
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
    }

    /// The "text cannot be deleted" toast with a Learn More action;
    /// auto-dismisses after ~3 seconds.
    fn hemingway_toast(&self) {
        let toast = adw::Toast::new("Text cannot be deleted in Hemingway Mode");
        toast.set_button_label(Some("Learn More"));
        toast.set_timeout(3);
        {
            let win = self.clone();
            toast.connect_button_clicked(move |_| {
                win.show_hemingway_dialog();
            });
        }
        self.toast_overlay.add_toast(toast);
    }

    /// Short informational dialog about Hemingway Mode.
    fn show_hemingway_dialog(&self) {
        let dialog = adw::MessageDialog::builder()
            .heading("Hemingway Mode")
            .body(
                "While Hemingway Mode is on, text can only be added, never deleted. \
Backspace, Delete, cutting, and replacing a selection do nothing. \
Undo still works. Toggle the mode with Ctrl+T or from the main menu.",
            )
            .transient_for(&self.root)
            .modal(true)
            .build();
        dialog.add_response("ok", "OK");
        dialog.set_default_response(Some("ok"));
        dialog.set_close_response("ok");
        dialog.present();
    }
}

// ---------------------------------------------------------------------------
// Statistics (`03-behavior/05-statistics.md`)
// ---------------------------------------------------------------------------

impl AppWindow {
    /// Debounced (~50 ms) statistics recompute.
    pub fn schedule_stats(&self) {
        let generation = self.state.stats_generation.get() + 1;
        self.state.stats_generation.set(generation);
        let win = self.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
            if win.state.stats_generation.get() == generation {
                win.update_stats_now();
            }
            glib::ControlFlow::Break
        });
    }

    /// Recompute statistics (background) and update the status button.
    pub fn update_stats_now(&self) {
        let win = self.clone();
        let full = self.editor.full_text();
        let selection_text: Option<String> = self
            .editor
            .buffer
            .selection_bounds()
            .map(|(s, e)| self.editor.buffer.text(&s, &e, true).into());
        let has_selection = selection_text.is_some();

        glib::spawn_future_local(async move {
            let handle = gio::spawn_blocking(move || {
                let total = crate::markdown::stats::compute(&full);
                let selected = selection_text
                    .as_deref()
                    .map(crate::markdown::stats::compute);
                (total, selected)
            });
            let (total, selected) = handle.await.unwrap_or_default();
            let setting = settings().default_statistic();
            let Some(stat) =
                crate::markdown::stats::Statistic::parse_name(setting.as_str())
            else {
                return;
            };
            let label = match (has_selection, selected) {
                (true, Some(sel)) => format!(
                    "{} of {} {}",
                    stat.value(&sel),
                    stat.value(&total),
                    stat.display_name()
                ),
                _ => stat.label(&total),
            };
            win.stats_button.set_label(&label);
        });
    }
}
