//! Dialogs: Preferences, Keyboard Shortcuts, and About.

use adw::prelude::*;
use gtk::gio;

use crate::settings::{settings, InputFormat, PreviewSecurity};

pub fn show_preferences(parent: &adw::ApplicationWindow) {
    let dialog = adw::PreferencesDialog::new();

    let page = adw::PreferencesPage::new();
    page.set_title("Preferences");
    page.set_icon_name(Some("preferences-system-symbolic"));
    dialog.add(&page);

    let group = adw::PreferencesGroup::new();
    group.set_title("Editor &amp; Window");
    page.add(&group);

    // 1. Check spelling
    let spell_row = adw::SwitchRow::new();
    spell_row.set_title("Check spelling");
    spell_row.set_subtitle("Live spell checking in the editor");
    let _ = settings().inner().bind("spellcheck", &spell_row, "active");
    group.add(&spell_row);

    // 2. Auto-hide header bar
    let autohide_row = adw::SwitchRow::new();
    autohide_row.set_title("Auto-hide header bar");
    autohide_row.set_subtitle("Hide header and bottom bars while typing");
    let _ = settings().inner().bind("autohide-headerbar", &autohide_row, "active");
    group.add(&autohide_row);

    // 3. Large text
    let large_text_row = adw::SwitchRow::new();
    large_text_row.set_title("Large text");
    large_text_row.set_subtitle("Use a larger editor font and wider margins");
    let _ = settings().inner().bind("bigger-text", &large_text_row, "active");
    group.add(&large_text_row);

    let format_group = adw::PreferencesGroup::new();
    format_group.set_title("Formatting &amp; Preview");
    page.add(&format_group);

    // 4. Input format ComboRow
    let formats = InputFormat::ALL;
    let names: Vec<&str> = formats.iter().map(|f| f.display_name()).collect();
    let model = gtk::StringList::new(&names);
    let format_row = adw::ComboRow::new();
    format_row.set_title("Input format");
    format_row.set_subtitle("Flavor used by preview and export");
    format_row.set_model(Some(&model));

    let current_format = settings().input_format();
    if let Some(idx) = formats.iter().position(|f| *f == current_format) {
        format_row.set_selected(idx as u32);
    }
    {
        let format_row = format_row.clone();
        format_row.connect_selected_notify(move |row| {
            let idx = row.selected() as usize;
            if let Some(f) = formats.get(idx) {
                settings().set_input_format(*f);
            }
        });
    }

    // Help button suffix for input format docs
    let help_button = gtk::Button::from_icon_name("help-browser-symbolic");
    help_button.add_css_class("flat");
    help_button.set_valign(gtk::Align::Center);
    help_button.set_tooltip_text(Some("Open official documentation for this format"));
    {
        let format_row = format_row.clone();
        help_button.connect_clicked(move |_| {
            let idx = format_row.selected() as usize;
            if let Some(f) = formats.get(idx) {
                let _ = gio::AppInfo::launch_default_for_uri(
                    f.docs_url(),
                    None::<&gio::AppLaunchContext>,
                );
            }
        });
    }
    format_row.add_suffix(&help_button);
    format_group.add(&format_row);

    // 5. Preview security ComboRow
    let security_policies = PreviewSecurity::ALL;
    let sec_names: Vec<&str> = security_policies.iter().map(|p| match p {
        PreviewSecurity::Ask => "Always ask",
        PreviewSecurity::AlwaysRestricted => "Always use restricted",
        PreviewSecurity::AlwaysUnrestricted => "Always use unrestricted",
    }).collect();
    let sec_model = gtk::StringList::new(&sec_names);
    let security_row = adw::ComboRow::new();
    security_row.set_title("Preview security");
    security_row.set_subtitle("Policy applied when previewing files containing raw HTML");
    security_row.set_model(Some(&sec_model));

    let current_sec = settings().preview_security();
    if let Some(idx) = security_policies.iter().position(|p| *p == current_sec) {
        security_row.set_selected(idx as u32);
    }
    security_row.connect_selected_notify(move |row| {
        let idx = row.selected() as usize;
        if let Some(p) = security_policies.get(idx) {
            settings().set_preview_security(*p);
        }
    });
    format_group.add(&security_row);

    dialog.present(Some(parent));
}

pub fn show_shortcuts(parent: &adw::ApplicationWindow) {
    let win = gtk::ShortcutsWindow::builder()
        .transient_for(parent)
        .modal(true)
        .build();

    let section = gtk::ShortcutsSection::builder()
        .title("Keyboard Shortcuts")
        .max_height(10)
        .build();

    // General group
    let general = gtk::ShortcutsGroup::builder()
        .title("General")
        .build();
    add_shortcut(&general, "<primary>n", "New window");
    add_shortcut(&general, "<primary>o", "Open file");
    add_shortcut(&general, "<primary>s", "Save document");
    add_shortcut(&general, "<primary><shift>s", "Save As");
    add_shortcut(&general, "<primary>w", "Close document");
    add_shortcut(&general, "<primary>q", "Quit application");
    add_shortcut(&general, "<primary>comma", "Open preferences");
    add_shortcut(&general, "F7", "Toggle spell checking");
    section.append(&general);

    // Modes group
    let modes = gtk::ShortcutsGroup::builder()
        .title("Modes & Views")
        .build();
    add_shortcut(&modes, "<primary>d", "Toggle Focus Mode");
    add_shortcut(&modes, "<primary>t", "Toggle Hemingway Mode");
    add_shortcut(&modes, "<primary>p", "Toggle preview pane");
    add_shortcut(&modes, "F11", "Toggle fullscreen");
    section.append(&modes);

    // Find group
    let find = gtk::ShortcutsGroup::builder()
        .title("Find & Replace")
        .build();
    add_shortcut(&find, "<primary>f", "Find");
    add_shortcut(&find, "<primary>h", "Find and replace");
    section.append(&find);

    // Markdown group
    let md = gtk::ShortcutsGroup::builder()
        .title("Markdown Formatting")
        .build();
    add_shortcut(&md, "<primary>b", "Bold");
    add_shortcut(&md, "<primary>i", "Italic");
    add_shortcut(&md, "<primary><shift>d", "Strikethrough");
    add_shortcut(&md, "<primary>r", "Horizontal rule");
    add_shortcut(&md, "<primary>u", "Bulleted list item");
    add_shortcut(&md, "<primary>1", "Heading 1");
    add_shortcut(&md, "<primary>2", "Heading 2");
    add_shortcut(&md, "<primary>3", "Heading 3");
    add_shortcut(&md, "<primary>4", "Heading 4");
    section.append(&md);

    // Editing group
    let editing = gtk::ShortcutsGroup::builder()
        .title("Editing")
        .build();
    add_shortcut(&editing, "<primary>c", "Copy");
    add_shortcut(&editing, "<primary>x", "Cut");
    add_shortcut(&editing, "<primary>v", "Paste");
    add_shortcut(&editing, "<primary>a", "Select all");
    add_shortcut(&editing, "<primary>z", "Undo");
    add_shortcut(&editing, "<primary><shift>z", "Redo");
    add_shortcut(&editing, "Tab", "Indent / smart list");
    add_shortcut(&editing, "<shift>Tab", "Outdent / smart list");
    section.append(&editing);

    win.set_child(Some(&section));
    win.set_visible(true);
}

fn add_shortcut(group: &gtk::ShortcutsGroup, accel: &str, title: &str) {
    let s = gtk::ShortcutsShortcut::builder()
        .accelerator(accel)
        .title(title)
        .build();
    group.append(&s);
}

pub fn show_about(parent: &adw::ApplicationWindow) {
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
