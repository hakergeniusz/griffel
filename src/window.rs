//! The main document window: headerbar, tab bar, editor area, bottom bar,
//! search bar and banners. One window holds multiple documents as tabs
//! (`AdwTabView`); one tab holds exactly one document.

use adw::prelude::*;
use gtk::gio;
use gtk::glib;
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;

use crate::editor::Editor;
use crate::files::{self, recents, Document};
use crate::format;
use crate::i18n::{i18n, i18n_f, i18n_n};
use crate::settings::{settings, ColorScheme, PreviewMode};

/// Per-tab state: one editor, one document and its per-document services
/// (file monitor, snapshot and autosave timers, spell persistence).
pub struct TabState {
    pub editor: Editor,
    pub document: RefCell<Document>,
    /// Suppress dirty marking while the program replaces the buffer
    /// (load, reload).
    pub suppress_dirty: RefCell<bool>,
    /// Debounce generations for highlighting and spell rescans; edits are
    /// wired per tab so a slow rescan still targets the edited tab even
    /// after the user switches away.
    highlight_generation: Cell<u64>,
    spell_generation: Cell<u64>,
    pub monitor: RefCell<Option<gio::FileMonitor>>,
    pub snapshot_timer: RefCell<Option<glib::SourceId>>,
    pub snapshot_file: RefCell<Option<std::path::PathBuf>>,
    /// Debounce timer for the autosave preference.
    pub autosave_timer: RefCell<Option<glib::SourceId>>,
}

impl TabState {
    fn new() -> Self {
        Self {
            editor: Editor::new(),
            document: RefCell::new(Document::new()),
            suppress_dirty: RefCell::new(false),
            highlight_generation: Cell::new(0),
            spell_generation: Cell::new(0),
            monitor: RefCell::new(None),
            snapshot_timer: RefCell::new(None),
            snapshot_file: RefCell::new(None),
            autosave_timer: RefCell::new(None),
        }
    }

    pub fn doc(&self) -> std::cell::Ref<'_, Document> {
        self.document.borrow()
    }

    pub fn doc_mut(&self) -> std::cell::RefMut<'_, Document> {
        self.document.borrow_mut()
    }
}

pub struct AppWindow {
    pub root: adw::ApplicationWindow,
    headerbar: adw::HeaderBar,
    title_label: gtk::Label,
    subtitle_label: gtk::Label,
    pub tab_view: adw::TabView,
    tab_bar: adw::TabBar,
    toolbar_cluster: gtk::Box,
    find_button: gtk::ToggleButton,
    layout_button: gtk::MenuButton,
    save_progress: gtk::Revealer,
    preview: crate::preview::PreviewPane,
    content_stack: gtk::Stack,
    editor_page: gtk::Box,
    hpaned: gtk::Paned,
    vpaned: gtk::Paned,
    pub search: crate::search::SearchBar,
    stats_button: gtk::MenuButton,
    exit_fullscreen: gtk::Button,
    toolbar_view: adw::ToolbarView,
    /// Auto-hide wrappers for the header and bottom bars.
    headerbar_revealer: gtk::Revealer,
    recovery_bar: gtk::InfoBar,
    file_changed_bar: gtk::InfoBar,
    file_changed_reload: gtk::Button,
    toast_overlay: adw::ToastOverlay,
    /// Shared mutable state; all clones of AppWindow see the same state.
    pub state: Rc<WindowState>,
}

/// Per-window mutable state shared between all `AppWindow` clones. The
/// documents themselves live in the tabs; everything tied to the window
/// chrome (preview, bars, statistics) stays here.
pub struct WindowState {
    /// All open tabs of this window, in creation order. The active tab is
    /// derived from `tab_view.selected_page`, not stored.
    pub tabs: RefCell<Vec<Rc<TabState>>>,
    last_layout_width: Cell<i32>,
    pub preview_active: Cell<bool>,
    preview_window: RefCell<Option<gtk::Window>>,
    preview_generation: Cell<u64>,
    stats_generation: Cell<u64>,
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
    /// Focus Mode is per-window and unpersisted; new tabs read this
    /// instead of the action (the "win" group is not reachable through
    /// the root's action map).
    focus_mode: Cell<bool>,
}

