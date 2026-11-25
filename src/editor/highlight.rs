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
use crate::editor::line_cache::LineProcessor;

/// Names of the tags this module owns.
pub const TAG_NAMES: &[&str] = &[
    "md-body", "md-bold", "md-italic", "md-strike", "md-code", "md-link-label",
    "md-dim", "md-rule", "md-heading", "md-heading-1", "md-heading-2",
    "md-heading-3", "md-heading-4", "md-heading-5", "md-heading-6", "md-quote",
    "md-fence", "md-code-str", "md-code-num", "md-code-comment", "md-code-keyword",
];

/// Create the highlighting tags on `buffer` if missing. Creation order is
/// priority order: `md-body` (low) first so outdenting tags can override
/// its column margin per paragraph; the Code Mode token tags come last so
/// their foreground wins over the fence background tag.
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
    create("md-code-str", &|_t| {});
    create("md-code-num", &|_t| {});
    create("md-code-comment", &|t| t.set_property("style", pango::Style::Italic));
    create("md-code-keyword", &|_t| {});
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
    set_fg("md-code-str", &palette.code_string);
    set_fg("md-code-num", &palette.code_number);
    set_fg("md-code-comment", &palette.code_comment);
    set_fg("md-code-keyword", &palette.code_keyword);
}

pub(crate) fn parse_color(hex: &str) -> gdk::RGBA {
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
    /// Code Mode token inside a fenced block.
    CodeStr,
    CodeNum,
    CodeComment,
    CodeKeyword,
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

/// Scan the document and produce styled ranges in char offsets. With
/// `code_mode`, fenced code block interiors additionally get lightweight
/// code-token colors (strings, numbers, comments, keywords).
///
/// This is the reference implementation: a fold of `scan_line` over all
/// lines. The incremental `DocHighlight` runs the same per-line core and
/// must agree with it exactly (property-tested).
pub fn scan(text: &str, code_mode: bool) -> Vec<Span> {
    let mut spans: Vec<Span> = Vec::new();
    let mut state = HlState { code_mode, ..Default::default() };
    let mut proc = HlProcessor;
    // (start, end) of the previous line, for setext underlines.
    let mut prev_line: Option<(usize, usize)> = None;
    let mut char_pos = 0usize;
    for line in text.split('\n') {
        let line_len = line.chars().count();
        let line_start = char_pos;
        let line_end = char_pos + line_len;
        let (value, out) = proc.process(line, &state, prev_line.is_none());
        if let Some(level) = value.setext_prev {
            if let Some((pstart, pend)) = prev_line {
                spans.push(Span { start: pstart, end: pend, kind: SpanKind::Heading(level) });
            }
        }
        for s in &value.spans {
            spans.push(Span {
                start: line_start + s.start,
                end: line_start + s.end,
                kind: s.kind,
            });
        }
        prev_line = Some((line_start, line_end));
        state = out;
        char_pos = line_end + 1;
    }
    spans
}

/// Cross-line state entering a line during the scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HlState {
    /// Open fenced code block: (fence char, run length).
    pub fence: Option<(char, usize)>,
    /// YAML front matter is open (`03-behavior/01-editing.md` B3).
    pub front_matter: bool,
    /// The previous line was blank (setext rule).
    pub prev_blank: bool,
    /// This is the first line (front matter can only open here).
    pub at_start: bool,
    /// Code Mode token coloring — part of the state so a toggle invalidates
    /// every cached fenced line.
    pub code_mode: bool,
}

impl HlState {
    /// The state entering the line after this one.
    fn advance(&mut self, prev_blank: bool) {
        self.prev_blank = prev_blank;
        self.at_start = false;
    }
}

/// Styled ranges of one line, offsets relative to the line start.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HlValue {
    pub spans: Vec<Span>,
    /// Setext underline: heading level applied to the PREVIOUS line.
    pub setext_prev: Option<u8>,
}

struct HlProcessor;

impl crate::editor::line_cache::LineProcessor for HlProcessor {
    type State = HlState;
    type Value = HlValue;

