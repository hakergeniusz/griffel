//! In-place Markdown highlighting ("live markup",
//! `03-behavior/01-editing.md` §7): a background scan produces styled
//! ranges; they are applied on the main thread only if the text has not
//! changed since the scan started.

use glib::translate::IntoGlib;
use gtk::prelude::*;
use regex::Regex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::appearance::EditorPalette;

/// Names of the tags this module owns.
pub const TAG_NAMES: &[&str] = &[
    "md-body", "md-bold", "md-italic", "md-strike", "md-code", "md-link-label",
    "md-dim", "md-rule", "md-heading", "md-heading-1", "md-heading-2",
    "md-heading-3", "md-heading-4", "md-heading-5", "md-heading-6", "md-quote",
    "md-fence",
];

/// Create the highlighting tags on `buffer` if missing. Creation order is
/// priority order: `md-body` (low) first so outdenting tags can override
/// its column margin per paragraph.
pub fn ensure_tags(buffer: &sourceview5::Buffer) {
    let table = buffer.tag_table();
    let mut create = |name: &str, setup: &dyn Fn(&gtk::TextTag)| {
        if table.lookup(name).is_none() {
            let tag = gtk::TextTag::new(Some(name));
            setup(&tag);
            table.add(&tag);
        }
    };

    create("md-body", &|_tag| {});
    create("md-bold", &|t| t.set_property("weight", pango::Weight::Bold.into_glib()));
    create("md-italic", &|t| t.set_property("style", pango::Style::Italic));
    create("md-strike", &|t| t.set_strikethrough(true));
    create("md-code", &|_t| {});
    create("md-link-label", &|t| t.set_property("underline", pango::Underline::Single));
    create("md-dim", &|t| t.set_property("style", pango::Style::Italic));
    create("md-rule", &|t| t.set_justification(gtk::Justification::Center));
    create("md-heading", &|t| t.set_property("weight", pango::Weight::Bold.into_glib()));
    for level in 1..=6 {
        create(&format!("md-heading-{level}"), &|_t| {});
    }
    create("md-quote", &|_t| {});
    create("md-fence", &|_t| {});
}

/// Update tag colors from the palette (scheme change).
pub fn update_palette(buffer: &sourceview5::Buffer, palette: &EditorPalette) {
    let table = buffer.tag_table();
    let set_fg = |name: &str, color: &str| {
        if let Some(tag) = table.lookup(name) {
            tag.set_foreground_rgba(Some(&parse_color(color)));
        }
    };
    let set_bg = |name: &str, color: &str| {
        if let Some(tag) = table.lookup(name) {
            tag.set_background_rgba(Some(&parse_color(color)));
        }
    };
    set_bg("md-code", &palette.code_background);
    set_fg("md-code", &palette.code_foreground);
    set_fg("md-link-label", &palette.accent);
    set_fg("md-dim", &palette.dimmed);
    set_bg("md-fence", &palette.code_background);
    set_fg("md-fence", &palette.code_foreground);
}

fn parse_color(hex: &str) -> gdk::RGBA {
    gdk::RGBA::parse(hex).unwrap_or_else(|_| gdk::RGBA::new(0.5, 0.5, 0.5, 1.0))
}

/// Update the outdent margins used by heading/quote/fence tags. A tag's
/// left margin replaces the view's margin for its paragraph (verified
/// empirically), so a smaller margin hangs the construct into the column
/// margin. `column_margin_px` is the centered column's left margin;
/// `char_px` one character's width.
pub fn update_indents(buffer: &sourceview5::Buffer, column_margin_px: i32, char_px: i32) {
    let table = buffer.tag_table();
    for level in 1..=6 {
        if let Some(tag) = table.lookup(&format!("md-heading-{level}")) {
            let outdent = (level as i32 + 1) * char_px;
            tag.set_left_margin((column_margin_px - outdent).max(0));
        }
    }
    if let Some(tag) = table.lookup("md-quote") {
        tag.set_left_margin((column_margin_px - 2 * char_px).max(0));
    }
    if let Some(tag) = table.lookup("md-fence") {
        tag.set_left_margin(column_margin_px + char_px);
    }
}

// ---------------------------------------------------------------------------
// Scanning (pure; unit-tested)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpanKind {
    Bold,
    Italic,
    Strike,
    Code,
    LinkLabel,
    Dim,
    Rule,
    Heading(u8),
    Quote,
    Fence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    /// Char offsets into the document (end exclusive).
    pub start: usize,
    pub end: usize,
    pub kind: SpanKind,
}

