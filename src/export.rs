//! Export (quick conversions via the external converter) (`08-export.md`).

use crate::window::AppWindow;
use gtk::prelude::*;
use gdk;
use std::fs;
use std::process::Command;

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
