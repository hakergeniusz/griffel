//! Dialogs: Preferences, Keyboard Shortcuts, and About.

use adw::prelude::*;
use gtk::gio;

use crate::i18n::i18n;
use crate::settings::{settings, InputFormat, PreviewSecurity};

pub fn show_preferences(parent: &adw::ApplicationWindow) {
    let dialog = adw::PreferencesDialog::new();

    let page = adw::PreferencesPage::new();
    page.set_title(&i18n("Preferences"));
    page.set_icon_name(Some("preferences-system-symbolic"));
    dialog.add(&page);

    let group = adw::PreferencesGroup::new();
    group.set_title(&i18n("Editor &amp; Window"));
    page.add(&group);

    // 1. Check spelling
    let spell_row = adw::SwitchRow::new();
    spell_row.set_title(&i18n("Check spelling"));
    spell_row.set_subtitle(&i18n("Live spell checking in the editor"));
    let _ = settings().inner().bind("spellcheck", &spell_row, "active").build();
    group.add(&spell_row);

    // 2. Auto-hide header bar
    let autohide_row = adw::SwitchRow::new();
    autohide_row.set_title(&i18n("Auto-hide header bar"));
    autohide_row.set_subtitle(&i18n("Hide header and bottom bars while typing"));
    let _ = settings().inner().bind("autohide-headerbar", &autohide_row, "active").build();
    group.add(&autohide_row);

    // 3. Large text
    let large_text_row = adw::SwitchRow::new();
    large_text_row.set_title(&i18n("Large text"));
    large_text_row.set_subtitle(&i18n("Use a larger editor font and wider margins"));
    let _ = settings().inner().bind("bigger-text", &large_text_row, "active").build();
    group.add(&large_text_row);

    // 4. Autosave (user-added setting; not in `03/10`)
    let autosave_row = adw::SwitchRow::new();
    autosave_row.set_title(&i18n("Autosave"));
    autosave_row.set_subtitle(&i18n("Save the document five seconds after you stop typing"));
    let _ = settings().inner().bind("autosave", &autosave_row, "active").build();
    group.add(&autosave_row);

    let format_group = adw::PreferencesGroup::new();
    format_group.set_title(&i18n("Formatting &amp; Preview"));
    page.add(&format_group);

    // 5. Input format ComboRow
    let formats = InputFormat::ALL;
    let names: Vec<&str> = formats.iter().map(|f| f.display_name()).collect();
    let model = gtk::StringList::new(&names);
    let format_row = adw::ComboRow::new();
    format_row.set_title(&i18n("Input format"));
    format_row.set_subtitle(&i18n("Flavor used by preview and export"));
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
    help_button.set_tooltip_text(Some(&i18n("Open official documentation for this format")));
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

    // 6. Preview security ComboRow
    let security_policies = PreviewSecurity::ALL;
    let sec_names: Vec<String> = security_policies.iter().map(|p| match p {
        PreviewSecurity::Ask => i18n("Always ask"),
        PreviewSecurity::AlwaysRestricted => i18n("Always use restricted"),
        PreviewSecurity::AlwaysUnrestricted => i18n("Always use unrestricted"),
    }).collect();
    let sec_model = gtk::StringList::new(
        &sec_names.iter().map(|s| s.as_str()).collect::<Vec<&str>>(),
    );
    let security_row = adw::ComboRow::new();
    security_row.set_title(&i18n("Preview security"));
    security_row.set_subtitle(&i18n("Policy applied when previewing files containing raw HTML"));
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
        .title(i18n("Keyboard Shortcuts"))
        .max_height(10)
        .build();

    // General group
    let general = gtk::ShortcutsGroup::builder()
        .title(i18n("General"))
        .build();
    add_shortcut(&general, "<primary>n", &i18n("New tab"));
    add_shortcut(&general, "<primary><shift>n", &i18n("New window"));
    add_shortcut(&general, "<primary>o", &i18n("Open file"));
    add_shortcut(&general, "<primary>s", &i18n("Save document"));
    add_shortcut(&general, "<primary><shift>s", &i18n("Save As"));
    add_shortcut(&general, "<primary>w", &i18n("Close tab"));
    add_shortcut(&general, "<primary><shift>w", &i18n("Close window"));
    add_shortcut(&general, "<primary>q", &i18n("Quit application"));
    add_shortcut(&general, "<primary>comma", &i18n("Open preferences"));
    add_shortcut(&general, "F7", &i18n("Toggle spell checking"));
    section.append(&general);

    // Modes group
    let modes = gtk::ShortcutsGroup::builder()
        .title(i18n("Modes & Views"))
        .build();
    add_shortcut(&modes, "<primary>d", &i18n("Toggle Focus Mode"));
    add_shortcut(&modes, "<primary>t", &i18n("Toggle Hemingway Mode"));
    add_shortcut(&modes, "<primary>p", &i18n("Toggle preview pane"));
    add_shortcut(&modes, "F8", &i18n("Toggle Code Mode"));
    add_shortcut(&modes, "F11", &i18n("Toggle fullscreen"));
    section.append(&modes);

    // Find group
    let find = gtk::ShortcutsGroup::builder()
        .title(i18n("Find & Replace"))
        .build();
    add_shortcut(&find, "<primary>f", &i18n("Find"));
    add_shortcut(&find, "<primary>h", &i18n("Find and replace"));
    section.append(&find);

    // Markdown group
    let md = gtk::ShortcutsGroup::builder()
        .title(i18n("Markdown Formatting"))
        .build();
    add_shortcut(&md, "<primary>b", &i18n("Bold"));
    add_shortcut(&md, "<primary>i", &i18n("Italic"));
    add_shortcut(&md, "<primary><shift>d", &i18n("Strikethrough"));
    add_shortcut(&md, "<primary>r", &i18n("Horizontal rule"));
    add_shortcut(&md, "<primary>u", &i18n("Bulleted list item"));
    add_shortcut(&md, "<primary>1", &i18n("Heading 1"));
    add_shortcut(&md, "<primary>2", &i18n("Heading 2"));
    add_shortcut(&md, "<primary>3", &i18n("Heading 3"));
    add_shortcut(&md, "<primary>4", &i18n("Heading 4"));
    section.append(&md);

    // Editing group
    let editing = gtk::ShortcutsGroup::builder()
        .title(i18n("Editing"))
        .build();
    add_shortcut(&editing, "<primary>c", &i18n("Copy"));
    add_shortcut(&editing, "<primary>x", &i18n("Cut"));
    add_shortcut(&editing, "<primary>v", &i18n("Paste"));
    add_shortcut(&editing, "<primary>a", &i18n("Select all"));
    add_shortcut(&editing, "<primary>z", &i18n("Undo"));
    add_shortcut(&editing, "<primary><shift>z", &i18n("Redo"));
    add_shortcut(&editing, "Tab", &i18n("Indent / smart list"));
    add_shortcut(&editing, "<shift>Tab", &i18n("Outdent / smart list"));
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
    // Standard GNOME about window (`02-ui-layout.md` §11). The Details
    // page carries the platform diagnostics (toolkit versions, OS,
    // windowing system, renderer); donation and translation links live in
    // the AppStream metainfo (logged deviation: AdwAboutWindow has no
    // donate/translate slots).
    let dialog = adw::AboutWindow::new();
    dialog.set_transient_for(Some(parent));
    dialog.set_modal(true);
    dialog.set_application_name("Griffel");
    dialog.set_application_icon(crate::app::ICON_NAME);
    dialog.set_version(env!("CARGO_PKG_VERSION"));
    dialog.set_comments(&i18n("A distraction-free Markdown editor"));
    dialog.set_developer_name("hakergeniusz");
    dialog.set_copyright("© 2026 hakergeniusz");
    dialog.set_license_type(gtk::License::MitX11);
    dialog.set_website("https://github.com/hakergeniusz/griffel");
    dialog.set_issue_url("https://github.com/hakergeniusz/griffel/issues");
    dialog.set_release_notes_version(env!("CARGO_PKG_VERSION"));
    dialog.set_release_notes(&i18n(
        "<p>Initial release: editing, in-place highlighting, live preview,\n         \
         Focus and Hemingway modes, spell checking, peek popovers, export,\n         \
         snapshots and recovery.</p>",
    ));
    dialog.set_visible(true);
}