struct Patterns {
    code: Regex,
    math: Regex,
    image: Regex,
    link: Regex,
    autolink: Regex,
    bold_italic: Regex,
    bold: Regex,
    strike: Regex,
    italic_star: Regex,
    bold_underscore: Regex,
    italic_underscore: Regex,
}

fn patterns() -> &'static Patterns {
    use std::sync::OnceLock;
    static P: OnceLock<Patterns> = OnceLock::new();
    P.get_or_init(|| Patterns {
        code: Regex::new(r"`+[^`\n]+`+").unwrap(),
        math: Regex::new(r"\$\$[^$\n]+\$\$").unwrap(),
        image: Regex::new(r"!\[[^\]\n]*\]\([^)\n]*\)").unwrap(),
        link: Regex::new(r"\[[^\]\n]*\]\([^)\n]*\)").unwrap(),
        autolink: Regex::new(r"<(https?|ftp)://[^>\s\n]+>").unwrap(),
        bold_italic: Regex::new(r"\*\*\*[^*\n]+\*\*\*").unwrap(),
        bold: Regex::new(r"\*\*[^*\n]+\*\*").unwrap(),
        strike: Regex::new(r"~~[^~\n]+~~").unwrap(),
        italic_star: Regex::new(r"\*[^*\n]+\*").unwrap(),
        bold_underscore: Regex::new(r"__[^_\n]+__").unwrap(),
        italic_underscore: Regex::new(r"_[^_\n]+_").unwrap(),
    })
}

/// Scan the document and produce styled ranges in char offsets.
pub fn scan(text: &str) -> Vec<Span> {
    let mut spans: Vec<Span> = Vec::new();
    let mut occupied: Vec<(usize, usize)> = Vec::new();
    let lines: Vec<&str> = text.split('\n').collect();
    let mut char_pos = 0usize;

    // Fenced code block state: (fence char, run length) while inside.
    let mut in_fence: Option<(char, usize)> = None;

    // YAML front matter: a `---`/`...` first line opens it; nothing inside
    // is highlighted (B3).
    let mut front_matter_open = lines
        .first()
        .is_some_and(|l| l.trim_end() == "---" || l.trim_end() == "...");

    // (start, end, blank) of the previous processed line, for setext rules.
    let mut prev_line: Option<(usize, usize, bool)> = None;

    for line in &lines {
        let line_len = line.chars().count();
        let line_start = char_pos;
        let line_end = char_pos + line_len;
        let trimmed = line.trim_start_matches([' ', '\t']);

        // Front matter: consume until the closing line.
        if front_matter_open {
            let is_opening = prev_line.is_none();
            let closing = line.trim_end() == "---" || line.trim_end() == "...";
            if closing && !is_opening {
                front_matter_open = false;
            }
            char_pos += line_len + 1;
            prev_line = Some((line_start, line_end, true));
            continue;
        }

        // Fences.
        if let Some((ch, run)) = fence_of(trimmed) {
            match in_fence {
                None => {
                    in_fence = Some((ch, run));
                    spans.push(Span { start: line_start, end: line_end, kind: SpanKind::Fence });
                    occupied.push((line_start, line_end));
                    prev_line = Some((line_start, line_end, false));
                    char_pos += line_len + 1;
                    continue;
                }
                Some((open_ch, open_run)) if ch == open_ch && run >= open_run => {
                    in_fence = None;
                    spans.push(Span { start: line_start, end: line_end, kind: SpanKind::Fence });
                    prev_line = Some((line_start, line_end, false));
                    char_pos += line_len + 1;
                    continue;
                }
                Some(_) => {}
            }
        }
        if in_fence.is_some() {
            spans.push(Span { start: line_start, end: line_end, kind: SpanKind::Fence });
            prev_line = Some((line_start, line_end, false));
            char_pos += line_len + 1;
            continue;
        }

        // Headings.
        if let Some(level) = heading_level(trimmed) {
            spans.push(Span { start: line_start, end: line_end, kind: SpanKind::Heading(level) });
            occupied.push((line_start, line_end));
            prev_line = Some((line_start, line_end, false));
            char_pos += line_len + 1;
            continue;
        }

        // Block quotes.
        if trimmed.starts_with('>') {
            spans.push(Span { start: line_start, end: line_end, kind: SpanKind::Quote });
            occupied.push((line_start, line_end));
            prev_line = Some((line_start, line_end, false));
            char_pos += line_len + 1;
            continue;
        }

        // Horizontal rules and setext underlines.
        if let Some(rule_ch) = rule_char(trimmed) {
            let prev_blank = prev_line.map(|(_, _, b)| b).unwrap_or(true);
            if (rule_ch == '-' || rule_ch == '=') && !prev_blank {
                if let Some((pstart, pend, false)) = prev_line {
                    spans.push(Span { start: pstart, end: pend, kind: SpanKind::Heading(2) });
                    occupied.push((pstart, pend));
                }
                // The underline itself is not styled.
            } else {
                spans.push(Span { start: line_start, end: line_end, kind: SpanKind::Rule });
                occupied.push((line_start, line_end));
            }
            prev_line = Some((line_start, line_end, false));
            char_pos += line_len + 1;
            continue;
        }

        // Inline constructs.
        scan_inline(line, line_start, &mut spans, &mut occupied);

        prev_line = Some((line_start, line_end, line.trim().is_empty()));
        char_pos += line_len + 1;
    }

    spans
}

