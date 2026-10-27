//! The main document window: headerbar, editor area, bottom bar, search
//! bar and banners. One window holds exactly one document.

use adw::prelude::*;
use gtk::prelude::*;
use gtk::gio;

use crate::editor::Editor;
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
}

impl AppWindow {
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
        open_button.set_menu_model(Some(&recents_menu_placeholder()));

        let save_button = gtk::Button::new();
        save_button.set_label("Save");
        save_button.set_tooltip_text(Some("Save"));
        save_button.add_css_class("flat");
        let save_menu = gtk::MenuButton::new();
        save_menu.set_icon_name("pan-down-symbolic");
        save_menu.set_tooltip_text(Some("Save and Export"));
        save_menu.set_menu_model(Some(&save_menu_placeholder()));
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
        subtitle_label.set_ellipsize(pango::EllipsizeMode::Start);
        title_box.append(&title_label);
        title_box.append(&subtitle_label);
        headerbar.set_title_widget(Some(&title_box));

        // --- bottom bar ----------------------------------------------------
        let bottom_bar = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        bottom_bar.add_css_class("bottom-bar");
        bottom_bar.set_margin_start(6);
        bottom_bar.set_margin_end(6);
        bottom_bar.set_margin_bottom(6);

        let toolbar_row = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        toolbar_row.add_css_class("toolbar-row");
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

        let toolbar_view = adw::ToolbarView::new();
        toolbar_view.add_top_bar(&headerbar);
        toolbar_view.set_content(Some(&overlay));

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
        };

        win.install_actions();
        win
    }

    fn install_actions(&self) {
        let actions = gio::SimpleActionGroup::new();

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
                }
            });
        }
        actions.add_action(&preview_mode);

        for name in [
            "close",
            "open",
            "find",
            "find-replace",
            "save",
            "save-as",
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

    pub fn present_window(&self) {
        self.root.present();
        self.editor.view.grab_focus();
    }
}

fn recents_menu_placeholder() -> gio::Menu {
    let menu = gio::Menu::new();
    menu.append(Some("Open File…"), Some("win.open"));
    menu
}

fn save_menu_placeholder() -> gio::Menu {
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