    fn process(
        &mut self,
        line: &str,
        st: &HlState,
        _is_last: bool,
    ) -> (HlValue, HlState) {
        let mut value = HlValue::default();
        let mut out = *st;
        out.at_start = false;

        let line_len = line.chars().count();
        let trimmed = line.trim_start_matches([' ', '\t']);
        // Overlap bookkeeping is line-local: block spans always cover whole
        // lines and inline candidates never leave their line, so nothing
        // outside this line can conflict (the previous document-wide list
        // made the check quadratic on block-heavy documents).
        let mut occupied: Vec<(usize, usize)> = Vec::new();

        // Front matter: consume until the closing line.
        if st.front_matter {
            let is_opening = st.at_start;
            let closing = line.trim_end() == "---" || line.trim_end() == "...";
            if closing && !is_opening {
                out.front_matter = false;
            }
            out.advance(true);
            return (value, out);
        }

        // Fences.
        if let Some((ch, run)) = fence_of(trimmed) {
            match st.fence {
                None => {
                    out.fence = Some((ch, run));
                    value.spans.push(Span { start: 0, end: line_len, kind: SpanKind::Fence });
                    out.advance(false);
                    return (value, out);
                }
                Some((open_ch, open_run)) if ch == open_ch && run >= open_run => {
                    out.fence = None;
                    value.spans.push(Span { start: 0, end: line_len, kind: SpanKind::Fence });
                    out.advance(false);
                    return (value, out);
                }
                Some(_) => {}
            }
        }
        if st.fence.is_some() {
            value.spans.push(Span { start: 0, end: line_len, kind: SpanKind::Fence });
            if st.code_mode {
                scan_code_line(line, 0, &mut value.spans);
            }
            out.advance(false);
            return (value, out);
        }

        // Headings.
        if let Some(level) = heading_level(trimmed) {
            value.spans.push(Span { start: 0, end: line_len, kind: SpanKind::Heading(level) });
            out.advance(false);
            return (value, out);
        }

        // Block quotes.
        if trimmed.starts_with('>') {
            value.spans.push(Span { start: 0, end: line_len, kind: SpanKind::Quote });
            out.advance(false);
            return (value, out);
        }

        // Horizontal rules and setext underlines.
        if let Some(rule_ch) = rule_char(trimmed) {
            if (rule_ch == '-' || rule_ch == '=') && !st.prev_blank {
                // Setext underline: the previous line becomes an H2. The
                // underline itself is not styled.
                value.setext_prev = Some(2);
                out.advance(false);
                return (value, out);
            }
            value.spans.push(Span { start: 0, end: line_len, kind: SpanKind::Rule });
            out.advance(false);
            return (value, out);
        }

        // Inline constructs.
        scan_inline(line, 0, &mut value.spans, &mut occupied);
        out.advance(line.trim().is_empty());
        (value, out)
    }
}

/// Incrementally maintained highlighting for one document: per-line spans
/// plus the cross-line scan state, synced through the shared line cache.
#[derive(Clone, Default)]
pub struct DocHighlight {
    cache: crate::editor::line_cache::LineCache<HlProcessor>,
    code_mode: bool,
}

impl DocHighlight {
    /// Sync with `text`; `dirty_from` is the conservative first-affected
    /// line hint (0 = rebuild). Returns the inclusive line ranges whose
    /// spans were recomputed (re-tagging those ranges on the buffer is
    /// sufficient).
    pub fn update(
        &mut self,
        text: &str,
        dirty_from: usize,
        code_mode: bool,
    ) -> Vec<(usize, usize)> {
        // A toggle changes the state every cached line was computed with,
        // so the whole document must be walked regardless of the hint.
        let toggled = code_mode != self.code_mode;
        self.code_mode = code_mode;
        let mut initial = HlState::default();
        initial.code_mode = code_mode;
        let mut proc = HlProcessor;
        let dirty = if toggled { 0 } else { dirty_from };
        let changed = self.cache.update_with(text, dirty, &mut proc, &initial);
        if toggled && changed.is_empty() && self.cache.line_count() > 0 {
            return vec![(0, self.cache.line_count() - 1)];
        }
        changed
    }

    pub fn line_count(&self) -> usize {
        self.cache.line_count()
    }

