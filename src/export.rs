//! Export (`03-behavior/08-export.md`): quick exports, the advanced
//! export dialog with the full format list and option matrix, missing-TeX
//! guidance, the reveal.js folder export, and Copy HTML. All conversions
//! run through the external converter; nothing is bundled from it.

use crate::settings::{settings, ColorScheme, InputFormat};
use crate::window::AppWindow;
use adw::prelude::*;
use gdk;
use gtk::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

// ---------------------------------------------------------------------------
// Converter availability
// ---------------------------------------------------------------------------

pub fn has_tex() -> bool {
    Command::new("xelatex")
        .arg("--version")
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn has_typst() -> bool {
    Command::new("typst")
        .arg("--version")
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Resources and temp files
// ---------------------------------------------------------------------------

fn resource_bytes(path: &str) -> Result<Vec<u8>, String> {
    gio::resources_lookup_data(
        &format!("/io/github/hakergeniusz/Griffel/{path}"),
        gio::ResourceLookupFlags::NONE,
    )
    .map(|b| b.to_vec())
    .map_err(|e| format!("missing bundled resource {path}: {e}"))
}

/// Materialize a bundled resource as a temp file: pandoc only accepts
/// template/reference paths, not resource URIs.
fn extract_resource(path: &str, name: &str) -> Result<PathBuf, String> {
    let bytes = resource_bytes(path)?;
    let out = std::env::temp_dir().join(format!("griffel-{}-{name}", std::process::id()));
    fs::write(&out, bytes).map_err(|e| e.to_string())?;
    Ok(out)
}

// ---------------------------------------------------------------------------
// Format table (`08-export.md` §2)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    TypstPdf,
    LatexPdf,
    ReferenceDoc,
    Reveal,
    DzSlides,
    PlainHtml,
    Other,
}

#[derive(Clone, Copy)]
pub struct FormatDef {
    pub name: &'static str,
    pub ext: &'static str,
    pub target: &'static str,
    pub needs_tex: bool,
    pub kind: Kind,
    // Option visibility, `08-export.md` §2.1.
    pub standalone: bool,
    pub toc: bool,
    pub number_sections: bool,
    pub page_size: bool,
    pub slide_size: bool,
    pub self_contained: bool,
    pub highlight: bool,
    pub incremental: bool,
}

/// All-flags-off base the `fmt!` entries start from.
const BASE_FORMAT: FormatDef = FormatDef {
    name: "",
    ext: "",
    target: "",
    needs_tex: false,
    kind: Kind::Other,
    standalone: false,
    toc: false,
    number_sections: false,
    page_size: false,
    slide_size: false,
    self_contained: false,
    highlight: false,
    incremental: false,
};

macro_rules! fmt {
    ($name:literal, $ext:literal, $target:literal, $kind:expr,
     $($flag:ident),* $(,)?) => {
        FormatDef {
            name: $name,
            ext: $ext,
            target: $target,
            kind: $kind,
            $($flag: true,)*
            ..BASE_FORMAT
        }
    };
}

pub const HIGHLIGHT_STYLES: [&str; 7] = [
    "pygments",
    "kate",
    "monochrome",
    "espresso",
    "zenburn",
    "haddock",
    "tango",
];

pub static FORMATS: [FormatDef; 19] = [
    fmt!("PDF (Typst engine)", "pdf", "typst", Kind::TypstPdf, toc, number_sections, page_size),
    fmt!("PDF (LaTeX/XeLaTeX engine)", "pdf", "latex", Kind::LatexPdf, needs_tex, toc, number_sections, page_size, highlight),
    fmt!("LibreOffice Text Document", "odt", "odt", Kind::ReferenceDoc, page_size),
    fmt!("Microsoft Word", "docx", "docx", Kind::ReferenceDoc, page_size),
    fmt!("EPUB v3", "epub", "epub3", Kind::Other, toc),
    fmt!("HTML5 Slideshow (reveal.js)", "html", "revealjs", Kind::Reveal, toc, incremental),
    fmt!("LaTeX Beamer Slideshow, PDF", "pdf", "beamer", Kind::LatexPdf, needs_tex, toc, number_sections, page_size, slide_size, highlight, incremental),
    fmt!("LaTeX Beamer Slideshow, TeX source", "tex", "beamer", Kind::Other, standalone, toc, number_sections, page_size, slide_size, highlight, incremental),
    fmt!("HTML5 Slideshow (DZSlides)", "html", "html5", Kind::DzSlides, incremental),
    fmt!("LaTeX", "tex", "latex", Kind::Other, standalone, toc, number_sections, page_size, highlight),
    fmt!("HTML", "html", "html5", Kind::PlainHtml, standalone, toc, number_sections, self_contained, highlight),
    fmt!("MediaWiki Markup", "txt", "mediawiki", Kind::Other, standalone),
    fmt!("DokuWiki Markup", "txt", "dokuwiki", Kind::Other, standalone),
    fmt!("ConTeXt", "tex", "context", Kind::Other, standalone, toc, number_sections, page_size),
    fmt!("Textile", "txt", "textile", Kind::Other, standalone),
    fmt!("reStructuredText", "txt", "rst", Kind::Other, standalone),
    fmt!("Texinfo", "texi", "texinfo", Kind::Other, standalone, toc, number_sections),
    fmt!("Rich Text Format", "rtf", "rtf", Kind::Other, standalone),
    fmt!("Groff Man", "man", "man", Kind::Other, standalone),
];

// ---------------------------------------------------------------------------
// Options (`08-export.md` §2.1)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
pub enum PageSize {
    A4,
    Letter,
}

#[derive(Clone)]
pub struct Options {
    pub standalone: bool,
    pub rel_to_abs: bool,
    pub toc: bool,
    pub number_sections: bool,
    pub page_size: PageSize,
    pub wide_slides: bool,
    pub self_contained: bool,
    pub highlight: bool,
    pub highlight_style: usize,
    pub incremental: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            standalone: true,
            rel_to_abs: false,
            toc: false,
            number_sections: false,
            page_size: PageSize::A4,
            wide_slides: false,
            self_contained: false,
            highlight: true,
            highlight_style: 0,
            incremental: false,
        }
    }
}

