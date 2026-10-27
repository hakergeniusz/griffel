//! Recently-used files (`05-file-formats-and-persistence.md` §3,
//! `02-ui-layout.md` §8): the platform recently-used store, filtered to
//! Markdown files, with per-entry removal and case-insensitive search.

use gtk::prelude::*;
use gtk::gio;

/// Register a successfully loaded/saved file in the recently-used store.
pub fn add(file: &gio::File) {
    let manager = gtk::RecentManager::default();
    let _ = manager.add_item(&file.uri());
}

/// Recently-used Markdown files that still exist, most recent first:
/// `(uri, display_name, folder_path)`.
pub fn markdown_entries() -> Vec<(String, String, String)> {
    let manager = gtk::RecentManager::default();
    let mut out = Vec::new();
    for item in manager.items() {
        let uri = item.uri().to_string();
        let mime = item.mime_type().to_string();
        if !is_markdown_mime(&mime) && !uri.ends_with(".md") {
            continue;
        }
        let file = gio::File::for_uri(&uri);
        // Prune entries whose files no longer exist (remote URIs: keep).
        if file.is_native() && !file.query_exists(gio::Cancellable::NONE) {
            let _ = manager.remove_item(&uri);
            continue;
        }
        let name = file
            .basename()
            .map(|b| b.display().to_string())
            .unwrap_or_else(|| uri.clone());
        let folder = file
            .parent()
            .and_then(|p| p.path())
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        out.push((uri, name, folder));
    }
    out
}

pub fn is_markdown_mime(mime: &str) -> bool {
    matches!(mime, "text/markdown" | "text/x-markdown")
}

/// Build the recents popover shown by the Open headerbar button.
pub fn build_popover(on_open_file: impl Fn() + 'static + Clone) -> gtk::Popover {
    let popover = gtk::Popover::new();
    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    root.set_size_request(360, -1);

    let search = gtk::SearchEntry::new();
    search.set_margin_top(6);
    search.set_margin_bottom(6);
    search.set_margin_start(6);
    search.set_margin_end(6);
    root.append(&search);

    let scrolled = gtk::ScrolledWindow::new();
    scrolled.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scrolled.set_max_content_height(420);
    scrolled.set_propagate_natural_height(true);
    root.append(&scrolled);

    let list = gtk::ListBox::new();
    list.add_css_class("navigation-sidebar");
    list.set_selection_mode(gtk::SelectionMode::None);
    scrolled.set_child(Some(&list));

    let empty = gtk::Box::new(gtk::Orientation::Vertical, 12);
    empty.set_margin_top(24);
    empty.set_margin_bottom(24);
    empty.set_margin_start(24);
    empty.set_margin_end(24);
    empty.set_valign(gtk::Align::Center);
    let empty_icon = gtk::Image::from_icon_name("document-open-recent-symbolic");
    empty_icon.set_pixel_size(48);
    let empty_label = gtk::Label::new(Some("No recently opened documents"));
    empty_label.add_css_class("dim-label");
    let open_btn = gtk::Button::with_label("Open File…");
    open_btn.add_css_class("pill");
    {
        let on_open_file = on_open_file.clone();
        let popover = popover.clone();
        open_btn.connect_clicked(move |_| {
            popover.popdown();
            on_open_file();
        });
    }
    empty.append(&empty_icon);
    empty.append(&empty_label);
    empty.append(&open_btn);

    // The open file button is always available at the bottom of the list.
    let open_row_btn = gtk::Button::with_label("Open File…");
    open_row_btn.add_css_class("flat");
    open_row_btn.set_halign(gtk::Align::Fill);
    {
        let on_open_file = on_open_file.clone();
        let popover = popover.clone();
        open_row_btn.connect_clicked(move |_| {
            popover.popdown();
            on_open_file();
        });
    }
    root.append(&open_row_btn);

    // Populate and refilter.
    let populate = {
        let list = list.clone();
        let empty = empty.clone();
        let search = search.clone();
        let popover = popover.clone();
        move || {
            while let Some(child) = list.first_child() {
                list.remove(&child);
            }
            let filter = search.text().to_lowercase();
            let mut shown = 0usize;
            for (uri, name, folder) in markdown_entries() {
                if !filter.is_empty() && !name.to_lowercase().contains(&filter) {
                    continue;
                }
                shown += 1;
                let row = recent_row(&popover, &uri, &name, &folder);
                list.append(&row);
            }
            empty.set_visible(shown == 0);
            scrolled.set_visible(shown > 0);
        }
    };
    populate();
    {
        let populate = populate.clone();
        popover.connect_closed(move |_| populate());
    }
    {
        let populate = populate.clone();
        search.connect_search_changed(move |_| populate());
    }

    popover.set_child(Some(&root));
    popover
}

fn recent_row(
    popover: &gtk::Popover,
    uri: &str,
    name: &str,
    folder: &str,
) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    row.set_margin_start(6);
    row.set_margin_end(6);
    row.set_margin_top(3);
    row.set_margin_bottom(3);

    let text = gtk::Box::new(gtk::Orientation::Vertical, 1);
    text.set_hexpand(true);
    let name_label = gtk::Label::new(Some(name));
    name_label.set_halign(gtk::Align::Start);
    name_label.set_ellipsize(pango::EllipsizeMode::Middle);
    let folder_label = gtk::Label::new(Some(folder));
    folder_label.set_halign(gtk::Align::Start);
    folder_label.set_ellipsize(pango::EllipsizeMode::Middle);
    folder_label.add_css_class("caption");
    folder_label.add_css_class("dim-label");
    text.append(&name_label);
    text.append(&folder_label);
    row.append(&text);

    let activate = gtk::GestureClick::new();
    {
        let popover = popover.clone();
        let uri = uri.to_owned();
        activate.connect_released(move |_, _, _, _| {
            popover.popdown();
            // Route through the application-level open so an existing
            // window showing the file is focused instead.
            gtk::Application::default()
                .open(&[gio::File::for_uri(&uri)], "");
        });
    }
    row.add_controller(activate);

    let remove = gtk::Button::from_icon_name("user-trash-symbolic");
    remove.add_css_class("flat");
    remove.set_tooltip_text(Some("Remove from Recent"));
    remove.set_valign(gtk::Align::Center);
    {
        let uri = uri.to_owned();
        let row = row.clone();
        remove.connect_clicked(move |_| {
            let _ = gtk::RecentManager::default().remove_item(&uri);
            row.set_visible(false);
        });
    }
    row.append(&remove);
    row
}
