//! Inline previews ("peek" popovers, `03-behavior/11-inline-previews.md`):
//! a ~400×300 popover at the cursor that previews the construct under it —
//! math (LaTeX), images, links (small web view), footnote references, or
//! the dictionary definition of the word under the cursor.
//!
//! The construct detection (`detect`, `footnote_definition`) is pure and
//! unit-tested; the popover builds its content asynchronously.

use gtk::prelude::*;
use std::sync::OnceLock;
use webkit6::prelude::*;

use crate::editor::Editor;

/// The construct found at a cursor position on a line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PeekTarget {
    Math { formula: String, display: bool },
    Image { path: String },
    Link { url: String },
    Footnote { id: String },
    Word { word: String },
}

/// What is under column `col` (char offset) on `line`, in the spec's
/// priority order: math, image, link/autolink, footnote, word.
pub fn detect(line: &str, col: usize) -> Option<PeekTarget> {
    if let Some((formula, display)) = math_span(line, col) {
        return Some(PeekTarget::Math { formula, display });
    }
    for m in image_re().captures_iter(line) {
        if contains_col(&m, line, col) {
            return Some(PeekTarget::Image {
                path: m[2].to_owned(),
            });
        }
    }
    for m in link_re().captures_iter(line) {
        // Not the label/url part of an image (already handled above).
        if contains_col(&m, line, col) && !line[..m.get(0).unwrap().start()].ends_with('!') {
            return Some(PeekTarget::Link {
                url: m[2].to_owned(),
            });
        }
    }
    for m in autolink_re().captures_iter(line) {
        if contains_col(&m, line, col) {
            return Some(PeekTarget::Link {
                url: m[1].to_owned(),
            });
        }
    }
    for m in footnote_re().captures_iter(line) {
        if contains_col(&m, line, col) {
            return Some(PeekTarget::Footnote {
                id: m[1].to_owned(),
            });
        }
    }
    word_at(line, col).map(|word| PeekTarget::Word { word })
}

fn image_re() -> &'static regex::Regex {
    static R: OnceLock<regex::Regex> = OnceLock::new();
    R.get_or_init(|| regex::Regex::new(r"!\[([^\]]*)\]\(([^)\s]*)\)").unwrap())
}

fn link_re() -> &'static regex::Regex {
    static R: OnceLock<regex::Regex> = OnceLock::new();
    R.get_or_init(|| regex::Regex::new(r"\[([^\]]*)\]\(([^)\s]*)\)").unwrap())
}

fn autolink_re() -> &'static regex::Regex {
    static R: OnceLock<regex::Regex> = OnceLock::new();
    R.get_or_init(|| regex::Regex::new(r"<([^<>\s]+)>").unwrap())
}

fn footnote_re() -> &'static regex::Regex {
    static R: OnceLock<regex::Regex> = OnceLock::new();
    R.get_or_init(|| regex::Regex::new(r"\[\^([^\]\s]+)\]").unwrap())
}

/// Whether the match spans `col` (a char offset; regex offsets are bytes).
fn contains_col(m: &regex::Captures, line: &str, col: usize) -> bool {
    let whole = m.get(0).unwrap();
    let start = line[..whole.start()].chars().count();
    let end = start + whole.as_str().chars().count();
    start <= col && col < end
}

/// Pair the unescaped `$`/`$$` delimiters on the line; return the formula
/// whose interior contains `col` (char offset).
fn math_span(line: &str, col: usize) -> Option<(String, bool)> {
    let chars: Vec<char> = line.chars().collect();
    let mut tokens: Vec<(usize, bool)> = Vec::new(); // (char index, is_double)
    let mut i = 0usize;
    while i < chars.len() {
        if chars[i] == '\\' {
            i += 2;
            continue;
        }
        if chars[i] == '$' {
            let double = i + 1 < chars.len() && chars[i + 1] == '$';
            tokens.push((i, double));
            i += if double { 2 } else { 1 };
            continue;
        }
        i += 1;
    }
    for w in tokens.windows(2) {
        let (open, open_d) = w[0];
        let (close, close_d) = w[1];
        if open_d != close_d {
            continue;
        }
        let inner_start = open + if open_d { 2 } else { 1 };
        if col >= inner_start && col <= close {
            let formula: String = chars[inner_start..close].iter().collect();
            if !formula.trim().is_empty() {
                return Some((formula, open_d));
            }
        }
    }
    None
}