/// Paths of the temp-file resources a conversion needs.
#[derive(Default)]
pub struct Assets {
    pub typst_template: Option<PathBuf>,
    pub odt_a4: Option<PathBuf>,
    pub odt_letter: Option<PathBuf>,
    pub docx_a4: Option<PathBuf>,
    pub docx_letter: Option<PathBuf>,
}

impl Assets {
    /// Extract the bundled template/reference documents to temp files.
    pub fn materialize() -> Result<Assets, String> {
        Ok(Assets {
            typst_template: Some(extract_resource(
                "export/typst-template.typ",
                "typst-template.typ",
            )?),
            odt_a4: Some(extract_resource(
                "export/reference-odt-a4.odt",
                "reference-odt-a4.odt",
            )?),
            odt_letter: Some(extract_resource(
                "export/reference-odt-letter.odt",
                "reference-odt-letter.odt",
            )?),
            docx_a4: Some(extract_resource(
                "export/reference-docx-a4.docx",
                "reference-docx-a4.docx",
            )?),
            docx_letter: Some(extract_resource(
                "export/reference-docx-letter.docx",
                "reference-docx-letter.docx",
            )?),
        })
    }
}

/// Rewrite relative image/link targets in `text` to absolute paths
/// anchored at `dir` (§2.1 "Relative→absolute paths"). URLs, absolute
/// paths, and `#anchors` are left untouched; only inline targets are
/// rewritten (reference definitions are rare in this app's documents).
pub fn rewrite_relative_to_absolute(text: &str, dir: &Path) -> String {
    use regex::Regex;
    let re = Regex::new(r"(\]\()([^)\s]+)(\))").unwrap();
    re.replace_all(text, |caps: &regex::Captures| {
        let target = &caps[2];
        let skip = target.is_empty()
            || target.starts_with('#')
            || target.starts_with('/')
            || target.contains("://")
            || target.starts_with("mailto:")
            || target.starts_with("data:");
        if skip {
            return caps[0].to_string();
        }
        let absolute = normalize(&dir.join(target));
        format!("]({})", absolute.display())
    })
    .into_owned()
}