impl Clone for AppWindow {
    fn clone(&self) -> Self {
        Self {
            root: self.root.clone(),
            headerbar: self.headerbar.clone(),
            title_label: self.title_label.clone(),
            subtitle_label: self.subtitle_label.clone(),
            tab_view: self.tab_view.clone(),
            tab_bar: self.tab_bar.clone(),
            toolbar_cluster: self.toolbar_cluster.clone(),
            find_button: self.find_button.clone(),
            layout_button: self.layout_button.clone(),
            save_progress: self.save_progress.clone(),
            content_stack: self.content_stack.clone(),
            editor_page: self.editor_page.clone(),
            hpaned: self.hpaned.clone(),
            vpaned: self.vpaned.clone(),
            search: self.search.clone(),
            stats_button: self.stats_button.clone(),
            exit_fullscreen: self.exit_fullscreen.clone(),
            toolbar_view: self.toolbar_view.clone(),
            headerbar_revealer: self.headerbar_revealer.clone(),
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

    // -- active-tab accessors -------------------------------------------------

    /// The tab whose page is currently selected. Falls back to the first
    /// tab when the selection is momentarily unset (e.g. during teardown).
    pub fn active_tab(&self) -> Rc<TabState> {
        let all = self.state.tabs.borrow().clone();
        let selected = self.tab_view.selected_page();
        if let Some(page) = selected {
            let child = page.child();
            if let Some(tab) = all
                .iter()
                .find(|t| {
                    t.editor
                        .scroll
                        .upcast_ref::<gtk::Widget>()
                        .as_ptr()
                        == child.as_ptr()
                })
            {
                return tab.clone();
            }
        }
        all.into_iter().next().expect("window has no tabs")
    }

    pub fn tabs(&self) -> Vec<Rc<TabState>> {
        self.state.tabs.borrow().clone()
    }

    /// The active tab's editor.
    pub fn editor(&self) -> Editor {
        self.active_tab().editor.clone()
    }

    fn tab_for_page(&self, page: &adw::TabPage) -> Option<Rc<TabState>> {
        let child = page.child();
        self.state
            .tabs
            .borrow()
            .iter()
            .find(|t| t.editor.scroll.upcast_ref::<gtk::Widget>().as_ptr() == child.as_ptr())
            .cloned()
    }

    /// Open a new empty document tab, select it and focus it.
    pub fn open_tab(&self) {
        let tab = Rc::new(TabState::new());
        // Modes and geometry the window already decided for its other
        // editors.
        tab.editor
            .flags
            .hemingway
            .set(settings().boolean("hemingway-mode"));
        {
            let win2 = self.clone();
            tab.editor
                .blocked_callback
                .borrow_mut()
                .replace(Box::new(move || win2.on_delete_blocked()));
        }
        if self.state.bars_hidden.get() {
            tab.editor.set_bar_comp(self.state.bar_height.get());
        }
        let width = self.state.last_layout_width.get();
        if width > 0 {
            tab.editor.relayout_column(
                width,
                settings().integer("characters-per-line"),
                settings().boolean("bigger-text"),
            );
            tab.editor.apply_focus_margins();
        }
        // Focus Mode is a window-wide state; new tabs follow it.
        if self.state.focus_mode.get() {
            tab.editor.set_focus_mode(true);
            tab.editor.refresh_spell();
        }

        self.tab_view.append(&tab.editor.scroll);
        self.state.tabs.borrow_mut().push(tab.clone());
        Self::wire_tab(self, &tab);
        self.update_tab_title(&tab);
        self.tab_view.set_selected_page(
            &self.page_for_tab(&tab).expect("just appended"),
        );
        tab.editor.view.grab_focus();
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

        // The window opens with one empty document tab.
        let first_tab = Rc::new(TabState::new());
        let tab_view = adw::TabView::new();
        tab_view.append(&first_tab.editor.scroll);
        // One tab shows no bar (auto-hide); with several tabs the bar
        // appears in the top bars and hides together with them (03/13).
        let tab_bar = adw::TabBar::new();
        tab_bar.set_view(Some(&tab_view));
        tab_bar.set_autohide(true);

        // --- headerbar -----------------------------------------------------
        let headerbar = adw::HeaderBar::new();

        let new_button = gtk::Button::new();
        new_button.set_icon_name("tab-new-symbolic");
        new_button.set_tooltip_text(Some(&i18n("New Tab")));
        new_button.set_action_name(Some("win.new-tab"));

        let open_button = gtk::MenuButton::new();
        open_button.set_icon_name("document-open-symbolic");
        open_button.set_tooltip_text(Some(&i18n("Open")));

        let save_button = gtk::Button::new();
        save_button.set_label(&i18n("Save"));
        save_button.set_tooltip_text(Some(&i18n("Save")));
        // Raised, like the dropdown half: with `.linked` the two halves
        // render as one split button with the same background.
        save_button.set_action_name(Some("win.save"));
        let save_menu = gtk::MenuButton::new();
        save_menu.set_icon_name("pan-down-symbolic");
        save_menu.set_tooltip_text(Some(&i18n("Save and Export")));
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
        find_button.set_tooltip_text(Some(&i18n("Find")));

        let layout_button = preview_layout_button();
        let exit_fullscreen = gtk::Button::from_icon_name("view-restore-symbolic");
        exit_fullscreen.set_tooltip_text(Some(&i18n("Leave fullscreen")));
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
        let title_label = gtk::Label::new(Some(&i18n("New File")));
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
        // Two overlay clusters (toolbar + chevron at the left, stats at the
        // right) leave the middle genuinely empty, so clicks there pass
        // through to the editor (`13-autohide.md` §3). A full-width bar
        // widget would swallow those clicks, and `can_target(false)` would
        // make its whole subtree unpickable.
        let toolbar_cluster = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        toolbar_cluster.add_css_class("bottom-bar");
        toolbar_cluster.set_halign(gtk::Align::Start);
        toolbar_cluster.set_valign(gtk::Align::End);
        toolbar_cluster.set_margin_start(6);
        toolbar_cluster.set_margin_bottom(6);

        let toolbar_row = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        toolbar_row.add_css_class("toolbar-row");
        let toolbar_toggle = gtk::ToggleButton::new();
        toolbar_toggle.set_icon_name("pan-end-symbolic");
        toolbar_toggle.set_tooltip_text(Some(&i18n("Show formatting")));
        // The row reveals with a slide so toggling reads as motion.
        let toolbar_row_revealer = gtk::Revealer::new();
        toolbar_row_revealer.set_transition_type(gtk::RevealerTransitionType::SlideLeft);
        toolbar_row_revealer.set_transition_duration(250);
        toolbar_row_revealer.set_child(Some(&toolbar_row));
        toolbar_row_revealer.set_reveal_child(false);
        toolbar_cluster.append(&toolbar_row_revealer);
        toolbar_cluster.append(&toolbar_toggle);

        let stats_button = gtk::MenuButton::new();
        stats_button.set_label(&i18n_n("{} word", "{} words", 0));
        // Raised, like the toolbar toggle beside it: it floats over the
        // editor text, so a transparent background would let the document
        // show through.
        stats_button.set_halign(gtk::Align::End);
        stats_button.set_valign(gtk::Align::End);
        stats_button.set_margin_end(6);
        stats_button.set_margin_bottom(6);

        // --- recovery & file-changed banners (infobars) ---------------------
        let recovery_bar = gtk::InfoBar::new();
        recovery_bar.set_message_type(gtk::MessageType::Info);
        recovery_bar.set_show_close_button(true);
        let recovery_label =
            gtk::Label::new(Some(&i18n("This document was recovered from an autosave.")));
        recovery_bar.add_child(&recovery_label);
        recovery_bar.set_revealed(false);

        let file_changed_bar = gtk::InfoBar::new();
        file_changed_bar.set_message_type(gtk::MessageType::Warning);
        file_changed_bar.set_show_close_button(true);
        let fc_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let fc_label = gtk::Label::new(Some(&i18n("This file changed on disk.")));
        fc_box.append(&fc_label);
        let file_changed_reload = gtk::Button::with_label(&i18n("Reload"));
        file_changed_reload.add_css_class("flat");
        fc_box.append(&file_changed_reload);
        file_changed_bar.add_child(&fc_box);
        file_changed_bar.set_revealed(false);

        // --- content -------------------------------------------------------
        // Preview layouts: the editor page hosts the tab view; the panes
        // host it together with the preview pane. The tab view is moved
        // between parents on layout switches.
        let content_stack = gtk::Stack::new();
        content_stack.set_transition_type(gtk::StackTransitionType::Crossfade);
        content_stack.set_transition_duration(250);
        content_stack.set_hexpand(true);
        content_stack.set_vexpand(true);
        let editor_page = gtk::Box::new(gtk::Orientation::Vertical, 0);
        editor_page.append(&tab_view);
        content_stack.add_named(&editor_page, Some("editor"));
        let hpaned = gtk::Paned::new(gtk::Orientation::Horizontal);
        let vpaned = gtk::Paned::new(gtk::Orientation::Vertical);
        content_stack.add_named(&hpaned, Some("half-width"));
        content_stack.add_named(&vpaned, Some("half-height"));

        // The bottom-bar clusters float over the whole content area
        // (toolbar at the left, statistics at the right) via an outer
        // overlay; the middle stays genuinely click-through to the editor
        // (`13-autohide.md` §3).
        let window_overlay = gtk::Overlay::new();
        window_overlay.set_child(Some(&content_stack));
        window_overlay.add_overlay(&toolbar_cluster);
        window_overlay.add_overlay(&stats_button);
        window_overlay.set_measure_overlay(&toolbar_cluster, true);
        window_overlay.set_measure_overlay(&stats_button, true);

        let search = crate::search::SearchBar::new(&first_tab.editor.view, &first_tab.editor.buffer);

        let toolbar_view = adw::ToolbarView::new();
        toolbar_view.add_top_bar(&save_progress);
        toolbar_view.add_top_bar(&recovery_bar);
        toolbar_view.add_top_bar(&file_changed_bar);
        toolbar_view.add_top_bar(&headerbar_revealer);
        toolbar_view.add_top_bar(&tab_bar);
        toolbar_view.add_top_bar(&search.bar);
        toolbar_view.set_content(Some(&window_overlay));

        // Toasts surface here (blocked-deletion feedback and friends).
        let toast_overlay = adw::ToastOverlay::new();
        toast_overlay.set_child(Some(&toolbar_view));

        root.set_content(Some(&toast_overlay));

        let win = Self {
            root,
            headerbar,
            title_label,
            subtitle_label,
            tab_view,
            tab_bar,
            toolbar_cluster,
            find_button,
            layout_button,
            save_progress,
            content_stack,
            editor_page,
            hpaned,
            vpaned,
            search,
            stats_button: stats_button.clone(),
            headerbar_revealer,
            recovery_bar,
            file_changed_bar,
            file_changed_reload,
            exit_fullscreen,
            toolbar_view,
            toast_overlay,
            state: std::rc::Rc::new(WindowState {
                tabs: RefCell::new(vec![first_tab]),
                last_layout_width: std::cell::Cell::new(0),
                preview_active: std::cell::Cell::new(settings().boolean("preview-active")),
                preview_window: RefCell::new(None),
                preview_generation: std::cell::Cell::new(0),
                stats_generation: std::cell::Cell::new(0),
                bars_hidden: Cell::new(false),
                bar_height: Cell::new(0),
                comp_generation: Cell::new(0),
                shaking: Cell::new(false),
                blocked_attempts: RefCell::new(VecDeque::new()),
                focus_mode: Cell::new(false),
            }),
            preview: crate::preview::PreviewPane::new(),
        };
        // Hemingway Mode startup state comes from the preference (03/04 §2).
        // Blocked-deletion feedback: shake + toast. Both apply to every
        // editor; per-tab signal wiring happens in `wire_tab`.
        {
            for tab in win.tabs() {
                tab.editor
                    .flags
                    .hemingway
                    .set(settings().boolean("hemingway-mode"));
                let win2 = win.clone();
                tab.editor
                    .blocked_callback
                    .borrow_mut()
                    .replace(Box::new(move || win2.on_delete_blocked()));
                Self::wire_tab(&win, &tab);
                // The startup tab gets no update_tab_title from open_tab.
                win.update_tab_title(&tab);
            }
        }

        // Find toggle button ↔ search bar visibility.
        {
            let win2 = win.clone();
            let find_btn = win.find_button.clone();
            find_btn.connect_toggled(move |t| {
                if t.is_active() {
                    win2.search.open(false, &win2.editor().buffer);
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

        build_toolbar(&win, &toolbar_row, &toolbar_row_revealer, &toolbar_toggle);

        win.install_actions();
        win.connect_signals();
        win.start_layout_polling();
        crate::app::register_window(win.clone());
        win
    }

    /// Adaptive column layout: re-run when the window size changes. GTK4
    /// has no size-allocate signal, so poll cheaply and skip no-ops. The
    /// width is the tab view's (the editors' container); every tab's
    /// column is re-laid out so switching tabs needs no reflow.
    fn start_layout_polling(&self) {
        let win = self.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(200), move || {
            // The column is laid out for the editors' own width, not the
            // window's: in the beside layout the editor only gets half the
            // window, and a layout switch changes the editor's width
            // without the window changing at all.
            let width = win.tab_view.width();
            if width > 0 && width != win.state.last_layout_width.get() {
                win.state.last_layout_width.set(width);
                for tab in win.tabs() {
                    tab.editor.relayout_column(
                        width,
                        settings().integer("characters-per-line"),
                        settings().boolean("bigger-text"),
                    );
                    // Keep Focus Mode margins at half the viewport.
                    tab.editor.apply_focus_margins();
                }
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

    /// Debounced highlighting rescan (~150 ms after the last edit). Wired
    /// per tab: a slow rescan still targets the edited tab after the user
    /// switches away.
    fn schedule_highlight(&self, tab: &Rc<TabState>) {
        let generation = tab.highlight_generation.get() + 1;
        tab.highlight_generation.set(generation);
        let tab = tab.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(150), move || {
            if tab.highlight_generation.get() == generation {
                tab.editor.highlight_now();
            }
            glib::ControlFlow::Break
        });
    }

    /// Debounced spell rescan (~250 ms after the last edit).
    fn schedule_spell(&self, tab: &Rc<TabState>) {
        let generation = tab.spell_generation.get() + 1;
        tab.spell_generation.set(generation);
        let tab = tab.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(250), move || {
            if tab.spell_generation.get() == generation {
                tab.editor.spell_now();
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
                    // Choosing a layout with the preview off turns the preview
                    // on in that layout — otherwise the switcher would do
                    // nothing visible at all.
                    let win2 = win_ref.clone();
                    if win2.state.preview_active.get() {
                        win2.apply_preview_mode(mode);
                    } else {
                        win2.preview_on();
                    }
                }
            });
        }
        actions.add_action(&preview_mode);

        let preview_off = gio::SimpleAction::new("preview-off", None);
        {
            let win2 = win_ref2.clone();
            preview_off.connect_activate(move |_, _| {
                win2.preview_off();
            });
        }
        actions.add_action(&preview_off);

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
        bind!("save-as", |w: &AppWindow| files::save_as_dialog(w, &w.active_tab()));

        // Formatting commands (`04-keyboard-shortcuts.md`).
        bind!("format-bold", |w: &AppWindow| format::wrap(&w.editor().buffer, "**", "bold text"));
        bind!("format-italic", |w: &AppWindow| format::wrap(&w.editor().buffer, "_", "italic text"));
        bind!("format-strike", |w: &AppWindow| format::wrap(&w.editor().buffer, "~~", "strikethrough text"));
        bind!("format-code", |w: &AppWindow| format::code(&w.editor().buffer));
        bind!("format-heading", |w: &AppWindow| format::heading(&w.editor().buffer, None));
        for n in 1..=6u8 {
            let action = gio::SimpleAction::new(&format!("format-heading-{n}"), None);
            let win = self.clone();
            action.connect_activate(move |_, _| format::heading(&win.editor().buffer, Some(n)));
            actions.add_action(&action);
        }
        bind!("format-bullet", |w: &AppWindow| format::list(&w.editor().buffer, format::ListCommand::Bullet));
        bind!("format-numbered", |w: &AppWindow| format::list(&w.editor().buffer, format::ListCommand::Numbered));
        bind!("format-checklist", |w: &AppWindow| format::list(&w.editor().buffer, format::ListCommand::Checklist));
        bind!("format-quote", |w: &AppWindow| format::block_quote(&w.editor().buffer));
        bind!("format-link", |w: &AppWindow| format::link(&w.editor().buffer));
        bind!("format-image", |w: &AppWindow| {
            let dir = w.editor().flags.doc_dir.borrow().clone();
            format::image(&w.editor().buffer, dir, &w.root);
        });
        bind!("format-rule", |w: &AppWindow| format::horizontal_rule(&w.editor().buffer));
        bind!("format-fix-table", |w: &AppWindow| {
            if !format::fix_table(&w.editor().buffer) {
                w.show_error(&i18n("No table"), &i18n("The cursor is not inside a table."));
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
        bind!("find", |w: &AppWindow| w.search.open(false, &w.editor().buffer));
        bind!("find-replace", |w: &AppWindow| w.search.open(true, &w.editor().buffer));

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
                    Some(&st.display_name()),
                    Some(&format!("win.statistic::{}", st.as_name())),
                );
            }
            self.stats_button.set_menu_model(Some(&menu));
            self.stats_button.set_always_show_arrow(false);
        }
        self.update_stats_now();

        let close_window = gio::SimpleAction::new("close-window", None);
        {
            let root = self.root.clone();
            close_window.connect_activate(move |_, _| {
                root.close();
            });
        }
        actions.add_action(&close_window);

        // Tabs: Ctrl+N opens a document tab; Ctrl+W closes the current one
        // (with the unsaved-changes guard). Shift variants act on the
        // window instead. Closing the last tab closes the window.
        let new_tab = gio::SimpleAction::new("new-tab", None);
        {
            let win = self.clone();
            new_tab.connect_activate(move |_, _| {
                win.open_tab();
            });
        }
        actions.add_action(&new_tab);

        let close_tab = gio::SimpleAction::new("close-tab", None);
        {
            let win = self.clone();
            close_tab.connect_activate(move |_, _| {
                if let Some(page) = win.tab_view.selected_page() {
                    win.tab_view.close_page(&page);
                }
            });
        }
        actions.add_action(&close_tab);

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
                win_ref.state.focus_mode.set(new);
                for tab in win_ref.tabs() {
                    tab.editor.set_focus_mode(new);
                    // Focus Mode temporarily disables spell checking (§4).
                    tab.editor.refresh_spell();
                }
            });
        }
        actions.add_action(&focus_mode);

        // Hemingway Mode (03/04 §2): deletions blocked; the state is
        // remembered across sessions. The preference is the single source
        // of truth — deriving the toggle from it (not from the action
        // state) stays correct no matter which window, the preferences
        // dialog, or gsettings moved the key. The editors follow through
        // the settings-changed handler in app.rs.
        let hemingway = gio::SimpleAction::new_stateful(
            "hemingway-mode",
            None,
            &settings().boolean("hemingway-mode").to_variant(),
        );
        hemingway.connect_activate(move |action, _| {
            let new = !settings().boolean("hemingway-mode");
            action.set_state(&new.to_variant());
            settings().set_boolean("hemingway-mode", new);
        });
        actions.add_action(&hemingway);

        // Code Mode (F8, main menu): line numbers, editor-style pairing,
        // plain Enter/Tab, code-token colors in fenced blocks. The state
        // is remembered across sessions; the editors follow the preference
        // through the global settings-changed handler in app.rs, which
        // also re-runs the column layout (prose ↔ code geometry).
        let code_mode = gio::SimpleAction::new_stateful(
            "code-mode",
            None,
            &settings().boolean("code-mode").to_variant(),
        );
        code_mode.connect_activate(move |action, _| {
            let new = !settings().boolean("code-mode");
            action.set_state(&new.to_variant());
            settings().set_boolean("code-mode", new);
        });
        actions.add_action(&code_mode);

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
        toggle_spell.connect_activate(move |action, _| {
            let new = !settings().boolean("spellcheck");
            action.set_state(&new.to_variant());
            settings().set_boolean("spellcheck", new);
        });
        actions.add_action(&toggle_spell);

        // Each window owns its own stateful action instance, and a state
        // set on one window's instance never reaches the others (the "win"
        // group is not visible through the root's action map, so a
        // cross-window lookup finds nothing). Mirror the preference into
        // every action on change: menu checkboxes stay right in windows
        // that did not originate the toggle, and the mirrors cannot fight
        // the activate handlers because those read the preference, not
        // the state.
        for (key, action) in [
            ("hemingway-mode", hemingway.clone()),
            ("code-mode", code_mode.clone()),
            ("spellcheck", toggle_spell.clone()),
        ] {
            settings().changed(move |s, k| {
                if k == key {
                    action.set_state(&s.boolean(key).to_variant());
                }
            });
        }

        self.root.insert_action_group("win", Some(&actions));
    }

    fn connect_signals(&self) {
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
                    win.editor().view.grab_focus();
                }
            });
        }