/// Expand `col` to the word around it (letters/digits/apostrophes/hyphens).
fn word_at(line: &str, col: usize) -> Option<String> {
    let chars: Vec<char> = line.chars().collect();
    if col > chars.len() {
        return None;
    }
    let is_word = |c: char| c.is_alphanumeric() || c == '\'' || c == '-' || c == '’';
    let left = chars[..col]
        .iter()
        .rposition(|&c| !is_word(c))
        .map(|p| p + 1)
        .unwrap_or(0);
    let right = left
        + chars[left..]
            .iter()
            .position(|&c| !is_word(c))
            .unwrap_or(chars.len() - left);
    if right <= left {
        return None;
    }
    let word: String = chars[left..right].iter().collect();
    if word.chars().any(char::is_alphabetic) {
        Some(word)
    } else {
        None
    }
}

/// The definition text of footnote `id`: the remainder of the
/// `[^id]: …` line plus following indented/non-blank lines, de-indented
/// (`03-behavior/11-inline-previews.md` §1.4).
pub fn footnote_definition(text: &str, id: &str) -> Option<String> {
    let marker = format!("[^{id}]:");
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        if let Some(rest) = line.strip_prefix(&marker) {
            let mut out = rest.trim().to_owned();
            for cont in lines.by_ref() {
                if cont.trim().is_empty() {
                    break;
                }
                // Indented continuation lines are de-indented for display.
                out.push(' ');
                out.push_str(cont.trim_start());
            }
            return Some(out);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Popover
// ---------------------------------------------------------------------------

/// Open the peek popover for the construct at `iter`.
pub fn open_at_iter(editor: &Editor, iter: &gtk::TextIter) {
    let line_no = iter.line();
    let line = buffer_line(editor, line_no);
    let line_start = editor
        .buffer
        .iter_at_line(line_no)
        .map(|i| i.offset())
        .unwrap_or(0);
    let Some(target) = detect(&line, (iter.offset() - line_start) as usize) else {
        return;
    };
    // Rect in view-widget coordinates for the popover anchor.
    let mut rect = editor.view.iter_location(iter);
    let (wx, wy) = editor
        .view
        .window_to_buffer_coords(gtk::TextWindowType::Widget, 0, 0);
    rect.set_x(rect.x() - wx);
    rect.set_y(rect.y() - wy);
    show(editor, &rect, target);
}

/// Open the peek popover at view-relative coordinates (Ctrl+click).
pub fn open_at_coords(editor: &Editor, x: i32, y: i32) {
    if let Some(iter) = editor.view.iter_at_location(x, y) {
        open_at_iter(editor, &iter);
    }
}

fn buffer_line(editor: &Editor, line_no: i32) -> String {
    let start = editor
        .buffer
        .iter_at_line(line_no)
        .unwrap_or_else(|| editor.buffer.start_iter());
    let mut end = start;
    if !end.ends_line() {
        end.forward_to_line_end();
    }
    start.text(&end).into()
}

fn show(editor: &Editor, rect: &gdk::Rectangle, target: PeekTarget) {
    let popover = gtk::Popover::new();
    popover.set_parent(&editor.view);
    popover.set_pointing_to(Some(rect));

    let stack = gtk::Stack::new();
    stack.set_size_request(400, 300);
    stack.add_named(&gtk::Box::new(gtk::Orientation::Vertical, 0), Some("blank"));
    stack.add_named(&spinner_page(), Some("loading"));
    stack.add_named(&error_page(""), Some("error"));
    stack.set_visible_child_name("blank");
    popover.set_child(Some(&stack));

    // The spinner appears only after a tiny delay so instant results
    // don't flash it (§2).
    {
        let stack = stack.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(150), move || {
            if stack.visible_child_name().as_deref() == Some("blank") {
                stack.set_visible_child_name("loading");
            }
            glib::ControlFlow::Break
        });
    }

    // Closing discards the content: detach so the popover (and any web
    // view inside it) can finalize.
    {
        let weak = glib::WeakRef::<gtk::Popover>::new();
        weak.set(Some(&popover));
        popover.connect_closed(move |_| {
            if let Some(p) = weak.upgrade() {
                p.unparent();
            }
        });
    }

    match target {
        PeekTarget::Math { formula, display } => load_math(&stack, &formula, display),
        PeekTarget::Image { path } => load_image(editor, &stack, &path),
        PeekTarget::Link { url } => load_link(&stack, &url),
        PeekTarget::Footnote { id } => {
            let text = editor.full_text();
            match footnote_definition(&text, &id) {
                Some(def) => show_text(&stack, &format!("^{id}"), &def),
                None => show_error(&stack, &format!("No matching footnote “[^{id}]”.")),
            }
        }
        PeekTarget::Word { word } => load_word(&stack, &word),
    }

    popover.popup();
}

fn spinner_page() -> gtk::Box {
    let box_ = gtk::Box::new(gtk::Orientation::Vertical, 12);
    box_.set_valign(gtk::Align::Center);
    box_.set_halign(gtk::Align::Center);
    let spinner = gtk::Spinner::new();
    spinner.set_size_request(32, 32);
    spinner.start();
    box_.append(&spinner);
    box_
}

fn error_page(message: &str) -> gtk::Box {
    let box_ = gtk::Box::new(gtk::Orientation::Vertical, 12);
    box_.set_valign(gtk::Align::Center);
    box_.set_halign(gtk::Align::Center);
    let label = gtk::Label::new(Some(message));
    label.set_wrap(true);
    label.set_max_width_chars(44);
    box_.append(&label);
    box_
}

fn show_error(stack: &gtk::Stack, message: &str) {
    if let Some(label) = stack
        .child_by_name("error")
        .and_then(|c| c.first_child())
        .and_then(|c| c.downcast::<gtk::Label>().ok())
    {
        label.set_text(message);
    }
    stack.set_visible_child_name("error");
}

/// Simple label content (footnotes, dictionary definitions).
fn show_text(stack: &gtk::Stack, title: &str, body: &str) {
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    let box_ = gtk::Box::new(gtk::Orientation::Vertical, 8);
    box_.set_margin_top(12);
    box_.set_margin_bottom(12);
    box_.set_margin_start(12);
    box_.set_margin_end(12);
    let heading = gtk::Label::new(Some(title));
    heading.set_halign(gtk::Align::Start);
    heading.add_css_class("heading");
    box_.append(&heading);
    let label = gtk::Label::new(Some(body));
    label.set_wrap(true);
    label.set_wrap_mode(pango::WrapMode::WordChar);
    label.set_xalign(0.0);
    box_.append(&label);
    scroll.set_child(Some(&box_));
    stack.add_named(&scroll, Some("content"));
    stack.set_visible_child_name("content");
}

// --- math -------------------------------------------------------------------

/// Render the formula with the local LaTeX toolchain (latex + dvipng) and
/// show the image; missing tools or a failing formula produce the spec's
/// error pages (§1.1).
fn load_math(stack: &gtk::Stack, formula: &str, display: bool) {
    if !tool_present("latex") || !tool_present("dvipng") {
        show_error(
            stack,
            "LaTeX is not installed. Install texlive to preview formulas.",
        );
        return;
    }
    let formula = formula.to_owned();
    let stack = stack.clone();
    glib::spawn_future_local(async move {
        let f = formula.clone();
        let handle = gio::spawn_blocking(move || render_math(&f, display));
        match handle.await.unwrap_or(Err("rendering failed".into())) {
            Ok(png) => {
                let picture = gtk::Picture::new();
                picture.set_paintable(Some(&png));
                picture.set_can_shrink(true);
                let scroll = gtk::ScrolledWindow::new();
                scroll.set_child(Some(&picture));
                stack.add_named(&scroll, Some("content"));
                stack.set_visible_child_name("content");
            }
            Err(_) => {
                show_error(&stack, &format!("Invalid formula: “{formula}”"));
            }
        }
    });
}

/// Run latex → dvi → dvipng in a temp dir; returns the rendered texture.
fn render_math(formula: &str, display: bool) -> Result<gdk::Texture, String> {
    let dir = std::env::temp_dir().join(format!("griffel-peek-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let body = if display {
        format!("\\[{formula}\\]")
    } else {
        format!("${formula}$")
    };
    let tex = format!(
        "\\documentclass{{article}}\n\\usepackage{{amsmath}}\n\\pagestyle{{empty}}\n\
         \\begin{{document}}\n{body}\n\\end{{document}}\n"
    );
    let tex_path = dir.join("formula.tex");
    std::fs::write(&tex_path, tex).map_err(|e| e.to_string())?;

    let out = std::process::Command::new("latex")
        .args(["-interaction=nonstopmode", "-halt-on-error", "formula.tex"])
        .current_dir(&dir)
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        let _ = std::fs::remove_dir_all(&dir);
        return Err("latex failed".into());
    }
    let out = std::process::Command::new("dvipng")
        .args([
            "-T",
            "tight",
            "--truecolor",
            "-bg",
            "Transparent",
            "-o",
            "formula.png",
            "formula.dvi",
        ])
        .current_dir(&dir)
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        let _ = std::fs::remove_dir_all(&dir);
        return Err("dvipng failed".into());
    }
    let texture =
        gdk::Texture::from_filename(dir.join("formula.png")).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_dir_all(&dir);
    Ok(texture)
}