    fn entry(&self, line: usize) -> Option<&std::sync::Arc<crate::editor::line_cache::Entry<HlProcessor>>> {
        self.cache.entries().get(line)
    }
}

/// Sorted keyword list for the Code Mode tokenizer (binary searched).
/// Deliberately cross-language and small: "some code colors", not a
/// grammar.
const CODE_KEYWORDS: &[&str] = &[
    "False", "None", "True", "abstract", "and", "as", "async", "await", "bool",
    "break", "case", "catch", "char", "class", "const", "continue", "def",
    "default", "del", "do", "double", "elif", "else", "end", "enum", "except",
    "extends", "false", "final", "finally", "float", "fn", "for", "from",
    "func", "function", "if", "impl", "import", "in", "include", "int",
    "interface", "is", "lambda", "let", "long", "match", "mod", "mut", "new",
    "nil", "not", "null", "of", "or", "package", "pass", "print", "priv",
    "private", "protected", "pub", "public", "raise", "return", "self",
    "static", "str", "struct", "super", "switch", "then", "this", "throw",
    "throws", "trait", "true", "try", "type", "typedef", "union", "unsigned",
    "use", "var", "void", "while", "with", "yield",
];

/// Color strings, comments, numbers and keywords on one fenced-block line
/// (Code Mode). Line-local and deliberately simple; token spans never
/// overlap because the scan is a single left-to-right pass.
fn scan_code_line(line: &str, base: usize, spans: &mut Vec<Span>) {
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();
    let mut i = 0usize;
    while i < len {
        let c = chars[i];
        let prev_wordish = i > 0 && (chars[i - 1].is_alphanumeric() || chars[i - 1] == '_');

        // `#` comments only when followed by whitespace/EOL, so C-style
        // `#include` stays out; `//` comments skip `://` in URLs.
        if c == '#' && (i + 1 >= len || chars[i + 1].is_whitespace()) {
            spans.push(Span { start: base + i, end: base + len, kind: SpanKind::CodeComment });
            return;
        }
        if c == '/' && i + 1 < len && chars[i + 1] == '/' && (i == 0 || chars[i - 1] != ':') {
            spans.push(Span { start: base + i, end: base + len, kind: SpanKind::CodeComment });
            return;
        }
        if c == '"' || c == '\'' || c == '`' {
            let start = i;
            i += 1;
            while i < len {
                if chars[i] == '\\' {
                    i += 2;
                    continue;
                }
                if chars[i] == c {
                    i += 1;
                    break;
                }
                i += 1;
            }
            spans.push(Span {
                start: base + start,
                end: base + i.min(len),
                kind: SpanKind::CodeStr,
            });
            continue;
        }
        if c.is_ascii_digit() && !prev_wordish {
            let start = i;
            if c == '0' && i + 1 < len && (chars[i + 1] == 'x' || chars[i + 1] == 'X') {
                i += 2;
                while i < len && chars[i].is_ascii_hexdigit() {
                    i += 1;
                }
            } else {
                while i < len && chars[i].is_ascii_digit() {
                    i += 1;
                }
                if i + 1 < len && chars[i] == '.' && chars[i + 1].is_ascii_digit() {
                    i += 1;
                    while i < len && chars[i].is_ascii_digit() {
                        i += 1;
                    }
                }
            }
            spans.push(Span { start: base + start, end: base + i, kind: SpanKind::CodeNum });
            continue;
        }
        if (c.is_alphabetic() || c == '_') && !prev_wordish {
            let start = i;
            while i < len && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            if CODE_KEYWORDS.binary_search(&word.as_str()).is_ok() {
                spans.push(Span {
                    start: base + start,
                    end: base + start + word.chars().count(),
                    kind: SpanKind::CodeKeyword,
                });
            }
            continue;
        }
        i += 1;
    }
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
    generation: Arc<AtomicU64>,
}