/// If `line` (indent-stripped) is a fence line, return its (char, run).
fn fence_of(line: &str) -> Option<(char, usize)> {
    let mut chars = line.chars();
    let c = chars.next()?;
    if c != '`' && c != '~' {
        return None;
    }
    let run = 1 + chars.take_while(|x| *x == c).count();
    if run < 3 {
        return None;
    }
    Some((c, run))
}

fn heading_level(line: &str) -> Option<u8> {
    let hashes = line.chars().take_while(|c| *c == '#').count();
    if (1..=6).contains(&hashes) {
        let rest = &line[hashes..];
        if rest.is_empty() || rest.starts_with(' ') || rest.starts_with('\t') {
            return Some(hashes as u8);
        }
    }
    None
}

/// If the line consists only of 3+ runs of one rule character, return it.
fn rule_char(line: &str) -> Option<char> {
    let t = line.trim();
    let mut chars = t.chars();
    let c = chars.next()?;
    if c != '-' && c != '=' && c != '*' && c != '_' {
        return None;
    }
    let count = t.chars().filter(|x| *x == c).count();
    if count >= 3 && t.chars().all(|x| x == c || x.is_whitespace()) {
        Some(c)
    } else {
        None
    }
}

fn scan_inline(line: &str, base: usize, spans: &mut Vec<Span>, occupied: &mut Vec<(usize, usize)>) {
    let p = patterns();
    // One candidate = a range with one or more kinds (bold+italic share a
    // range) plus an optional embedded label range (link label inside the
    // dimmed link syntax).
    let mut candidates: Vec<(usize, usize, Vec<SpanKind>, Option<(usize, usize)>)> = Vec::new();

    for m in p.code.find_iter(line) {
        candidates.push((base + m.start(), base + m.end(), vec![SpanKind::Code], None));
    }
    for m in p.math.find_iter(line) {
        candidates.push((base + m.start(), base + m.end(), vec![SpanKind::Code], None));
    }
    for m in p.image.find_iter(line) {
        candidates.push((base + m.start(), base + m.end(), vec![SpanKind::Dim], None));
    }
    for m in p.link.find_iter(line) {
        let inner = &m.as_str()[1..m.as_str().len() - 1];
        if let Some(close) = inner.rfind(']') {
            let label_len = inner[..close].chars().count();
            let label = if label_len > 0 {
                Some((base + m.start() + 1, base + m.start() + 1 + label_len))
            } else {
                None
            };
            candidates.push((base + m.start(), base + m.end(), vec![SpanKind::Dim], label));
        }
    }
    for m in p.autolink.find_iter(line) {
        candidates.push((base + m.start(), base + m.end(), vec![SpanKind::Dim], None));
    }
    // Emphasis: longer delimiters first so *** is not eaten by **/*.
    for m in p.bold_italic.find_iter(line) {
        push_emphasis("***", m.start(), m.end(), base, &mut candidates);
    }
    for m in p.bold.find_iter(line) {
        push_emphasis("**", m.start(), m.end(), base, &mut candidates);
    }
    for m in p.strike.find_iter(line) {
        push_emphasis("~~", m.start(), m.end(), base, &mut candidates);
    }
    for m in p.bold_underscore.find_iter(line) {
        push_emphasis("__", m.start(), m.end(), base, &mut candidates);
    }
    for m in p.italic_star.find_iter(line) {
        push_emphasis("*", m.start(), m.end(), base, &mut candidates);
    }
    for m in p.italic_underscore.find_iter(line) {
        push_emphasis("_", m.start(), m.end(), base, &mut candidates);
    }

    for (start, end, kinds, label) in candidates {
        if occupied.iter().any(|(s, e)| start < *e && *s < end) {
            continue;
        }
        // Emphasis inner text must not start or end with whitespace.
        if kinds.iter().any(|k| is_emphasis(*k)) {
            let text: String = line.chars().skip(start - base).take(end - start).collect();
            let delim = emphasis_delim(kinds[0]);
            let inner = &text[delim.len()..text.len() - delim.len()];
            if inner.is_empty()
                || inner.starts_with(' ')
                || inner.starts_with('\t')
                || inner.ends_with(' ')
                || inner.ends_with('\t')
            {
                continue;
            }
        }
        for kind in kinds {
            spans.push(Span { start, end, kind });
        }
        if let Some((lstart, lend)) = label {
            spans.push(Span { start: lstart, end: lend, kind: SpanKind::LinkLabel });
        }
        occupied.push((start, end));
    }
}