        // Scroll synchronization (preview → editor) via injected script.
        {
            let win = self.clone();
            self.preview.install_scroll_script(move |f| {
                let win = &win;
                if win.state.preview_active.get() {
                    win.preview.sync_from_preview(f, |v| win.editor().set_scroll_fraction(v));
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

        // Tab switching: the window chrome (title, statistics, preview,
        // search context) follows the selected document.
        {
            let win = self.clone();
            self.tab_view.connect_selected_page_notify(move |_| {
                win.on_tab_switched();
            });
        }

        // Tab close: unsaved-changes confirmation per document, then the
        // usual per-document cleanup. Closing the last tab closes the
        // window.
        {
            let win = self.clone();
            self.tab_view.connect_close_page(move |view, page| {
                let Some(tab) = win.tab_for_page(page) else {
                    return glib::Propagation::Proceed;
                };
                let win = win.clone();
                let view = view.clone();
                let page = page.clone();
                glib::spawn_future_local(async move {
                    let may_close = win.confirm_close_tab(&tab).await;
                    if may_close {
                        view.close_page_finish(&page, true);
                    } else {
                        view.close_page_finish(&page, false);
                    }
                });
                glib::Propagation::Stop
            });
        }
        {
            let win = self.clone();
            self.tab_view.connect_page_detached(move |_view, page, _pos| {
                if let Some(tab) = win.tab_for_page(page) {
                    win.teardown_tab(&tab);
                }
                // Closing the last tab closes the whole window; the close
                // request path re-runs the per-tab confirmations, which are
                // all no-ops on an empty tab set.
                if win.state.tabs.borrow().is_empty() {
                    win.root.close();
                }
            });
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
                        for tab in win.tabs() {
                            win.teardown_tab(&tab);
                        }
                        crate::app::unregister_window(&win);
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

    /// Window-level reactions to the selected tab changing.
    fn on_tab_switched(&self) {
        // During teardown of the last tab the selection fires with nothing
        // left to show; the window is closing anyway.
        if self.state.tabs.borrow().is_empty() {
            return;
        }
        self.update_title();
        self.update_stats_now();
        let tab = self.active_tab();
        // The search bar's matches always target the visible document.
        self.search.retarget(&tab.editor.buffer);
        if self.state.preview_active.get() {
            if self.active_code_file() {
                // Markdown preview does not follow onto a code file.
                self.preview_off();
            } else {
                self.preview
                    .evaluate_security(&tab.editor.full_text(), tab.doc().file.is_none());
                self.schedule_preview();
            }
        }
        tab.editor.view.grab_focus();
    }

    /// Per-tab signal wiring: dirty tracking, debounced highlighting and
    /// spell rescans, statistics, preview/autohide nudges, scroll sync,
    /// auto-hide reveal on focus loss.
    fn wire_tab(win: &AppWindow, tab: &Rc<TabState>) {
        // Dirty tracking: any buffer change marks the document dirty
        // (suppressed while the program replaces content).
        {
            let win = win.clone();
            let tab = tab.clone();
            let buffer = tab.editor.buffer.clone();
            buffer.connect_changed(move |_| {
                win.set_dirty_from_edit(&tab);
                win.schedule_highlight(&tab);
                win.schedule_spell(&tab);
                win.schedule_preview();
                win.maybe_autohide_bars(&tab);
                tab.editor.focus_update();
            });
        }

        // Statistics update on selection change.
        {
            let win = win.clone();
            tab.editor
                .buffer
                .connect_mark_set(move |_buf, _location, _mark| {
                    win.schedule_stats();
                });
        }

        // Scroll synchronization (editor → preview), only while this tab
        // is the one the user is looking at.
        {
            let win = win.clone();
            let tab = tab.clone();
            tab.editor
                .scroll
                .vadjustment()
                .connect_value_changed(move |_| {
                    if !win.is_active_tab(&tab) {
                        return;
                    }
                    if win.state.preview_active.get() {
                        let f = tab.editor.scroll_fraction();
                        win.preview.sync_from_editor(f);
                    }
                });
        }

        // Auto-hide reveal: the editor losing keyboard focus (03/13 §2).
        {
            let win = win.clone();
            let focus_ctl = gtk::EventControllerFocus::new();
            focus_ctl.connect_leave(move |_| win.reveal_bars());
            tab.editor.view.add_controller(focus_ctl);
        }
    }

    /// True when `tab` is the selected tab of this window.
    fn is_active_tab(&self, tab: &Rc<TabState>) -> bool {
        self.active_tab().editor.scroll.as_ptr() == tab.editor.scroll.as_ptr()
    }

    /// Make `tab` the selected tab (open routing, recovery).
    pub fn select_tab(&self, tab: &Rc<TabState>) {
        if let Some(page) = self.page_for_tab(tab) {
            self.tab_view.set_selected_page(&page);
        }
    }

    /// Drop a tab's per-document services (monitor, timers, snapshot) and
    /// remove it from the window's tab registry.
    fn teardown_tab(&self, tab: &Rc<TabState>) {
        if let Some(id) = tab.snapshot_timer.borrow_mut().take() {
            id.remove();
        }
        if let Some(id) = tab.autosave_timer.borrow_mut().take() {
            id.remove();
        }
        if let Some(m) = tab.monitor.borrow_mut().take() {
            m.cancel();
        }
        files::delete_snapshot_tab(self, tab);
        self.state
            .tabs
            .borrow_mut()
            .retain(|t| t.editor.scroll.as_ptr() != tab.editor.scroll.as_ptr());
    }

    fn set_dirty_from_edit(&self, tab: &Rc<TabState>) {
        if *tab.suppress_dirty.borrow() {
            return;
        }
        self.schedule_autosave(tab);
        if !tab.doc().dirty {
            tab.doc_mut().dirty = true;
            self.update_title();
            self.update_tab_title(tab);
            files::schedule_snapshot_tab(self, tab);
            crate::app::update_logout_inhibition();
        }
    }

    /// Autosave preference: save a few seconds after the last edit
    /// (user-added setting, not in `03/10`). Debounced per edit; skipped
    /// for untitled documents (nothing to save to) and while an
    /// external-change conflict is pending the user's choice.
    fn schedule_autosave(&self, tab: &Rc<TabState>) {
        if !settings().boolean("autosave") {
            return;
        }
        self.cancel_autosave_timer(tab);
        let win = self.clone();
        let tab = tab.clone();
        let timer_tab = tab.clone();
        let id = glib::timeout_add_seconds_local(5, move || {
            timer_tab.autosave_timer.borrow_mut().take();
            win.autosave_now(&timer_tab);
            glib::ControlFlow::Break
        });
        *tab.autosave_timer.borrow_mut() = Some(id);
    }

    fn cancel_autosave_timer(&self, tab: &Rc<TabState>) {
        if let Some(id) = tab.autosave_timer.borrow_mut().take() {
            id.remove();
        }
    }

    fn autosave_now(&self, tab: &Rc<TabState>) {
        if !settings().boolean("autosave") {
            return;
        }
        if !tab.doc().dirty {
            return;
        }
        if self.file_changed_bar.is_revealed() {
            return;
        }
        if let Some(file) = tab.doc().file.clone() {
            self.save_tab_to(tab.clone(), file);
        }
    }

    pub fn set_document_dirty(&self, dirty: bool) {
        let tab = self.active_tab();
        tab.doc_mut().dirty = dirty;
        self.update_title();
        self.update_tab_title(&tab);
        crate::app::update_logout_inhibition();
    }

    pub fn update_title(&self) {
        let tab = self.active_tab();
        let doc = tab.doc();
        let name = doc.display_name();
        let marker = if doc.dirty { "• " } else { "" };
        self.title_label.set_text(&format!("{marker}{name}"));
        let parent = doc.parent_path().unwrap_or_default();
        self.subtitle_label.set_text(&parent);
        let path = doc.path_string();
        self.root.set_tooltip_text(path.as_deref());
    }

    /// The tab's page title: the document name, marked when dirty.
    pub(crate) fn update_tab_title(&self, tab: &Rc<TabState>) {
        let Some(page) = self.page_for_tab(tab) else {
            return;
        };
        let doc = tab.doc();
        let name = doc.display_name();
        let marker = if doc.dirty { "• " } else { "" };
        page.set_title(&format!("{marker}{name}"));
    }

    fn page_for_tab(&self, tab: &Rc<TabState>) -> Option<adw::TabPage> {
        let ptr = tab
            .editor
            .scroll
            .upcast_ref::<gtk::Widget>()
            .as_ptr();
        let pages = self.tab_view.pages();
        (0..pages.n_items())
            .filter_map(|i| pages.item(i))
            .find_map(|obj| {
                let page = obj.downcast::<adw::TabPage>().ok()?;
                (page.child().as_ptr() == ptr).then_some(page)
            })
    }

    pub fn reload_from_disk(&self) {
        self.file_changed_bar.set_revealed(false);
        let tab = self.active_tab();
        if let Some(file) = tab.doc().file.clone() {
            self.load_file_into(&tab, &file);
        }
        self.file_changed_bar.set_revealed(false);
    }

    pub fn show_file_changed(&self) {
        self.file_changed_bar.set_revealed(true);
    }

    pub fn show_recovery_banner(&self) {
        self.recovery_bar.set_revealed(true);
    }

    pub fn cancel_snapshot_timer(&self, tab: &Rc<TabState>) {
        if let Some(id) = tab.snapshot_timer.borrow_mut().take() {
            id.remove();
        }
    }

    /// Replace the editor content and document state after a successful
    /// load (§2.2).
    pub fn apply_loaded(
        &self,
        tab: &Rc<TabState>,
        file: &gio::File,
        decoded: &files::encoding::Decoded,
    ) {
        *tab.suppress_dirty.borrow_mut() = true;
        tab.editor.set_full_text(&decoded.text);
        *tab.suppress_dirty.borrow_mut() = false;
        {
            let mut doc = tab.doc_mut();
            doc.file = Some(file.clone());
            doc.encoding = decoded.encoding;
            doc.dirty = false;
            doc.recovered = false;
        }
        self.update_title();
        self.update_tab_title(tab);
        recents::add(file);
        files::monitor::install_tab(self, tab, file);
        *tab.editor.flags.doc_dir.borrow_mut() = file.parent().and_then(|p| p.path());
        // Per-file spell language (`05-…persistence.md` §4).
        *tab.editor.spell_persist_target.borrow_mut() = Some(file.clone());
        // Code files open with Code Mode on for this tab regardless of the
        // global preference; other files restore the preference (a tab
        // reused from a code document goes back to prose behavior).
        let code_file = files::is_code_file(file);
        let code_on = settings().boolean("code-mode") || code_file;
        if tab.editor.flags.code_mode.get() != code_on {
            tab.editor.set_code_mode(code_on);
            if code_on {
                let width = self.editor_width();
                if width > 0 {
                    tab.editor.relayout_column(
                        width,
                        settings().integer("characters-per-line"),
                        settings().boolean("bigger-text"),
                    );
                    tab.editor.apply_focus_margins();
                }
            }
        }
        let lang = files::metadata::load_language(file);
        tab.editor.set_spell_language(lang);
        tab.editor.highlight_now();
        // The changed-handler path skips statistics while content is
        // replaced programmatically; recompute for the loaded text.
        self.schedule_stats();
        if code_file && self.state.preview_active.get() && self.is_active_tab(tab) {
            // Markdown preview does not apply to code files.
            self.preview_off();
        }
        if self.state.preview_active.get() && self.is_active_tab(tab) {
            self.preview.evaluate_security(&decoded.text, false);
            self.schedule_preview();
        }
        if self.is_active_tab(tab) {
            tab.editor.view.grab_focus();
        }
    }

    /// Internal resource (tutorial): loads text, not added to recents,
    /// starts unsaved (`03-behavior/12-windows.md` §3).
    pub fn load_internal_text(&self, text: &str) {
        let tab = self.active_tab();
        *tab.suppress_dirty.borrow_mut() = true;
        tab.editor.set_full_text(text);
        *tab.suppress_dirty.borrow_mut() = false;
        {
            let mut doc = tab.doc_mut();
            doc.file = None;
            doc.dirty = true;
        }
        self.update_title();
        self.update_tab_title(&tab);
        *tab.editor.spell_persist_target.borrow_mut() = None;
        tab.editor.set_spell_language(None);
        tab.editor.highlight_now();
        self.schedule_stats();
    }

    /// Load a file into the active tab (used by routing).
    pub fn load_file(&self, file: &gio::File) {
        let tab = self.active_tab();
        self.load_file_into(&tab, file);
    }

    /// Load a file into `tab` (used by routing and reload). On failure an
    /// error dialog is shown and the tab resets to a new empty document.
    pub fn load_file_into(&self, tab: &Rc<TabState>, file: &gio::File) {
        if let Err(e) = files::load_into_tab(self, tab, file) {
            self.show_error(&i18n("Cannot open file"), &e);
            self.reset_to_new_document(tab);
        }
    }

    /// Reset the tab to a new empty, clean document (load-failure path).
    fn reset_to_new_document(&self, tab: &Rc<TabState>) {
        *tab.suppress_dirty.borrow_mut() = true;
        tab.editor.set_full_text("");
        *tab.suppress_dirty.borrow_mut() = false;
        {
            let mut doc = tab.doc_mut();
            doc.file = None;
            doc.encoding = "UTF-8";
            doc.dirty = false;
            doc.recovered = false;
        }
        *tab.editor.flags.doc_dir.borrow_mut() = None;
        *tab.editor.spell_persist_target.borrow_mut() = None;
        tab.editor.set_spell_language(None);
        self.update_title();
        self.update_tab_title(tab);
        self.schedule_stats();
    }

    /// Save: to the backing file, or via Save As when there is none.
    pub fn save(&self) {
        let tab = self.active_tab();
        let file = tab.doc().file.clone();
        match file {
            Some(file) => self.save_tab_to(tab, file),
            None => files::save_as_dialog(self, &tab),
        }
    }

    /// Write to `file`, associate it as the backing file, clear dirty,
    /// register recents, drop the recovery snapshot. Saves the active tab.
    pub fn save_to(&self, file: gio::File) {
        let tab = self.active_tab();
        self.save_tab_to(tab, file);
    }

    /// Write `tab`'s document to `file`, associate it as the backing file,
    /// clear dirty, register recents, drop the recovery snapshot.
    pub(crate) fn save_tab_to(&self, tab: Rc<TabState>, file: gio::File) {
        // A save starting reveals the auto-hidden bars (03/13 §2).
        self.reveal_bars();
        files::monitor::pause_tab(&tab);
        self.save_progress.set_reveal_child(true);
        let text = tab.editor.full_text();
        let encoding = tab.doc().encoding.to_owned();
        let result = files::write_to(&file, &text, &encoding);
        self.save_progress.set_reveal_child(false);

        match result {
            Ok(effective) => {
                {
                    let mut doc = tab.doc_mut();
                    doc.file = Some(file.clone());
                    doc.encoding = Box::leak(effective.into_boxed_str());
                    doc.dirty = false;
                }
                *tab.editor.flags.doc_dir.borrow_mut() = file.parent().and_then(|p| p.path());
                // A session-chosen spell language persists on first save.
                if let Some(lang) = tab.editor.spell_language.borrow().clone() {
                    files::metadata::store_language(&file, &lang);
                }
                *tab.editor.spell_persist_target.borrow_mut() = Some(file.clone());
                self.update_title();
                self.update_tab_title(&tab);
                recents::add(&file);
                files::delete_snapshot_tab(self, &tab);
                self.cancel_snapshot_timer(&tab);
                // Re-arm the file monitor after a grace delay so our own
                // write does not trigger the external-change banner.
                {
                    let win = self.clone();
                    let f = file.clone();
                    glib::timeout_add_seconds_local(1, move || {
                        files::monitor::install_tab(&win, &tab, &f);
                        glib::ControlFlow::Break
                    });
                }
                crate::app::update_logout_inhibition();
            }
            Err(e) => {
                self.show_error(&i18n("Cannot save file"), &e);
                // Dirty state stays set; the progress bar already faded.
            }
        }
    }

    /// Unsaved-changes confirmation (`02-ui-layout.md` §11) for every
    /// dirty tab, in order. Returns true when the window may close.
    pub async fn confirm_close(&self) -> bool {
        for tab in self.tabs() {
            if !self.confirm_close_tab(&tab).await {
                return false;
            }
        }
        true
    }

    /// Unsaved-changes confirmation for one tab. Returns true when the
    /// tab may close.
    pub async fn confirm_close_tab(&self, tab: &Rc<TabState>) -> bool {
        if !tab.doc().dirty {
            return true;
        }
        let dialog = adw::MessageDialog::new(
            Some(&self.root),
            Some(&i18n("Save changes?")),
            Some(&i18n_f(
                "“{}” contains unsaved changes. If you close it, the changes will be lost.",
                &[&tab.doc().display_name()],
            )),
        );
        dialog.add_response("cancel", &i18n("Cancel"));
        dialog.set_close_response("cancel");
        dialog.add_response("discard", &i18n("Discard"));
        dialog.set_response_appearance("discard", adw::ResponseAppearance::Destructive);
        dialog.add_response("save", &i18n("Save"));
        dialog.set_response_appearance("save", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("save"));

        match dialog.choose_future().await.as_str() {
            "save" => {
                let file = tab.doc().file.clone();
                match file {
                    Some(file) => self.save_tab_to(tab.clone(), file),
                    None => files::save_as_dialog(self, &tab),
                }
                // A failed save aborts the close (dirty stays set).
                !tab.doc().dirty
            }
            "discard" => true,
            _ => false,
        }
    }

    pub fn show_error(&self, title: &str, body: &str) {
        let dialog = adw::AlertDialog::new(Some(title), Some(body));
        dialog.add_response("close", &i18n("Close"));
        dialog.set_default_response(Some("close"));
        dialog.present(Some(&self.root));
    }

    /// The editors' container's current width — what the adaptive column
    /// layout is sized against. Not the window's width in the beside
    /// and below preview layouts.
    pub fn editor_width(&self) -> i32 {
        self.tab_view.width()
    }

    pub fn present_window(&self) {
        self.root.present();
        if self.state.preview_active.get() {
            self.preview_on();
        }
        self.editor().view.grab_focus();
    }
}

fn save_menu_model() -> gio::Menu {
    let menu = gio::Menu::new();
    menu.append(Some(&i18n("Save As…")), Some("win.save-as"));
    let exports = gio::Menu::new();
    exports.append(Some(&i18n("Export as PDF")), Some("win.export-pdf"));
    exports.append(Some(&i18n("Export as HTML")), Some("win.export-html"));
    exports.append(Some(&i18n("Export as ODT")), Some("win.export-odt"));
    exports.append(Some(&i18n("Copy HTML")), Some("win.copy-html"));
    menu.append(Some(&i18n("Advanced Export…")), Some("win.export-advanced"));    menu.append_section(Some(&i18n("Export")), &exports);
    menu
}

fn preview_layout_button() -> gtk::MenuButton {
    let menu = gio::Menu::new();
    let section = gio::Menu::new();
    for mode in PreviewMode::ALL {
        section.append(
            Some(&match mode {
                PreviewMode::FullWidth => i18n("Preview Instead of Editor"),
                PreviewMode::HalfWidth => i18n("Preview Beside the Editor"),
                PreviewMode::HalfHeight => i18n("Preview Below the Editor"),
                PreviewMode::Windowed => i18n("Preview in Separate Window"),
            }),
            Some(&format!("win.preview-mode::{}", mode.as_str())),
        );
    }
    menu.append_section(None, &section);

    let off = gio::Menu::new();
    off.append(Some(&i18n("No Preview")), Some("win.preview-off"));
    menu.append_section(None, &off);

    let button = gtk::MenuButton::new();
    button.set_icon_name("view-right-pane-symbolic");
    button.set_tooltip_text(Some(&i18n("Preview Layout")));
    button.set_menu_model(Some(&menu));
    button
}

fn main_menu_button() -> gtk::MenuButton {
    let menu = gio::Menu::new();

    let appearance = gio::Menu::new();
    for scheme in ColorScheme::ALL {
        let label = match scheme {
            ColorScheme::System => i18n("System"),
            ColorScheme::Light => i18n("Light"),
            ColorScheme::Sepia => i18n("Sepia"),
            ColorScheme::Dark => i18n("Dark"),
        };
        appearance.append(
            Some(&label),
            Some(&format!("win.color-scheme::{}", scheme.as_str())),
        );
    }
    menu.append_section(Some(&i18n("Appearance")), &appearance);

    let modes = gio::Menu::new();
    modes.append(Some(&i18n("Hemingway Mode")), Some("win.hemingway-mode"));
    modes.append(Some(&i18n("Focus Mode")), Some("win.focus-mode"));
    modes.append(Some(&i18n("Code Mode")), Some("win.code-mode"));
    menu.append_section(None, &modes);

    let actions = gio::Menu::new();
    actions.append(Some(&i18n("New Window")), Some("app.new-window"));
    actions.append(Some(&i18n("Find and Replace")), Some("win.find-replace"));
    actions.append(Some(&i18n("Preferences")), Some("win.preferences"));
    menu.append_section(None, &actions);

    let help = gio::Menu::new();
    help.append(Some(&i18n("Open Tutorial")), Some("app.open-tutorial"));
    help.append(Some(&i18n("Keyboard Shortcuts")), Some("win.show-shortcuts"));
    help.append(Some(&i18n("About Griffel")), Some("win.about"));
    menu.append_section(None, &help);

    let button = gtk::MenuButton::new();
    button.set_icon_name("open-menu-symbolic");
    button.set_tooltip_text(Some(&i18n("Main Menu")));
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

fn build_toolbar(win: &AppWindow, row: &gtk::Box, row_revealer: &gtk::Revealer, toggle: &gtk::ToggleButton) {
    tool_button(win, row, "format-text-bold-symbolic", &i18n("Bold"), |w| {
        format::wrap(&w.editor().buffer, "**", &i18n("bold text"))
    });
    tool_button(win, row, "format-text-italic-symbolic", &i18n("Italic"), |w| {
        format::wrap(&w.editor().buffer, "_", &i18n("italic text"))
    });
    tool_button(win, row, "format-text-strikethrough-symbolic", &i18n("Strikethrough"), |w| {
        format::wrap(&w.editor().buffer, "~~", &i18n("strikethrough text"))
    });
    tool_button(win, row, "H", &i18n("Heading"), |w| {
        format::heading(&w.editor().buffer, None)
    });
    tool_button(win, row, "view-list-bullet-symbolic", &i18n("Bulleted List"), |w| {
        format::list(&w.editor().buffer, format::ListCommand::Bullet)
    });
    tool_button(win, row, "view-list-ordered-symbolic", &i18n("Numbered List"), |w| {
        format::list(&w.editor().buffer, format::ListCommand::Numbered)
    });
    tool_button(win, row, "checkbox-symbolic", &i18n("Checklist"), |w| {
        format::list(&w.editor().buffer, format::ListCommand::Checklist)
    });
    tool_button(win, row, "❝", &i18n("Block Quote"), |w| {
        format::block_quote(&w.editor().buffer)
    });
    tool_button(win, row, "</>", &i18n("Code"), |w| {
        format::code(&w.editor().buffer)
    });
    tool_button(win, row, "insert-link-symbolic", &i18n("Link"), |w| {
        format::link(&w.editor().buffer)
    });
    tool_button(win, row, "insert-image-symbolic", &i18n("Image"), |w| {
        let dir = w.editor().flags.doc_dir.borrow().clone();
        format::image(&w.editor().buffer, dir, &w.root);
    });
    tool_button(win, row, "▦", &i18n("Table"), |_w| {
        // The table button's popover is attached to this button; open it.
    });
    // Horizontal rule and fix-table trailing buttons.
    tool_button(win, row, "configure-line-symbolic", &i18n("Horizontal Rule"), |w| {
        format::horizontal_rule(&w.editor().buffer)
    });
    tool_button(win, row, "view-grid-symbolic", &i18n("Fix Table Alignment"), |w| {
        if !format::fix_table(&w.editor().buffer) {
            w.show_error(&i18n("No table"), &i18n("The cursor is not inside a table."));
        }
    });

    // The table button opens the size picker popover.
    let table_button = gtk::MenuButton::new();
    table_button.set_label("▦");
    table_button.set_tooltip_text(Some(&i18n("Table")));
    table_button.set_popover(Some(&table_size_popover(win)));
    table_button.add_css_class("flat");
    row.append(&table_button);

    // Chevron toggle: show/hide the row; state persisted (`02-ui-layout.md`
    // §5). The row slides out horizontally, so the arrow is horizontal too:
    // pan-end invites expanding the row, pan-start collapses it back. The
    // toggle trails the row (it sits right after it), so it moves with the
    // row's end.
    let visible = settings().boolean("toolbar-visible");
    toggle.set_active(visible);
    toggle.set_icon_name(if visible {
        "pan-start-symbolic"
    } else {
        "pan-end-symbolic"
    });
    toggle.set_tooltip_text(Some(if visible {
        "Hide formatting"
    } else {
        "Show formatting"
    }));
    row_revealer.set_reveal_child(visible);
    {
        let row_revealer = row_revealer.clone();
        toggle.connect_toggled(move |t| {
            let active = t.is_active();
            row_revealer.set_reveal_child(active);
            t.set_icon_name(if active {
                "pan-start-symbolic"
            } else {
                "pan-end-symbolic"
            });
            t.set_tooltip_text(Some(&if active {
                i18n("Hide formatting")
            } else {
                i18n("Show formatting")
            }));
            settings().set_boolean("toolbar-visible", active);
        });
    }
}

/// Size picker (up to 4 rows × 6 columns); hovering highlights the
/// selected rows×columns region and a label shows "R x C"
/// (`02-formatting.md` §7).
fn table_size_popover(win: &AppWindow) -> gtk::Popover {
    const MAX_ROWS: usize = 4;
    const MAX_COLS: usize = 6;
    let popover = gtk::Popover::new();
    let box_ = gtk::Box::new(gtk::Orientation::Vertical, 6);
    box_.set_margin_top(8);
    box_.set_margin_bottom(8);
    box_.set_margin_start(8);
    box_.set_margin_end(8);

    let label = gtk::Label::new(Some(&i18n("Insert Table")));
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
            let win = win.clone();
            let popover = popover.clone();
            let (rr, cc) = (r, c);
            cell.connect_clicked(move |_| {
                popover.popdown();
                format::insert_table(
                    &win.editor().buffer,
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
            label.set_text(&i18n("Insert Table"));
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

    /// Whether the active document is a source-code file (`05-…persistence.md`):
    /// Markdown preview and conversion do not apply to it.
    pub fn active_code_file(&self) -> bool {
        self.active_tab()
            .doc()
            .file
            .as_ref()
            .is_some_and(files::is_code_file)
    }

    pub fn preview_on(&self) {
        // Markdown preview does not apply to code files.
        if self.active_code_file() {
            return;
        }
        // Evaluate the document's security level for the current text.
        let tab = self.active_tab();
        let text = tab.editor.full_text();
        let is_new = tab.doc().file.is_none();
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
        self.detach(&self.tab_view);
        self.editor_page.append(&self.tab_view);
        self.content_stack.set_visible_child_name("editor");
    }

    fn detach_preview_window(&self) {
        // Take the window out of the cell before closing it: the close
        // request handler reads the same cell, and the `if let` form below
        // would hold the mutable borrow across close().
        let win = self.state.preview_window.borrow_mut().take();
        if let Some(win) = win {
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

    /// Place a pane divider at half the window. On the startup-restore
    /// path the layout switch runs before the window's first allocation,
    /// so the width read at that moment is not the real one — retry until
    /// it is.
    fn queue_pane_position(&self, pane: &gtk::Paned, horizontal: bool) {
        let win = self.clone();
        let pane = pane.clone();
        let tries = std::rc::Rc::new(std::cell::Cell::new(0));
        glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
            let tries = tries.get() + 1;
            let size = if horizontal {
                win.root.width()
            } else {
                win.root.height()
            };
            if size > 1 {
                pane.set_position(size / 2);
                glib::ControlFlow::Break
            } else if tries > 100 {
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
    }

    /// Arrange editor and preview per `mode` (§2, `02-ui-layout.md` §4).
    pub fn apply_preview_mode(&self, mode: PreviewMode) {
        self.detach(&self.tab_view);
        self.detach(&self.preview.root);
        self.detach_preview_window();

        let width = self.root.width().max(600);
        let height = self.root.height().max(400);
        match mode {
            PreviewMode::HalfWidth => {
                self.hpaned.set_start_child(Some(&self.tab_view));
                self.hpaned.set_end_child(Some(&self.preview.root));
                self.hpaned.set_position(width / 2);
                self.queue_pane_position(&self.hpaned, true);
                self.content_stack.set_visible_child_name("half-width");
            }
            PreviewMode::HalfHeight => {
                self.vpaned.set_start_child(Some(&self.tab_view));
                self.vpaned.set_end_child(Some(&self.preview.root));
                self.vpaned.set_position(height / 2);
                self.queue_pane_position(&self.vpaned, false);
                self.content_stack.set_visible_child_name("half-height");
            }
            PreviewMode::FullWidth => {
                self.editor_page.append(&self.preview.root);
                self.content_stack.set_visible_child_name("editor");
            }
            PreviewMode::Windowed => {
                self.editor_page.append(&self.tab_view);
                self.content_stack.set_visible_child_name("editor");
                let win = gtk::Window::new();
                win.set_title(Some(&format!(
                    "{} — Preview",
                    self.active_tab().doc().display_name()
                )));
                win.set_default_size(700, 700);
                win.set_child(Some(&self.preview.root));
                win.set_transient_for(Some(&self.root));
                {
                    let win_self = self.clone();
                    let preview_win = win.clone();
                    win.connect_close_request(move |_| {
                        // Only a close of the still-tracked preview window is
                        // the user turning the preview off. Mode switches call
                        // detach_preview_window() first, which untracks the
                        // window before closing it — that close must not
                        // deactivate the preview that is about to be
                        // rearranged.
                        let tracked = win_self
                            .state
                            .preview_window
                            .borrow()
                            .as_ref()
                            .is_some_and(|w| w.as_ptr() == preview_win.as_ptr());
                        if tracked && win_self.state.preview_active.get() {
                            win_self.preview_off();
                        }
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
                let text = win.editor().full_text();
                let doc_dir = win.editor().flags.doc_dir.borrow().clone();
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
    fn maybe_autohide_bars(&self, tab: &Rc<TabState>) {
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
        if !tab.editor.view.has_focus() {
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
        self.toolbar_cluster.set_visible(true);
        self.stats_button.set_visible(true);
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
            win.toolbar_cluster.set_opacity(o.clamp(0.0, 1.0));
            win.stats_button.set_opacity(o.clamp(0.0, 1.0));
            for tab in win.tabs() {
                tab.editor.set_bar_comp(v);
            }
            win.preview.root.set_margin_top(v);
            if i >= steps {
                if hiding {
                    win.toolbar_cluster.set_visible(false);
                    win.toolbar_cluster.set_opacity(1.0);
                    win.stats_button.set_visible(false);
                    win.stats_button.set_opacity(1.0);
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
            win.editor()
                .scroll
                .set_margin_start(dx.max(0));
            win.editor()
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
        let toast = adw::Toast::new(&i18n("Text cannot be deleted in Hemingway Mode"));
        toast.set_button_label(Some(&i18n("Learn More")));
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
            .heading(i18n("Hemingway Mode"))
            .body(i18n(
                "While Hemingway Mode is on, text can only be added, never deleted. \
Backspace, Delete, cutting, and replacing a selection do nothing. \
Undo still works. Toggle the mode with Ctrl+T or from the main menu.",
            ))
            .transient_for(&self.root)
            .modal(true)
            .build();
        dialog.add_response("ok", &i18n("OK"));
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
        let full = self.editor().full_text();
        let selection_text: Option<String> = self
            .editor()
            .buffer
            .selection_bounds()
            .map(|(s, e)| self.editor().buffer.text(&s, &e, true).into());
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
                (true, Some(sel)) => i18n_f(
                    "{} of {} {}",
                    &[&stat.value(&sel), &stat.value(&total), &stat.display_name()],
                ),
                _ => stat.label(&total),
            };
            win.stats_button.set_label(&label);
        });
    }
}
