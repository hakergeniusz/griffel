//! Export (quick conversions via the external converter) (`08-export.md`).

use crate::window::AppWindow;
use gtk::prelude::*;
use adw::prelude::*;
use gdk;
use std::fs;
use std::process::Command;

pub fn has_tex() -> bool {
    std::process::Command::new("xelatex")
        .arg("--version")
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn run_pandoc(input: &str, output: &std::path::Path, extra: &[&str]) -> Result<(), String> {
    let input_path = std::env::temp_dir().join(format!("griffel-export-{}.md", std::process::id()));
    fs::write(&input_path, input.as_bytes()).map_err(|e| e.to_string())?;
    let mut cmd = Command::new("pandoc");
    cmd.arg(&input_path);
    cmd.args(extra);
    cmd.arg("-o").arg(output);
    let status = cmd.status().map_err(|e| format!("pandoc: {}", e))?;
    let _ = fs::remove_file(&input_path);
    if status.success() {
        Ok(())
    } else {
        Err(format!("converter exited with status {}", status))
    }
}

fn pandoc_html(input: &str, output: &std::path::Path) -> Result<(), String> {
    run_pandoc(input, output, &["--standalone", "--embed-resources"])
}

fn pandoc_pdf(input: &str, output: &std::path::Path) -> Result<(), String> {
    run_pandoc(input, output, &["--pdf-engine=xelatex", "-V", "geometry:a4paper"])
}

fn pandoc_odt(input: &str, output: &std::path::Path) -> Result<(), String> {
    run_pandoc(input, output, &[])
}

fn export_dialog(
    win: &AppWindow,
    title: &str,
    filter_name: &str,
    pattern: &str,
    convert: impl FnOnce(&str, &std::path::Path) -> Result<(), String> + 'static,
) {
    let win = (*win).clone();
    let dialog = gtk::FileDialog::new();
    dialog.set_title(title);
    let filter = gtk::FileFilter::new();
    filter.set_name(Some(filter_name));
    filter.add_pattern(pattern);
    let filters = gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&filter);
    dialog.set_filters(Some(&filters));

    let text = win.editor.full_text();
    let parent = win.root.clone();
    dialog.save(
        Some(&parent),
        gio::Cancellable::NONE,
        move |result| match result {
            Ok(file) => {
                if let Some(path) = file.path() {
                    match (convert)(&text, &path) {
                        Ok(()) => {}
                        Err(e) => win.show_error("Export failed", &e),
                    }
                }
            }
            Err(_) => {}
        },
    );
}

pub fn quick_pdf(win: &AppWindow) {
    export_dialog(win, "Export as PDF", "PDF", "*.pdf", pandoc_pdf);
}

pub fn quick_html(win: &AppWindow) {
    export_dialog(win, "Export as HTML", "HTML", "*.html", pandoc_html);
}

pub fn quick_odt(win: &AppWindow) {
    export_dialog(win, "Export as ODT", "ODT", "*.odt", pandoc_odt);
}

pub fn copy_html(win: &AppWindow) {
    let text = win.editor.full_text();
    let tmp = std::env::temp_dir().join(format!("griffel-copyhtml-{}.md", std::process::id()));
    if fs::write(&tmp, text.as_bytes()).is_err() {
        return;
    }
    let output = std::process::Command::new("pandoc")
        .arg(&tmp)
        .arg("-t")
        .arg("html")
        .output();
    let _ = fs::remove_file(&tmp);
    if let Ok(out) = output {
        if out.status.success() {
            let html = String::from_utf8_lossy(&out.stdout).to_string();
            let bytes = glib::Bytes::from(html.as_bytes());
            let provider = gdk::ContentProvider::for_bytes("text/html", &bytes);
            let _ = win.root.clipboard().set_content(Some(&provider));
        }
    }
}



pub fn advanced_dialog(win: &AppWindow) {
    let win = win.clone();
    let dialog = gtk::Window::builder()
        .title("Advanced Export")
        .modal(true)
        .transient_for(&win.root)
        .default_width(420)
        .default_height(320)
        .build();

    let outer = gtk::Box::new(gtk::Orientation::Vertical, 12);
    outer.set_margin_top(18);
    outer.set_margin_bottom(18);
    outer.set_margin_start(18);
    outer.set_margin_end(18);

    let formats = ["PDF", "HTML", "ODT", "LaTeX PDF"];
    let model = gtk::StringList::new(&formats);
    let combo = adw::ComboRow::new();
    combo.set_title("Format");
    combo.set_model(Some(&model));
    combo.set_selected(0);

    let options_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let standalone = gtk::CheckButton::with_label("Standalone document");
    standalone.set_active(true);
    let embed = gtk::CheckButton::with_label("Embed resources");
    embed.set_active(true);
    options_box.append(&standalone);
    options_box.append(&embed);

    let guidance = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let glabel = gtk::Label::new(Some("TeX is not installed. Install a TeX distribution (e.g. TeX Live) to enable LaTeX-based export."));
    glabel.set_wrap(true);
    guidance.append(&glabel);
    guidance.set_visible(false);

    outer.append(&combo);
    outer.append(&options_box);
    outer.append(&guidance);

    let export_btn = gtk::Button::with_label("Export…");
    export_btn.add_css_class("suggested-action");
    outer.append(&export_btn);

    dialog.set_child(Some(&outer));

    let options_box_c = options_box.clone();
    let guidance_c = guidance.clone();
    combo.connect_selected_notify(move |row| {
        let is_latex = row.selected() == 3;
        if is_latex && !has_tex() {
            options_box_c.set_visible(false);
            guidance_c.set_visible(true);
        } else {
            options_box_c.set_visible(true);
            guidance_c.set_visible(false);
        }
    });

    let dialog_c = dialog.clone();
    let combo_e = combo.clone();
    let win_e = win.clone();
    export_btn.connect_clicked(move |_| {
        match combo_e.selected() {
            0 => quick_pdf(&win_e),
            1 => quick_html(&win_e),
            2 => quick_odt(&win_e),
            3 => quick_pdf(&win_e),
            _ => {}
        }
        dialog_c.close();
    });

    dialog.present();
}