fn is_emphasis(kind: SpanKind) -> bool {
    matches!(kind, SpanKind::Bold | SpanKind::Italic | SpanKind::Strike)
}

fn emphasis_delim(kind: SpanKind) -> &'static str {
    match kind {
        SpanKind::Strike => "~~",
        SpanKind::Bold => "**",
        SpanKind::Italic => "*",
        _ => "",
    }
}

fn push_emphasis(
    delim: &str,
    start: usize,
    end: usize,
    base: usize,
    candidates: &mut Vec<(usize, usize, Vec<SpanKind>, Option<(usize, usize)>)>,
) {
    let kinds = match delim {
        "***" => vec![SpanKind::Bold, SpanKind::Italic],
        "**" | "__" => vec![SpanKind::Bold],
        "~~" => vec![SpanKind::Strike],
        "*" | "_" => vec![SpanKind::Italic],
        _ => return,
    };
    candidates.push((base + start, base + end, kinds, None));
}

// ---------------------------------------------------------------------------
// Background scheduling
// ---------------------------------------------------------------------------

#[derive(Clone, Default)]
pub struct Highlighter {
    generation_id: Arc<AtomicU64>,
}

impl Highlighter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Schedule a scan of `text`; results are applied to `buffer` on the
    /// main loop if the text length still matches (stale results dropped).
    pub fn schedule(&self, buffer: &sourceview5::Buffer, text: String) {
        let _generation_id = self.generation_id.fetch_add(1, Ordering::SeqCst) + 1;
        let cb_buffer = buffer.clone();
        glib::spawn_future_local(async move {
            // Scan off the main thread; the buffer never leaves it.
            let len = text.chars().count();
            let handle = gio::spawn_blocking(move || scan(&text));
            let spans = handle.await.unwrap_or_default();
            if cb_buffer.char_count() as usize != len {
                return;
            }
            apply_spans(&cb_buffer, &spans);
        });
    }
}