fn tool_present(name: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths).any(|dir| dir.join(name).is_file())
        })
        .unwrap_or(false)
}

// --- image ------------------------------------------------------------------

fn load_image(editor: &Editor, stack: &gtk::Stack, path: &str) {
    if path.starts_with("http://") || path.starts_with("https://") {
        // Remote URLs fall through to the web preview (§1.2).
        load_link(stack, path);
        return;
    }
    let decoded = crate::files::paths::decode_label(path);
    let resolved = match editor.flags.doc_dir.borrow().clone() {
        Some(dir) if !std::path::Path::new(&decoded).is_absolute() => dir.join(&decoded),
        _ => std::path::PathBuf::from(&decoded),
    };
    match gdk::Texture::from_filename(&resolved) {
        Ok(texture) => {
            let picture = gtk::Picture::new();
            picture.set_paintable(Some(&texture));
            picture.set_can_shrink(true);
            let scroll = gtk::ScrolledWindow::new();
            scroll.set_child(Some(&picture));
            stack.add_named(&scroll, Some("content"));
            stack.set_visible_child_name("content");
        }
        Err(_) => {
            show_error(stack, &format!("Image not found: {decoded}"));
        }
    }
}

// --- link -------------------------------------------------------------------

fn load_link(stack: &gtk::Stack, url: &str) {
    let url = if url.contains("://") {
        url.to_owned()
    } else {
        format!("http://{url}")
    };
    let view = webkit6::WebView::new();
    view.set_zoom_level(0.5);
    view.load_uri(&url);
    stack.add_named(&view, Some("content"));
    stack.set_visible_child_name("content");
}