impl Highlighter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Schedule an incremental scan: `cache` is the current main-thread
    /// document state, `dirty_from` the conservative first-affected line.
    /// On completion (still current generation) the new state is adopted
    /// via `adopt` and only the changed line ranges are re-tagged on
    /// `buffer` — stale results are dropped whole, which also fixes the
    /// previous char-count-only staleness check (a char-for-char
    /// replacement could apply tags for the wrong text).
    pub fn schedule(
        &self,
        buffer: &sourceview5::Buffer,
        text: Arc<String>,
        cache: Arc<DocHighlight>,
        dirty_from: usize,
        code_mode: bool,
        adopt: impl Fn(Arc<DocHighlight>) + 'static,
    ) {
        let generation_id = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let generation = self.generation.clone();
        let cb_buffer = buffer.clone();
        glib::spawn_future_local(async move {
            // Scan off the main thread; the buffer never leaves it.
            let handle = gio::spawn_blocking(move || {
                let mut doc = (*cache).clone();
                let changed = doc.update(&text, dirty_from, code_mode);
                (Arc::new(doc), changed)
            });
            let Ok((doc, changed)) = handle.await else {
                return;
            };
            if generation.load(Ordering::SeqCst) != generation_id {
                return;
            }
            apply_ranges(&cb_buffer, &doc, &changed);
            adopt(doc);
        });
    }
}

/// GTK tag names for one span kind (shared by the whole-range and the
/// per-line apply paths).
fn tag_names(kind: SpanKind) -> &'static [&'static str] {
    match kind {
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
        SpanKind::CodeStr => &["md-code-str"],
        SpanKind::CodeNum => &["md-code-num"],
        SpanKind::CodeComment => &["md-code-comment"],
        SpanKind::CodeKeyword => &["md-code-keyword"],
    }
}

/// Re-tag only the given line ranges (inclusive, per the cache update).
/// Untouched lines keep their tags: GTK moves tag boundaries with the
/// text, so an edit elsewhere never invalidates them.
pub fn apply_ranges(
    buffer: &sourceview5::Buffer,
    doc: &DocHighlight,
    ranges: &[(usize, usize)],
) {
    let all = doc.line_count();
    let full = ranges.len() == 1 && ranges[0] == (0, all.saturating_sub(1));
    if full {
        // Fresh document (load, Code Mode toggle): one whole-range clear
        // beats per-line clears.
        let start = buffer.start_iter();
        let end = buffer.end_iter();
        for name in TAG_NAMES {
            buffer.remove_tag_by_name(name, &start, &end);
        }
    }
    for (first, last) in ranges {
        for line in *first..=*last {
            apply_line(buffer, doc, line, full);
        }
    }
}

/// Re-tag one line: clear the owned tags, apply the body margin, then the
/// line's spans and (when this line is a setext underline) the heading on
/// the previous line.
fn apply_line(
    buffer: &sourceview5::Buffer,
    doc: &DocHighlight,
    line: usize,
    already_cleared: bool,
) {
    let Some(entry) = doc.entry(line) else {
        return;
    };
    let Some(mut start) = buffer.iter_at_line(line as i32) else {
        return;
    };
    let mut end = start.clone();
    end.forward_to_line_end();
    if !already_cleared {
        for name in TAG_NAMES {
            buffer.remove_tag_by_name(name, &start, &end);
        }
    }
    // Body margin over the line; outdented tags override it (higher tag
    // priority).
    buffer.apply_tag_by_name("md-body", &start, &end);
    for span in &entry.value.spans {
        let Some(s) = buffer.iter_at_line_offset(line as i32, span.start as i32) else {
            continue;
        };
        let Some(e) = buffer.iter_at_line_offset(line as i32, span.end as i32) else {
            continue;
        };
        for name in tag_names(span.kind) {
            buffer.apply_tag_by_name(name, &s, &e);
        }
    }
    // Setext underline: the heading lives on the previous line (its own
    // tags are otherwise untouched).
    if let Some(level) = entry.value.setext_prev {
        if line > 0 {
            if let (Some(mut ps), Some(mut pe)) = (
                buffer.iter_at_line(line as i32 - 1),
                buffer.iter_at_line(line as i32 - 1),
            ) {
                pe.forward_to_line_end();
                for name in TAG_NAMES {
                    buffer.remove_tag_by_name(name, &ps, &pe);
                }
                buffer.apply_tag_by_name("md-body", &ps, &pe);
                for name in tag_names(SpanKind::Heading(level)) {
                    buffer.apply_tag_by_name(name, &ps, &pe);
                }
            }
        }
    }
}