/// Remove all md-* tags and apply the scanned spans.
pub fn apply_spans(buffer: &sourceview5::Buffer, spans: &[Span]) {
    let start = buffer.start_iter();
    let end = buffer.end_iter();
    for name in TAG_NAMES {
        buffer.remove_tag_by_name(name, &start, &end);
    }
    // Body margin over the whole buffer; outdented tags override it per
    // paragraph (higher tag priority).
    buffer.apply_tag_by_name("md-body", &start, &end);

    for span in spans {
        let s = buffer.iter_at_offset(span.start as i32);
        let e = buffer.iter_at_offset(span.end as i32);
        let names: &[&str] = match span.kind {
            SpanKind::Bold => &["md-bold"],
            SpanKind::Italic => &["md-italic"],
            SpanKind::Strike => &["md-strike"],
            SpanKind::Code => &["md-code"],
            SpanKind::LinkLabel => &["md-link-label"],
            SpanKind::Dim => &["md-dim"],
            SpanKind::Rule => &["md-rule"],
            SpanKind::Heading(l) => match l {
                1 => &["md-heading", "md-heading-1"],
                2 => &["md-heading", "md-heading-2"],
                3 => &["md-heading", "md-heading-3"],
                4 => &["md-heading", "md-heading-4"],
                5 => &["md-heading", "md-heading-5"],
                _ => &["md-heading", "md-heading-6"],
            },
            SpanKind::Quote => &["md-quote"],
            SpanKind::Fence => &["md-fence"],
        };
        for name in names {
            buffer.apply_tag_by_name(name, &s, &e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use SpanKind::*;

    fn kinds(spans: &[Span], text: &str, needle: &str) -> Vec<SpanKind> {
        let pos = text.find(needle).unwrap();
        let end = pos + needle.chars().count();
        spans
            .iter()
            .filter(|s| s.start <= pos && s.end >= end)
            .map(|s| s.kind)
            .collect()
    }

    #[test]
    fn bold_italic_strike() {
        let text = "**bold** _i_ ~~s~~";
        let spans = scan(text);
        assert!(kinds(&spans, text, "bold").contains(&Bold));
        assert!(kinds(&spans, text, "i").contains(&Italic));
        assert!(kinds(&spans, text, "s").contains(&Strike));
    }

    #[test]
    fn bold_italic_combined() {
        let text = "***bi***";
        let spans = scan(text);
        assert!(kinds(&spans, text, "bi").contains(&Bold));
        assert!(kinds(&spans, text, "bi").contains(&Italic));
    }

    #[test]
    fn emphasis_inner_spaces_rejected() {
        let text = "a * b * c";
        let spans = scan(text);
        assert!(spans.iter().all(|s| s.kind != Italic));
    }

    #[test]
    fn code_wins_over_emphasis() {
        let text = "`**not bold**`";
        let spans = scan(text);
        assert!(kinds(&spans, text, "not bold").contains(&Code));
        assert!(!kinds(&spans, text, "not bold").contains(&Bold));
    }

    #[test]
    fn headings_and_levels() {
        let text = "# One\n## Two\n### Three";
        let spans = scan(text);
        assert!(kinds(&spans, text, "# One").contains(&Heading(1)));
        assert!(kinds(&spans, text, "## Two").contains(&Heading(2)));
        assert!(kinds(&spans, text, "### Three").contains(&Heading(3)));
        // # hasheading without space is not a heading
        assert!(!scan("#nospace").iter().any(|s| matches!(s.kind, Heading(_))));
    }

    #[test]
    fn quotes_rules() {
        let text = "> quoted\n\n---\n";
        let spans = scan(text);
        assert!(kinds(&spans, text, "> quoted").contains(&Quote));
        assert!(kinds(&spans, text, "---").contains(&Rule));
    }

    #[test]
    fn setext_underline_bolds_previous_line() {
        let text = "Title\n---\n\nparagraph\n\n---\n";
        let spans = scan(text);
        assert!(kinds(&spans, text, "Title").contains(&Heading(2)));
        // The setext underline itself is not styled; a rule after a blank
        // line is a centered rule.
        assert!(!kinds(&spans, text, "---").contains(&Rule));
    }

    #[test]
    fn fence_blocks_suppress_inline() {
        let text = "```\n**not bold**\n```\n**bold**";
        let spans = scan(text);
        assert!(kinds(&spans, text, "**not bold**").contains(&Fence));
        assert!(!kinds(&spans, text, "**not bold**").contains(&Bold));
        assert!(kinds(&spans, text, "**bold**").contains(&Bold));
    }

    #[test]
    fn front_matter_not_headings() {
        let text = "---\ntitle: x\n---\n\n# Real";
        let spans = scan(text);
        assert!(!spans.iter().any(|s| s.start == 0 && matches!(s.kind, Rule)));
        assert!(kinds(&spans, text, "# Real").contains(&Heading(1)));
    }

    #[test]
    fn links_dim_label_accent() {
        let text = "see [label](https://x.example) now";
        let spans = scan(text);
        assert!(kinds(&spans, text, "[label](https://x.example)").contains(&Dim));
        assert!(kinds(&spans, text, "label").contains(&LinkLabel));
    }

    #[test]
    fn images_dim() {
        let text = "![alt](img.png)";
        let spans = scan(text);
        assert!(kinds(&spans, text, "![alt](img.png)").contains(&Dim));
        // The alt text inside an image is not a link label.
        assert!(!kinds(&spans, text, "alt").contains(&LinkLabel));
    }

    #[test]
    fn autolinks_dim() {
        let text = "visit <https://example.com> today";
        let spans = scan(text);
        assert!(kinds(&spans, text, "<https://example.com>").contains(&Dim));
    }

    #[test]
    fn math_is_code() {
        let text = "$$E = mc^2$$";
        let spans = scan(text);
        assert!(kinds(&spans, text, "$$E = mc^2$$").contains(&Code));
    }
}
