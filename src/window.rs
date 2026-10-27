//! The main document window: headerbar, editor area, bottom bar, search
//! bar and banners. One window holds exactly one document.

use adw::prelude::*;
use gtk::gio;
use gtk::glib;
use std::cell::RefCell;

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
    /// Shared mutable state; all clones of AppWindow see the same state.
    pub state: std::rc::Rc<WindowState>,
}

/// Per-window mutable state shared between all `AppWindow` clones.
pub struct WindowState {
    pub document: RefCell<Document>,
    /// Suppress dirty marking while the program replaces the buffer
    /// (load, reload).
    pub suppress_dirty: RefCell<bool>,
    highlight_pending: RefCell<Option<glib::SourceId>>,
    last_layout_width: std::cell::Cell<i32>,
    pub preview_active: std::cell::Cell<bool>,
    preview_window: RefCell<Option<gtk::Window>>,
    preview_pending: RefCell<Option<glib::SourceId>>,
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
        headerbar.pack_end(&main_menu_button());
        headerbar.pack_end(&find_button);
        headerbar.pack_end(&layout_button);

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

        let stats_button = gtk::Button::new();
        stats_button.set_label("0 Words");
        stats_button.add_css_class("flat");
        stats_button.set_halign(gtk::Align::End);
        stats_button.set_valign(gtk::Align::Center);
        bottom_bar.append(&stats_button);

        // --- content -------------------------------------------------------
        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(&editor.scroll));
        overlay.add_overlay(&bottom_bar);
        overlay.set_measure_overlay(&bottom_bar, true);

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

        let toolbar_view = adw::ToolbarView::new();
        toolbar_view.add_top_bar(&save_progress);
        toolbar_view.add_top_bar(&headerbar);
        toolbar_view.set_content(Some(&content_stack));

        root.set_content(Some(&toolbar_view));

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
            state: std::rc::Rc::new(WindowState {
                document: RefCell::new(Document::new()),
                suppress_dirty: RefCell::new(false),
                highlight_pending: RefCell::new(None),
                last_layout_width: std::cell::Cell::new(0),
                preview_active: std::cell::Cell::new(settings().boolean("preview-active")),
                preview_window: RefCell::new(None),
                preview_pending: RefCell::new(None),
            }),
            preview: crate::preview::PreviewPane::new(),
        };

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
            }
            glib::ControlFlow::Continue
        });
    }

    /// Debounced highlighting rescan (~150 ms after the last edit).
    fn schedule_highlight(&self) {
        if let Some(id) = self.state.highlight_pending.borrow_mut().take() {
            id.remove();
        }
        let win = self.clone();
        let id = glib::timeout_add_local(std::time::Duration::from_millis(150), move || {
            win.editor.highlight_now();
            glib::ControlFlow::Break
        });
        *self.state.highlight_pending.borrow_mut() = Some(id);
    }

    fn install_actions(&self) {
        let actions = gio::SimpleActionGroup::new();
        let win_ref = self.clone();

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
        bind!("preview-close", AppWindow::preview_off);

        let close = gio::SimpleAction::new("close", None);
        {
            let root = self.root.clone();
            close.connect_activate(move |_, _| {
                root.close();
            });
        }
        actions.add_action(&close);

        for name in [
            "find",
            "find-replace",
            "toggle-preview",
            "focus-mode",
            "hemingway-mode",
            "preferences",
            "show-shortcuts",
        ] {
            let action = gio::SimpleAction::new(name, None);
            action.connect_activate(|_, _| {
                // Implemented in later milestones; the action exists so the
                // accelerators resolve.
            });
            actions.add_action(&action);
        }

        let about = gio::SimpleAction::new("about", None);
        {
            let root = self.root.clone();
            about.connect_activate(move |_, _| show_about(&root));
        }
        actions.add_action(&about);

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
                    win.schedule_preview();
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
        *self.editor.flags.doc_dir.borrow_mut() =
            file.parent().and_then(|p| p.path());
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
                self.update_title();
                recents::add(&file);
                files::delete_snapshot(self);
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

fn show_about(parent: &adw::ApplicationWindow) {
    let dialog = gtk::AboutDialog::new();
    dialog.set_transient_for(Some(parent));
    dialog.set_modal(true);
    dialog.set_program_name(Some("Griffel"));
    dialog.set_version(Some(env!("CARGO_PKG_VERSION")));
    dialog.set_comments(Some("A distraction-free Markdown editor"));
    dialog.set_website(Some("https://github.com/hakergeniusz/griffel"));
    dialog.set_website_label("Website");
    dialog.set_license_type(gtk::License::MitX11);
    dialog.set_logo_icon_name(Some(crate::app::ICON_NAME));
    dialog.set_authors(&["hakergeniusz"]);
    dialog.set_visible(true);
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

/// 5×5 size picker; hovering highlights the region and a label shows
/// "R x C" (`02-formatting.md` §7).
fn table_size_popover(buffer: &sourceview5::Buffer) -> gtk::Popover {
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
    for r in 1..=5usize {
        for c in 1..=5usize {
            let cell = gtk::Button::new();
            cell.set_size_request(20, 20);
            cell.add_css_class("flat");
            cell.set_tooltip_text(Some(&format!("{r} x {c}")));
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
        }
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
        if let Some(id) = self.state.preview_pending.borrow_mut().take() {
            id.remove();
        }
        let win = self.clone();
        let id = glib::timeout_add_local(std::time::Duration::from_millis(300), move || {
            let text = win.editor.full_text();
            let doc_dir = win.editor.flags.doc_dir.borrow().clone();
            win.preview.update(&text, doc_dir);
            glib::ControlFlow::Break
        });
        *self.state.preview_pending.borrow_mut() = Some(id);
    }
}