/// Remove all md-* tags and apply the scanned spans (whole-document form,
/// used by callers holding a plain scan result).
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
        for name in tag_names(span.kind) {
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
        let spans = scan(text, false);
        assert!(kinds(&spans, text, "bold").contains(&Bold));
        assert!(kinds(&spans, text, "i").contains(&Italic));
        assert!(kinds(&spans, text, "s").contains(&Strike));
    }

    #[test]
    fn bold_italic_combined() {
        let text = "***bi***";
        let spans = scan(text, false);
        assert!(kinds(&spans, text, "bi").contains(&Bold));
        assert!(kinds(&spans, text, "bi").contains(&Italic));
    }

    #[test]
    fn emphasis_inner_spaces_rejected() {
        let text = "a * b * c";
        let spans = scan(text, false);
        assert!(spans.iter().all(|s| s.kind != Italic));
    }

    #[test]
    fn code_wins_over_emphasis() {
        let text = "`**not bold**`";
        let spans = scan(text, false);
        assert!(kinds(&spans, text, "not bold").contains(&Code));
        assert!(!kinds(&spans, text, "not bold").contains(&Bold));
    }

    #[test]
    fn headings_and_levels() {
        let text = "# One\n## Two\n### Three";
        let spans = scan(text, false);
        assert!(kinds(&spans, text, "# One").contains(&Heading(1)));
        assert!(kinds(&spans, text, "## Two").contains(&Heading(2)));
        assert!(kinds(&spans, text, "### Three").contains(&Heading(3)));
        // # hasheading without space is not a heading
        assert!(!scan("#nospace", false).iter().any(|s| matches!(s.kind, Heading(_))));
    }

    #[test]
    fn quotes_rules() {
        let text = "> quoted\n\n---\n";
        let spans = scan(text, false);
        assert!(kinds(&spans, text, "> quoted").contains(&Quote));
        assert!(kinds(&spans, text, "---").contains(&Rule));
    }

    #[test]
    fn setext_underline_bolds_previous_line() {
        let text = "Title\n---\n\nparagraph\n\n---\n";
        let spans = scan(text, false);
        assert!(kinds(&spans, text, "Title").contains(&Heading(2)));
        // The setext underline itself is not styled; a rule after a blank
        // line is a centered rule.
        assert!(!kinds(&spans, text, "---").contains(&Rule));
    }

    #[test]
    fn fence_blocks_suppress_inline() {
        let text = "```\n**not bold**\n```\n**bold**";
        let spans = scan(text, false);
        assert!(kinds(&spans, text, "**not bold**").contains(&Fence));
        assert!(!kinds(&spans, text, "**not bold**").contains(&Bold));
        assert!(kinds(&spans, text, "**bold**").contains(&Bold));
    }

    #[test]
    fn front_matter_not_headings() {
        let text = "---\ntitle: x\n---\n\n# Real";
        let spans = scan(text, false);
        assert!(!spans.iter().any(|s| s.start == 0 && matches!(s.kind, Rule)));
        assert!(kinds(&spans, text, "# Real").contains(&Heading(1)));
    }

    #[test]
    fn links_dim_label_accent() {
        let text = "see [label](https://x.example) now";
        let spans = scan(text, false);
        assert!(kinds(&spans, text, "[label](https://x.example)").contains(&Dim));
        assert!(kinds(&spans, text, "label").contains(&LinkLabel));
    }

    #[test]
    fn images_dim() {
        let text = "![alt](img.png)";
        let spans = scan(text, false);
        assert!(kinds(&spans, text, "![alt](img.png)").contains(&Dim));
        // The alt text inside an image is not a link label.
        assert!(!kinds(&spans, text, "alt").contains(&LinkLabel));
    }

    #[test]
    fn autolinks_dim() {
        let text = "visit <https://example.com> today";
        let spans = scan(text, false);
        assert!(kinds(&spans, text, "<https://example.com>").contains(&Dim));
    }

    #[test]
    fn math_is_code() {
        let text = "$$E = mc^2$$";
        let spans = scan(text, false);
        assert!(kinds(&spans, text, "$$E = mc^2$$").contains(&Code));
    }

    #[test]
    fn code_mode_tokens_in_fences() {
        let text = "```\nlet x = 42 # note\ny = \"hi\"\n```\nlet outside = 1";
        let spans = scan(text, true);
        assert!(kinds(&spans, text, "let").contains(&CodeKeyword));
        assert!(kinds(&spans, text, "42").contains(&CodeNum));
        assert!(kinds(&spans, text, "# note").contains(&CodeComment));
        assert!(kinds(&spans, text, "\"hi\"").contains(&CodeStr));
        // Outside fences nothing is tokenized.
        assert!(!kinds(&spans, text, "outside").contains(&CodeKeyword));
        assert!(!scan(text, false).iter().any(|s| matches!(
            s.kind,
            CodeStr | CodeNum | CodeComment | CodeKeyword
        )));
    }

    #[test]
    fn code_mode_url_not_comment() {
        let text = "```\nfetch(https://x.example)\n```";
        let spans = scan(text, true);
        assert!(!spans.iter().any(|s| s.kind == CodeComment));
        // The word `fetch` is not a keyword either.
        assert!(!kinds(&spans, text, "fetch").contains(&CodeKeyword));
    }

    #[test]
    fn code_mode_escapes_and_hex() {
        let text = "```\ns = \"a\\\"b\" + 0xFF + 3.14\n```";
        let spans = scan(text, true);
        assert!(kinds(&spans, text, "\"a\\\"b\"").contains(&CodeStr));
        assert!(kinds(&spans, text, "0xFF").contains(&CodeNum));
        assert!(kinds(&spans, text, "3.14").contains(&CodeNum));
    }

    #[test]
    fn code_mode_hash_needs_space() {
        let text = "```\n#include <x.h>\n# real comment\n```";
        let spans = scan(text, true);
        assert!(kinds(&spans, text, "# real comment").contains(&CodeComment));
        assert!(!kinds(&spans, text, "#include").contains(&CodeComment));
        assert!(kinds(&spans, text, "include").contains(&CodeKeyword));
    }
}