/// Lexically resolve `.` and `..` components of an absolute path.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::from("/");
    for c in path.components() {
        match c {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Build the pandoc argument list for `fmt`/`opts`. Pure so the option
/// matrix is unit-testable; the caller prepends the input path.
pub fn pandoc_args(
    fmt: &FormatDef,
    opts: &Options,
    assets: &Assets,
    flavor: InputFormat,
    output: &Path,
) -> Vec<String> {
    let mut a: Vec<String> = vec![
        "--from".into(),
        flavor.pandoc_format(false),
        "--to".into(),
        fmt.target.into(),
    ];

    match fmt.kind {
        Kind::TypstPdf => {
            if let Some(t) = &assets.typst_template {
                a.push("--template".into());
                a.push(t.display().to_string());
            }
            a.push("--pdf-engine=typst".into());
            a.extend(["-V".into(), papersize_var(opts)]);
        }
        Kind::LatexPdf => {
            a.extend(["--pdf-engine=xelatex".into(), "-V".into(), papersize_var(opts)]);
        }
        Kind::ReferenceDoc => {
            let doc = match (fmt.ext, opts.page_size) {
                ("odt", PageSize::A4) => &assets.odt_a4,
                ("odt", PageSize::Letter) => &assets.odt_letter,
                (_, PageSize::A4) => &assets.docx_a4,
                (_, PageSize::Letter) => &assets.docx_letter,
            };
            if let Some(d) = doc {
                a.push("--reference-doc".into());
                a.push(d.display().to_string());
            }
        }
        Kind::Reveal => {
            a.push("--standalone".into());
            a.extend(["-V".into(), "revealjs-url=reveal.js".into()]);
        }
        Kind::DzSlides => {
            // The DZSlides template ships with the converter; fetch it at
            // export time instead of bundling anything from it.
            a.push("--standalone".into());
            a.extend(["--template".into(), "$DZSLIDES$".into()]);
        }
        Kind::PlainHtml => {
            if opts.self_contained {
                a.push("--standalone".into());
                a.push("--embed-resources".into());
            } else if opts.standalone {
                a.push("--standalone".into());
            }
        }
        Kind::Other => {
            if opts.standalone {
                a.push("--standalone".into());
            }
        }
    }

    if opts.toc && fmt.toc {
        a.push("--toc".into());
    }
    if opts.number_sections && fmt.number_sections {
        a.push("--number-sections".into());
    }
    if opts.incremental && fmt.incremental {
        a.push("-i".into());
    }
    if opts.highlight && fmt.highlight {
        a.push("--highlight-style".into());
        a.push(HIGHLIGHT_STYLES[opts.highlight_style.min(6)].into());
    }
    if fmt.slide_size && opts.wide_slides {
        a.extend(["-V".into(), "aspectratio=169".into()]);
    }
    a.push("-o".into());
    a.push(output.display().to_string());
    a
}

fn papersize_var(opts: &Options) -> String {
    format!(
        "papersize={}",
        match opts.page_size {
            PageSize::A4 => "a4",
            PageSize::Letter => "letter",
        }
    )
}

/// Fetch the converter's bundled DZSlides template into a temp file at
/// export time (it is used, not redistributed).
fn dzslides_template() -> Result<PathBuf, String> {
    let out = Command::new("pandoc")
        .arg("-D")
        .arg("dzslides")
        .output()
        .map_err(|e| format!("pandoc: {e}"))?;
    if !out.status.success() {
        return Err("could not obtain the DZSlides template".into());
    }
    let path = std::env::temp_dir().join(format!("griffel-{}-dzslides.tpl", std::process::id()));
    fs::write(&path, &out.stdout).map_err(|e| e.to_string())?;
    Ok(path)
}

// ---------------------------------------------------------------------------
// Conversion execution
// ---------------------------------------------------------------------------

fn run_pandoc(input: &str, output: &Path, extra: &[String]) -> Result<(), String> {
    let input_path =
        std::env::temp_dir().join(format!("griffel-export-{}.md", std::process::id()));
    fs::write(&input_path, input.as_bytes()).map_err(|e| e.to_string())?;
    let mut cmd = Command::new("pandoc");
    cmd.arg(&input_path);
    cmd.args(extra);
    cmd.arg("-o").arg(output);
    let status = cmd.status().map_err(|e| format!("pandoc: {e}"))?;
    let _ = fs::remove_file(&input_path);
    if status.success() {
        Ok(())
    } else {
        Err(format!("converter exited with status {status}"))
    }
}

/// Copy the bundled reveal.js library into `folder/reveal.js/` so the
/// exported HTML's relative `revealjs-url` resolves (§2.2).
fn extract_reveal_library(folder: &Path) -> Result<(), String> {
    let manifest = String::from_utf8_lossy(&resource_bytes("export/reveal-files.txt")?)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    for rel in manifest {
        let bytes = resource_bytes(&format!("export/{rel}"))?;
        let target = folder.join(&rel);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::write(&target, bytes).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Run one export synchronously with a busy cursor (the spec keeps
/// conversion synchronous and shows the platform busy state).
fn run_conversion(win: &AppWindow, fmt: &FormatDef, opts: &Options, output: &Path) {
    let cursor = gdk::Cursor::from_name("wait", None);
    win.root.set_cursor(cursor.as_ref());
    while glib::MainContext::default().iteration(false) {}

    let result = perform_export(win, fmt, opts, output);

    win.root.set_cursor(None);
    if let Err(e) = result {
        win.show_error("Export failed", &e);
    }
}

fn perform_export(
    win: &AppWindow,
    fmt: &FormatDef,
    opts: &Options,
    output: &Path,
) -> Result<(), String> {
    let assets = Assets::materialize()?;
    let mut text = win.editor.full_text();
    if opts.rel_to_abs {
        if let Some(dir) = win.editor.flags.doc_dir.borrow().as_deref() {
            text = rewrite_relative_to_absolute(&text, dir);
        }
    }
    let mut args = pandoc_args(fmt, opts, &assets, settings().input_format(), output);
    if fmt.kind == Kind::DzSlides {
        let tpl = dzslides_template()?;
        if let Some(slot) = args.iter_mut().find(|a| a.as_str() == "$DZSLIDES$") {
            *slot = tpl.display().to_string();
        }
    }
    // Relative document resources resolve against the document's folder
    // (same mechanism as the preview).
    if let Some(dir) = win.editor.flags.doc_dir.borrow().as_deref() {
        let _ = std::env::set_current_dir(dir);
    }
    let result = run_pandoc(&text, output, &args);
    if fmt.kind == Kind::Reveal && result.is_ok() {
        if let Some(parent) = output.parent() {
            extract_reveal_library(parent)?;
        }
    }
    result
}

/// Standalone HTML template for the quick export (§1): the app's preview
/// stylesheet for the current color scheme embedded, MathJax loading from
/// the converter's standard CDN script.
fn quick_html_template() -> Result<PathBuf, String> {
    let dark = match settings().color_scheme() {
        ColorScheme::Dark => true,
        ColorScheme::System => adw::StyleManager::default().is_dark(),
        _ => false,
    };
    let base = String::from_utf8_lossy(&resource_bytes("styles/base.css")?).into_owned();
    let scheme =
        String::from_utf8_lossy(&resource_bytes(&settings().color_scheme().preview_css(dark))?)
            .into_owned();
    let tpl = format!(
        "<!DOCTYPE html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n<title>$title$</title>\n\
         <style>\n{base}\n</style>\n<style>\n{scheme}\n</style>\n$math$\n</head>\n\
         <body>\n$body$\n</body>\n</html>\n"
    );
    let path =
        std::env::temp_dir().join(format!("griffel-{}-html-template.html", std::process::id()));
    fs::write(&path, tpl).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Quick HTML export (§1): standalone, scheme stylesheet embedded,
/// MathJax math, all resources embedded.
fn run_quick_html(win: &AppWindow, output: &Path) {
    let cursor = gdk::Cursor::from_name("wait", None);
    win.root.set_cursor(cursor.as_ref());
    while glib::MainContext::default().iteration(false) {}

    let result = (|| -> Result<(), String> {
        let tpl = quick_html_template()?;
        let args: Vec<String> = vec![
            "--from".into(),
            settings().input_format().pandoc_format(false),
            "--to".into(),
            "html5".into(),
            "--standalone".into(),
            "--embed-resources".into(),
            "--mathjax".into(),
            "--template".into(),
            tpl.display().to_string(),
            "-o".into(),
            output.display().to_string(),
        ];
        let text = win.editor.full_text();
        if let Some(dir) = win.editor.flags.doc_dir.borrow().as_deref() {
            let _ = std::env::set_current_dir(dir);
        }
        run_pandoc(&text, output, &args)
    })();

    win.root.set_cursor(None);
    if let Err(e) = result {
        win.show_error("Export failed", &e);
    }
}

// ---------------------------------------------------------------------------
// Dialogs
// ---------------------------------------------------------------------------

fn document_stem(win: &AppWindow) -> String {
    let name = win.state.document.borrow().display_name();
    match name.rfind('.') {
        Some(dot) if dot > 0 => name[..dot].to_string(),
        _ => name,
    }
}

/// Pre-named save dialog defaulting to the document's folder (§1).
fn save_dialog(win: &AppWindow, title: &str, fmt: &FormatDef, opts: Options) {
    let dialog = gtk::FileDialog::new();
    dialog.set_title(title);
    let filter = gtk::FileFilter::new();
    filter.set_name(Some(fmt.name));
    filter.add_pattern(&format!("*.{}", fmt.ext));
    let filters = gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&filter);
    dialog.set_filters(Some(&filters));
    dialog.set_initial_name(Some(&format!("{}.{}", document_stem(win), fmt.ext)));
    let initial_folder = win
        .state
        .document
        .borrow()
        .parent_path()
        .map(gio::File::for_path)
        .or_else(crate::files::default_folder);
    if let Some(folder) = initial_folder {
        dialog.set_initial_folder(Some(&folder));
    }

    let win = win.clone();
    let fmt = fmt.clone();
    let parent = win.root.clone();
    dialog.save(
        Some(&parent),
        gio::Cancellable::NONE,
        move |result| match result {
            Ok(file) => {
                if let Some(path) = file.path() {
                    run_conversion(&win, &fmt, &opts, &path);
                }
            }
            Err(_) => {}
        },
    );
}

/// Folder chooser for the reveal.js export (§2.2): the HTML plus the
/// bundled library land in the chosen folder.
fn folder_dialog(win: &AppWindow, fmt: &FormatDef, opts: Options) {
    let dialog = gtk::FileDialog::new();
    dialog.set_title("Export slideshow");
    let win = win.clone();
    let fmt = fmt.clone();
    let parent = win.root.clone();
    dialog.select_folder(
        Some(&parent),
        gio::Cancellable::NONE,
        move |result| match result {
            Ok(file) => {
                if let Some(dir) = file.path() {
                    let html = dir.join(format!("{}.html", document_stem(&win)));
                    run_conversion(&win, &fmt, &opts, &html);
                }
            }
            Err(_) => {}
        },
    );
}

// ---------------------------------------------------------------------------
// Quick exports (§1): one click, defaults, pre-named dialog.
// ---------------------------------------------------------------------------

fn quick(win: &AppWindow, index: usize, title: &str) {
    let fmt = &FORMATS[index];
    if fmt.kind == Kind::TypstPdf && !has_typst() {
        win.show_error(
            "Export failed",
            "The Typst engine was not found.\n\nInstall Typst (https://typst.app) to export PDF.",
        );
        return;
    }
    let opts = Options {
        self_contained: true,
        ..Options::default()
    };
    let dialog = gtk::FileDialog::new();
    dialog.set_title(title);
    let filter = gtk::FileFilter::new();
    filter.set_name(Some(fmt.name));
    filter.add_pattern(&format!("*.{}", fmt.ext));
    let filters = gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&filter);
    dialog.set_filters(Some(&filters));
    dialog.set_initial_name(Some(&format!("{}.{}", document_stem(win), fmt.ext)));
    let initial_folder = win
        .state
        .document
        .borrow()
        .parent_path()
        .map(gio::File::for_path)
        .or_else(crate::files::default_folder);
    if let Some(folder) = initial_folder {
        dialog.set_initial_folder(Some(&folder));
    }

    let win = win.clone();
    let fmt = fmt.clone();
    let parent = win.root.clone();
    dialog.save(
        Some(&parent),
        gio::Cancellable::NONE,
        move |result| match result {
            Ok(file) => {
                if let Some(path) = file.path() {
                    if fmt.kind == Kind::PlainHtml {
                        run_quick_html(&win, &path);
                    } else {
                        run_conversion(&win, &fmt, &opts, &path);
                    }
                }
            }
            Err(_) => {}
        },
    );
}

pub fn quick_pdf(win: &AppWindow) {
    quick(win, 0, "Export as PDF");
}

pub fn quick_html(win: &AppWindow) {
    quick(win, 10, "Export as HTML");
}

pub fn quick_odt(win: &AppWindow) {
    quick(win, 2, "Export as ODT");
}

/// Copy HTML (§3): the document fragment (no standalone wrapper), same
/// flavor preference, on the clipboard.
pub fn copy_html(win: &AppWindow) {
    let text = win.editor.full_text();
    let flavor = settings().input_format();
    let doc_dir = win.editor.flags.doc_dir.borrow().clone();
    let tmp = std::env::temp_dir().join(format!("griffel-copyhtml-{}.md", std::process::id()));
    if fs::write(&tmp, text.as_bytes()).is_err() {
        return;
    }
    let mut cmd = Command::new("pandoc");
    cmd.arg("--from").arg(flavor.pandoc_format(false));
    cmd.arg("--to").arg("html5");
    cmd.arg("--mathml");
    cmd.arg("--wrap=none");
    if let Some(dir) = doc_dir {
        cmd.current_dir(dir);
    }
    cmd.arg(&tmp);
    let output = cmd.output();
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

// ---------------------------------------------------------------------------
// Advanced export dialog (§2)
// ---------------------------------------------------------------------------

pub fn advanced_dialog(win: &AppWindow) {
    let win = win.clone();
    let dialog = gtk::Window::builder()
        .title("Export to PDF (Typst engine)")
        .modal(true)
        .transient_for(&win.root)
        .default_width(860)
        .default_height(520)
        .build();

    let paned = gtk::Paned::new(gtk::Orientation::Horizontal);
    paned.set_position(250);
    paned.set_margin_top(12);
    paned.set_margin_bottom(12);
    paned.set_margin_start(12);
    paned.set_margin_end(12);

    // --- format list (left) -------------------------------------------------
    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::Browse);
    for f in FORMATS.iter() {
        let row = gtk::ListBoxRow::new();
        let label = gtk::Label::new(Some(f.name));
        label.set_halign(gtk::Align::Start);
        label.set_margin_top(6);
        label.set_margin_bottom(6);
        label.set_margin_start(8);
        label.set_margin_end(8);
        row.set_child(Some(&label));
        list.append(&row);
    }
    let list_scroll = gtk::ScrolledWindow::builder()
        .child(&list)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .build();

    // --- options pane (right) ----------------------------------------------
    let pane = gtk::Box::new(gtk::Orientation::Vertical, 10);

    let rel_abs = gtk::CheckButton::with_label("Rewrite relative paths to absolute");
    let standalone = gtk::CheckButton::with_label("Standalone document");
    standalone.set_active(true);
    let toc = gtk::CheckButton::with_label("Table of contents");
    let number = gtk::CheckButton::with_label("Number sections");

    let page_a4 = gtk::CheckButton::with_label("A4");
    page_a4.set_active(true);
    let page_letter = gtk::CheckButton::with_label("Letter");
    page_letter.set_group(Some(&page_a4));
    let page_box = labeled_row("Page size", &page_a4, &page_letter);

    let slide_43 = gtk::CheckButton::with_label("4:3");
    slide_43.set_active(true);
    let slide_169 = gtk::CheckButton::with_label("16:9");
    slide_169.set_group(Some(&slide_43));
    let slide_box = labeled_row("Slide size", &slide_43, &slide_169);

    let selfcontained = gtk::CheckButton::with_label("Self-contained (embed all resources)");

    let highlight_expander = gtk::Expander::new(Some("Syntax highlighting"));
    let highlight_on = gtk::CheckButton::with_label("Enable");
    highlight_on.set_active(true);
    let styles = gtk::ComboBoxText::new();
    for s in HIGHLIGHT_STYLES {
        styles.append_text(s);
    }
    styles.set_active(Some(0));
    let hl_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    hl_row.append(&highlight_on);
    hl_row.append(&styles);
    highlight_expander.set_child(Some(&hl_row));

    let incremental = gtk::CheckButton::with_label("Incremental bullets");

    pane.append(&rel_abs);
    pane.append(&standalone);
    pane.append(&toc);
    pane.append(&number);
    pane.append(&page_box);
    pane.append(&slide_box);
    pane.append(&selfcontained);
    pane.append(&highlight_expander);
    pane.append(&incremental);
    let options_scroll = gtk::ScrolledWindow::builder()
        .child(&pane)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .build();

    // --- missing-TeX warning page (§2.2) ------------------------------------
    let tex_box = gtk::Box::new(gtk::Orientation::Vertical, 12);
    tex_box.set_margin_top(24);
    tex_box.set_margin_start(24);
    tex_box.set_margin_end(24);
    let tex_title = gtk::Label::new(Some("LaTeX toolchain not found"));
    tex_title.add_css_class("title-2");
    tex_title.set_halign(gtk::Align::Start);
    let tex_body = gtk::Label::new(Some(&tex_warning_text()));
    tex_body.set_wrap(true);
    tex_body.set_xalign(0.0);
    let tex_copy = gtk::Button::with_label("Copy install command");
    let tex_root = win.root.clone();
    tex_copy.connect_clicked(move |_| {
        let bytes = glib::Bytes::from(tex_install_command().as_bytes());
        let provider = gdk::ContentProvider::for_bytes("text/plain", &bytes);
        let _ = tex_root.clipboard().set_content(Some(&provider));
    });
    tex_box.append(&tex_title);
    tex_box.append(&tex_body);
    tex_box.append(&tex_copy);

    let right_stack = gtk::Stack::new();
    right_stack.add_named(&options_scroll, Some("options"));
    right_stack.add_named(&tex_box, Some("tex"));

    paned.set_start_child(Some(&list_scroll));
    paned.set_end_child(Some(&right_stack));

    let btn_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    btn_box.set_halign(gtk::Align::End);
    btn_box.set_margin_top(8);
    btn_box.set_margin_bottom(8);
    btn_box.set_margin_start(12);
    btn_box.set_margin_end(12);
    let cancel = gtk::Button::with_label("Cancel");
    let export_btn = gtk::Button::with_label("Export…");
    export_btn.add_css_class("suggested-action");
    btn_box.append(&cancel);
    btn_box.append(&export_btn);

    let outer = gtk::Box::new(gtk::Orientation::Vertical, 0);
    outer.append(&paned);
    outer.append(&btn_box);
    dialog.set_child(Some(&outer));

    // --- selection drives visibility and the dialog title -------------------
    {
        let dialog = dialog.clone();
        let standalone = standalone.clone();
        let toc = toc.clone();
        let number = number.clone();
        let page_box = page_box.clone();
        let slide_box = slide_box.clone();
        let selfcontained = selfcontained.clone();
        let highlight_expander = highlight_expander.clone();
        let incremental = incremental.clone();
        let right_stack = right_stack.clone();
        list.connect_row_selected(move |_, row| {
            let Some(row) = row else { return };
            let idx = row.index().max(0) as usize;
            let Some(f) = FORMATS.get(idx) else { return };
            dialog.set_title(Some(&format!("Export to {}", f.name)));
            standalone.set_visible(f.standalone);
            toc.set_visible(f.toc);
            number.set_visible(f.number_sections);
            page_box.set_visible(f.page_size);
            slide_box.set_visible(f.slide_size);
            selfcontained.set_visible(f.self_contained);
            highlight_expander.set_visible(f.highlight);
            incremental.set_visible(f.incremental);
            if f.needs_tex && !has_tex() {
                right_stack.set_visible_child_name("tex");
            } else {
                right_stack.set_visible_child_name("options");
            }
        });
    }

    // --- export --------------------------------------------------------------
    {
        let dialog = dialog.clone();
        cancel.connect_clicked(move |_| dialog.close());
    }
    {
        let dialog = dialog.clone();
        let win = win.clone();
        let list = list.clone();
        let standalone = standalone.clone();
        let rel_abs = rel_abs.clone();
        let toc = toc.clone();
        let number = number.clone();
        let page_a4 = page_a4.clone();
        let slide_169 = slide_169.clone();
        let selfcontained = selfcontained.clone();
        let highlight_on = highlight_on.clone();
        let styles = styles.clone();
        let incremental = incremental.clone();
        let right_stack = right_stack.clone();
        export_btn.connect_clicked(move |_| {
            let idx = list
                .selected_row()
                .map(|r| r.index())
                .unwrap_or(0)
                .max(0) as usize;
            let Some(fmt) = FORMATS.get(idx) else { return };
            if fmt.needs_tex && !has_tex() {
                right_stack.set_visible_child_name("tex");
                return;
            }
            let opts = Options {
                standalone: standalone.is_active(),
                rel_to_abs: rel_abs.is_active(),
                toc: toc.is_active(),
                number_sections: number.is_active(),
                page_size: if page_a4.is_active() {
                    PageSize::A4
                } else {
                    PageSize::Letter
                },
                wide_slides: slide_169.is_active(),
                self_contained: selfcontained.is_active(),
                highlight: highlight_on.is_active(),
                highlight_style: styles.active().unwrap_or(0) as usize,
                incremental: incremental.is_active(),
            };
            let win = win.clone();
            let fmt = fmt.clone();
            dialog.close();
            if fmt.kind == Kind::Reveal {
                folder_dialog(&win, &fmt, opts);
            } else {
                save_dialog(&win, "Export", &fmt, opts);
            }
        });
    }

    list.select_row(list.row_at_index(0).as_ref());
    dialog.present();
}

fn labeled_row(label: &str, a: &gtk::CheckButton, b: &gtk::CheckButton) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let l = gtk::Label::new(Some(label));
    row.append(&l);
    row.append(a);
    row.append(b);
    row
}

fn sandboxed_install() -> bool {
    Path::new("/.flatpak-info").exists()
}

fn tex_warning_text() -> String {
    if sandboxed_install() {
        "LaTeX-based export needs a TeX installation, which is not available inside \
         the sandbox. Install the TeX extension for your sandbox runtime (e.g. \
         org.freedesktop.Sdk.Extension.texlive) with your software store or flatpak, \
         then restart the application."
            .to_string()
    } else {
        "LaTeX-based export needs a TeX installation, which was not found on this \
         system. Install a TeX distribution such as TeX Live (the XeLaTeX engine in \
         particular) with your system's package manager, then try again."
            .to_string()
    }
}

fn tex_install_command() -> String {
    if sandboxed_install() {
        "flatpak install org.freedesktop.Sdk.Extension.texlive//24.08".into()
    } else if Path::new("/etc/arch-release").exists() {
        "sudo pacman -S texlive-basic texlive-xetex".into()
    } else if Path::new("/etc/fedora-release").exists() {
        "sudo dnf install texlive-xetex".into()
    } else {
        "sudo apt install texlive-xetex".into()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> Options {
        Options::default()
    }

    fn assets() -> Assets {
        Assets {
            typst_template: Some(PathBuf::from("/tmp/typ.tpl")),
            odt_a4: Some(PathBuf::from("/tmp/odt-a4")),
            odt_letter: Some(PathBuf::from("/tmp/odt-letter")),
            docx_a4: Some(PathBuf::from("/tmp/docx-a4")),
            docx_letter: Some(PathBuf::from("/tmp/docx-letter")),
        }
    }

    #[test]
    fn format_table_matches_spec() {
        assert_eq!(FORMATS.len(), 19);
        for (i, f) in FORMATS.iter().enumerate() {
            assert!(!f.name.is_empty(), "format {i} unnamed");
            assert!(!f.ext.is_empty());
        }
        // Only LaTeX-engine formats demand TeX.
        for f in FORMATS.iter() {
            assert_eq!(f.needs_tex, f.kind == Kind::LatexPdf, "{} tex flag", f.name);
        }
        assert!(FORMATS[0].page_size, "typst pdf paged");
        assert!(!FORMATS[0].highlight, "typst pdf has no highlighting");
        assert!(FORMATS[5].incremental, "reveal incremental");
        assert!(FORMATS[6].slide_size, "beamer slide size");
        assert!(FORMATS[10].self_contained, "plain html self-contained");
        assert!(FORMATS[11].standalone, "mediawiki standalone per spec");
    }

    #[test]
    fn args_typst_pdf_uses_template_and_papersize() {
        let o = Options {
            page_size: PageSize::Letter,
            ..opts()
        };
        let a = pandoc_args(
            &FORMATS[0],
            &o,
            &assets(),
            InputFormat::Markdown,
            Path::new("/x.pdf"),
        );
        assert!(a.contains(&"/tmp/typ.tpl".to_string()));
        assert!(a.contains(&"papersize=letter".to_string()));
        assert!(a.contains(&"--pdf-engine=typst".to_string()));
    }

    #[test]
    fn args_odt_picks_reference_doc_by_size() {
        let a4 = pandoc_args(
            &FORMATS[2],
            &opts(),
            &assets(),
            InputFormat::Markdown,
            Path::new("/x.odt"),
        );
        assert!(a4.contains(&"/tmp/odt-a4".to_string()));
        let letter = pandoc_args(
            &FORMATS[2],
            &Options {
                page_size: PageSize::Letter,
                ..opts()
            },
            &assets(),
            InputFormat::Markdown,
            Path::new("/x.odt"),
        );
        assert!(letter.contains(&"/tmp/odt-letter".to_string()));
    }

    #[test]
    fn args_reveal_and_beamer() {
        let a = pandoc_args(
            &FORMATS[5],
            &opts(),
            &assets(),
            InputFormat::Markdown,
            Path::new("/d/s.html"),
        );
        assert!(a.contains(&"revealjs-url=reveal.js".to_string()));
        assert!(a.contains(&"--standalone".to_string()));
        let wide = pandoc_args(
            &FORMATS[6],
            &Options {
                wide_slides: true,
                ..opts()
            },
            &assets(),
            InputFormat::Markdown,
            Path::new("/b.pdf"),
        );
        assert!(wide.contains(&"aspectratio=169".to_string()));
        assert!(wide.contains(&"--pdf-engine=xelatex".to_string()));
    }

    #[test]
    fn args_flags_respect_format_visibility() {
        // MediaWiki has no toc/numbering/incremental UI; opts must not leak.
        let o = Options {
            toc: true,
            number_sections: true,
            incremental: true,
            ..opts()
        };
        let a = pandoc_args(
            &FORMATS[11],
            &o,
            &assets(),
            InputFormat::Markdown,
            Path::new("/x.txt"),
        );
        assert!(!a.contains(&"--toc".to_string()));
        assert!(!a.contains(&"--number-sections".to_string()));
        assert!(!a.contains(&"-i".to_string()));
    }

    #[test]
    fn rewrite_paths() {
        let dir = Path::new("/home/u/doc");
        let out = rewrite_relative_to_absolute(
            "![pic](img/p.png) [x](other/a.md) [u](https://e.x/a.png) [a](#s) [abs](/etc) [m](mailto:a@b) []()",
            dir,
        );
        assert!(out.contains("](/home/u/doc/img/p.png)"));
        assert!(out.contains("](/home/u/doc/other/a.md)"));
        assert!(out.contains("](https://e.x/a.png)"));
        assert!(out.contains("](#s)"));
        assert!(out.contains("](/etc)"));
        assert!(out.contains("](mailto:a@b)"));
        assert!(out.contains("]()"));
    }

    #[test]
    fn rewrite_upward_paths() {
        let out = rewrite_relative_to_absolute("[x](../up/i.png)", Path::new("/a/b"));
        assert!(out.contains("](/a/up/i.png)"));
    }
}