// --- dictionary -------------------------------------------------------------

fn load_word(stack: &gtk::Stack, word: &str) {
    let stack = stack.clone();
    let word = word.to_owned();
    glib::spawn_future_local(async move {
        let w = word.clone();
        let handle = gio::spawn_blocking(move || crate::dict::lookup(&w));
        match handle.await.unwrap_or(Err("lookup failed".into())) {
            Ok(defs) if defs.is_empty() => {
                show_error(&stack, &format!("No entry for “{word}”."));
            }
            Ok(defs) => {
                let mut body = String::new();
                for def in defs {
                    if let Some(class) = &def.word_class {
                        body.push_str(class);
                        body.push('\n');
                    }
                    for (i, sense) in def.senses.iter().enumerate() {
                        body.push_str(&format!("{}. {sense}\n", i + 1));
                    }
                    body.push('\n');
                }
                show_text(&stack, &word, body.trim_end());
            }
            Err(_) => {
                show_error(&stack, "Not found (the dictionary service is unreachable).");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Char offset of the middle of `needle` in `line`.
    fn col_of(line: &str, needle: &str) -> usize {
        let start = line.find(needle).unwrap();
        start + needle.len() / 2
    }

    #[test]
    fn math_inline_and_display() {
        let line = "text $e=mc^2$ more";
        assert_eq!(
            detect(line, col_of(line, "e=mc")),
            Some(PeekTarget::Math {
                formula: "e=mc^2".into(),
                display: false
            })
        );
        let line = "$$\\int x\\,dx$$";
        assert_eq!(
            detect(line, col_of(line, "int")),
            Some(PeekTarget::Math {
                formula: "\\int x\\,dx".into(),
                display: true
            })
        );
    }

    #[test]
    fn escaped_dollars_not_math() {
        let line = r"cost \$5 and \$6";
        match detect(line, col_of(line, "5 and")) {
            Some(PeekTarget::Math { .. }) => panic!("escaped dollars parsed as math"),
            _ => {}
        }
    }

    #[test]
    fn image_beats_link() {
        let line = "![alt](img.png)";
        assert_eq!(
            detect(line, col_of(line, "img.png")),
            Some(PeekTarget::Image {
                path: "img.png".into()
            })
        );
        assert_eq!(
            detect(line, col_of(line, "alt")),
            Some(PeekTarget::Image {
                path: "img.png".into()
            })
        );
    }

    #[test]
    fn link_and_autolink() {
        let line = "see [example](https://x.org) or <https://y.org>";
        assert_eq!(
            detect(line, col_of(line, "https://x.org")),
            Some(PeekTarget::Link {
                url: "https://x.org".into()
            })
        );
        assert_eq!(
            detect(line, col_of(line, "https://y.org")),
            Some(PeekTarget::Link {
                url: "https://y.org".into()
            })
        );
    }

    #[test]
    fn footnote_detected() {
        let line = "see note[^1] here";
        assert_eq!(
            detect(line, col_of(line, "1]")),
            Some(PeekTarget::Footnote { id: "1".into() })
        );
    }

    #[test]
    fn word_fallback() {
        let line = "plain text";
        assert_eq!(
            detect(line, col_of(line, "text")),
            Some(PeekTarget::Word {
                word: "text".into()
            })
        );
        // On a delimiter away from words: no target.
        assert_eq!(detect("plain   text", 6), None);
    }

    #[test]
    fn word_detection_multi_byte_line() {
        let line = "héllo wörld";
        assert_eq!(
            word_at(line, 8),
            Some("wörld".into()),
            "char-offset expansion"
        );
    }

    #[test]
    fn footnote_definitions() {
        let text = "para[^1]\n\n[^1]: The note text\n    continued here\n\nmore";
        assert_eq!(
            footnote_definition(text, "1"),
            Some("The note text continued here".into())
        );
        assert_eq!(footnote_definition(text, "2"), None);
    }
}