#[cfg(test)]
mod incremental {
    use super::*;
    use crate::editor::line_cache::LineProcessor as _;
    use std::time::Instant;

    /// Deterministic xorshift PRNG — reproducible property tests without
    /// a dependency.
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x;
            x
        }
        fn below(&mut self, n: usize) -> usize {
            (self.next() % n as u64) as usize
        }
    }

    const LINES: &[&str] = &[
        "# Heading one",
        "## Sub",
        "",
        "- item one",
        "  - nested item",
        "> quoted **text**",
        "```",
        "let x = 42; // note",
        "```",
        "plain *text* with [link](https://x.example) and `code`",
        "| a | b |",
        "|---|---|",
        "---",
        "setext body",
        "===",
        "~~~",
        "tilde ~fenced~",
        "~~~",
        "***rule***",
        "trailing sentence. Another one!",
    ];

    fn random_doc(rng: &mut Rng, lines: usize) -> String {
        let mut out = String::new();
        for i in 0..lines {
            if i > 0 {
                out.push('\n');
            }
            out.push_str(LINES[rng.below(LINES.len())]);
        }
        out
    }

    fn random_edit(rng: &mut Rng, text: &mut String) -> usize {
        let pos = rng.below(text.len() + 1);
        let mut pos = pos;
        while !text.is_char_boundary(pos) {
            pos += 1;
        }
        let hint = text[..pos].matches('\n').count();
        match rng.below(3) {
            0 => text.insert_str(pos, LINES[rng.below(LINES.len())]),
            1 => {
                let mut end = (pos + rng.below(30)).min(text.len());
                while !text.is_char_boundary(end) {
                    end += 1;
                }
                text.replace_range(pos..end, "");
            }
            _ => text.insert_str(pos, "\n"),
        }
        hint
    }

    /// Expand the incremental cache to document-absolute spans in `scan`'s
    /// output order (setext span first, then the line's own spans).
    fn doc_spans(doc: &DocHighlight, text: &str) -> Vec<Span> {
        let lines: Vec<&str> = text.split('\n').collect();
        let mut out = Vec::new();
        let mut pos = 0usize;
        for (i, line) in lines.iter().enumerate() {
            if let Some(entry) = doc.entry(i) {
                if let Some(level) = entry.value.setext_prev {
                    if i > 0 {
                        let prev_len = lines[i - 1].chars().count();
                        out.push(Span {
                            start: pos - prev_len - 1,
                            end: pos - 1,
                            kind: SpanKind::Heading(level),
                        });
                    }
                }
                for s in &entry.value.spans {
                    out.push(Span { start: pos + s.start, end: pos + s.end, kind: s.kind });
                }
            }
            pos += line.chars().count() + 1;
        }
        out
    }

    /// The incremental cache must agree with the reference `scan()` after
    /// every edit — the two share the per-line core, but the resync walk,
    /// state propagation and dirty hints are where divergence would hide.
    #[test]
    fn incremental_matches_scan_through_random_edits() {
        let mut rng = Rng(0xa11c_e5c0_0000_0001);
        for round in 0..30 {
            let lines = 5 + rng.below(20);
            let mut text = random_doc(&mut rng, lines);
            for code_mode in [false, true] {
                let mut doc = DocHighlight::default();
                doc.update(&text, 0, code_mode);
                assert_eq!(
                    doc_spans(&doc, &text),
                    scan(&text, code_mode),
                    "round {round} initial (code_mode {code_mode}): {text:?}"
                );
                for edit in 0..20 {
                    let hint = random_edit(&mut rng, &mut text);
                    doc.update(&text, hint, code_mode);
                    assert_eq!(
                        doc_spans(&doc, &text),
                        scan(&text, code_mode),
                        "round {round} edit {edit} (code_mode {code_mode}): {text:?}"
                    );
                }
            }
        }
    }

    /// A maintained cache rebuilt from scratch must equal one kept through
    /// edits.
    #[test]
    fn maintained_matches_fresh_rebuild() {
        let mut rng = Rng(0xba5e_0000_0000_0042);
        let mut text = random_doc(&mut rng, 40);
        let mut maintained = DocHighlight::default();
        maintained.update(&text, 0, false);
        for _ in 0..30 {
            let hint = random_edit(&mut rng, &mut text);
            maintained.update(&text, hint, false);
        }
        let mut fresh = DocHighlight::default();
        fresh.update(&text, 0, false);
        assert_eq!(doc_spans(&maintained, &text), doc_spans(&fresh, &text));
    }


    #[test]
    fn code_mode_toggle_retags_everything() {
        let text = "```rust\nlet s = \"hi\";\n```";
        let mut doc = DocHighlight::default();
        doc.update(&text, 0, false);
        let plain = doc_spans(&doc, &text);
        assert!(!plain.iter().any(|s| s.kind == SpanKind::CodeStr));
        assert!(!plain.iter().any(|s| s.kind == SpanKind::CodeKeyword));
        let changed = doc.update(&text, 5, true);
        assert_eq!(changed, vec![(0, 2)]);
        let with_code = doc_spans(&doc, &text);
        assert!(with_code.iter().any(|s| s.kind == SpanKind::CodeStr));
        assert!(with_code.iter().any(|s| s.kind == SpanKind::CodeKeyword));
    }

    /// The point of the incremental pass: one character edited in a 2 MB
    /// document must not rescan it. The budget is generous (real runs land
    /// near a few ms; a full rescan takes seconds) — it catches the 100x
    /// regression.
    #[test]
    fn single_edit_on_two_megabytes_stays_local() {
        let para = "Lorem ipsum dolor sit amet, consectetur adipiscing elit. \
                    Sed do eiusmod tempor incididunt ut labore.\n\n";
        let mut text = String::with_capacity(2_200_000);
        while text.len() < 2_000_000 {
            text.push_str(para);
        }
        let mut doc = DocHighlight::default();
        doc.update(&text, 0, false);
        // Type one character into the middle of the document.
        let pos = text.len() / 2;
        let hint = text[..pos].matches('\n').count();
        text.insert(pos, 'x');
        let start = Instant::now();
        doc.update(&text, hint, false);
        let elapsed = start.elapsed();
        assert!(
            elapsed.as_millis() < 250,
            "incremental highlight took {} ms",
            elapsed.as_millis()
        );
    }
}
