//! Document statistics (`03-behavior/05-statistics.md`): counts are taken
//! from the text with Markdown markup stripped.

use crate::editor::line_cache::LineProcessor;

/// One of the counted statistics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Statistic {
    Characters,
    Words,
    Tokens,
    Sentences,
    Paragraphs,
    ReadTime,
}

/// All counts at once.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub characters: u64,
    pub words: u64,
    pub tokens: u64,
    pub sentences: u64,
    pub paragraphs: u64,
}

impl Statistic {
    /// Parse the settings identifier (`read_time`, …).
    pub fn parse_name(s: &str) -> Option<Statistic> {
        match s {
            "characters" => Some(Statistic::Characters),
            "words" => Some(Statistic::Words),
            "tokens" => Some(Statistic::Tokens),
            "sentences" => Some(Statistic::Sentences),
            "paragraphs" => Some(Statistic::Paragraphs),
            "read_time" => Some(Statistic::ReadTime),
            _ => None,
        }
    }

    pub fn as_name(&self) -> &'static str {
        match self {
            Statistic::Characters => "characters",
            Statistic::Words => "words",
            Statistic::Tokens => "tokens",
            Statistic::Sentences => "sentences",
            Statistic::Paragraphs => "paragraphs",
            Statistic::ReadTime => "read_time",
        }
    }

    pub fn display_name(&self) -> String {
        match self {
            Statistic::Characters => crate::i18n::i18n("Characters"),
            Statistic::Words => crate::i18n::i18n("Words"),
            Statistic::Tokens => crate::i18n::i18n("Tokens"),
            Statistic::Sentences => crate::i18n::i18n("Sentences"),
            Statistic::Paragraphs => crate::i18n::i18n("Paragraphs"),
            Statistic::ReadTime => crate::i18n::i18n("Read Time"),
        }
    }

    /// The bare value: a number or an `H:MM:SS` duration.
    pub fn value(&self, st: &Stats) -> String {
        match self {
            Statistic::Characters => group_thousands(st.characters),
            Statistic::Words => group_thousands(st.words),
            Statistic::Tokens => group_thousands(st.tokens),
            Statistic::Sentences => group_thousands(st.sentences),
            Statistic::Paragraphs => group_thousands(st.paragraphs),
            Statistic::ReadTime => read_time(st.words),
        }
    }

    /// The full button label, e.g. "512 Words" (translated, with plural
    /// forms and the locale-grouped value).
    pub fn label(&self, st: &Stats) -> String {
        let v = self.value(st);
        match self {
            Statistic::Characters => crate::i18n::i18n_nf(
                "{} character",
                "{} characters",
                st.characters,
                &v,
            ),
            Statistic::Words => {
                crate::i18n::i18n_nf("{} word", "{} words", st.words, &v)
            }
            Statistic::Tokens => {
                crate::i18n::i18n_nf("{} token", "{} tokens", st.tokens, &v)
            }
            Statistic::Sentences => crate::i18n::i18n_nf(
                "{} sentence",
                "{} sentences",
                st.sentences,
                &v,
            ),
            Statistic::Paragraphs => crate::i18n::i18n_nf(
                "{} paragraph",
                "{} paragraphs",
                st.paragraphs,
                &v,
            ),
            Statistic::ReadTime => crate::i18n::i18n_f("{} reading time", &[&v]),
        }
    }
}

/// Compute all statistics for `text` (markup stripped first).
///
/// Folds the same per-line core as the incremental `StatsDoc`, so no
/// stripped copy of the whole document is ever materialized — a full
/// `strip_markup` + `join` costs several full-buffer allocations on a
/// multi-megabyte file. Every counter is line-decomposable, so the result
/// is bit-for-bit identical to counting the joined stripped text.
pub fn compute(text: &str) -> Stats {
    let mut proc = StatsProc;
    let mut state = StatsState::default();
    let mut totals = Stats::default();
    for line in text.split('\n') {
        let (value, next) = proc.process(line, &state, false);
        state = next;
        if let Some(ls) = value {
            totals.characters += ls.chars;
            totals.words += ls.words;
            totals.tokens += ls.tokens;
            totals.sentences += ls.sentences;
            totals.paragraphs += u64::from(ls.para_start);
        }
    }
    totals
}

/// Characters: non-whitespace characters; each run of whitespace within a
/// line counts as one; newlines are not counted.
pub fn count_characters(text: &str) -> u64 {
    let mut count = 0u64;
    for line in text.split('\n') {
        let mut in_ws = false;
        for c in line.chars() {
            if c.is_whitespace() {
                if !in_ws {
                    count += 1;
                    in_ws = true;
                }
            } else {
                count += 1;
                in_ws = false;
            }
        }
    }
    count
}

/// Words: whitespace-delimited tokens; each CJK character counts as one
/// word.
pub fn count_words(text: &str) -> u64 {
    let mut count = 0u64;
    for token in text.split_whitespace() {
        let cjk = token.chars().filter(|c| is_cjk(*c)).count();
        let non_cjk = token.chars().filter(|c| !is_cjk(*c)).any(|c| !c.is_whitespace());
        if cjk > 0 {
            count += cjk as u64;
            if non_cjk {
                count += 1;
            }
        } else {
            count += 1;
        }
    }
    count
}

/// Tokens: a deterministic estimate of the subword pieces an LLM tokenizer
/// (e.g. BPE) would emit. No vocabulary is bundled, so this is a heuristic,
/// not an exact count for any particular model:
///
/// - whitespace is ignored (tokenizers fold it into the following piece);
/// - each CJK character is one token;
/// - a run of letters/digits is one token, plus one extra for roughly every
///   six characters beyond the first (long, rare words split into subwords);
/// - every other character (punctuation, symbols, emoji) is one token.
///
/// For typical English this lands near the usual "¾ of a word per token"
/// rule of thumb. It is a single allocation-free pass and line-decomposable,
/// so it works with the incremental line cache like the other counters.
pub fn count_tokens(text: &str) -> u64 {
    // Number of characters per extra subword piece for a long run.
    const PIECE: usize = 6;
    let mut count = 0u64;
    let mut run = 0usize;
    for c in text.chars() {
        if c.is_alphanumeric() && !is_cjk(c) {
            run += 1;
            continue;
        }
        if run > 0 {
            count += (1 + (run - 1) / PIECE) as u64;
            run = 0;
        }
        if c.is_whitespace() {
            continue;
        }
        // CJK characters and every punctuation/symbol/emoji character.
        count += 1;
    }
    if run > 0 {
        count += (1 + (run - 1) / PIECE) as u64;
    }
    count
}

fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF
        | 0x3040..=0x309F | 0x30A0..=0x30FF      // kana
        | 0xAC00..=0xD7AF | 0x1100..=0x11FF      // hangul
        | 0x20000..=0x2A6DF | 0x2A700..=0x2B81F)
}

/// Sentences: text segments ending in sentence-terminal punctuation
/// (including fullwidth variants), where line breaks terminate a potential
/// sentence.
pub fn count_sentences(text: &str) -> u64 {
    const TERMINAL: [char; 8] = ['.', '?', '!', '。', '？', '！', '…', '．'];
    let mut count = 0u64;
    let mut pending = false;
    for line in text.split('\n') {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        for c in trimmed.chars() {
            if TERMINAL.contains(&c) {
                count += 1;
                pending = false;
            } else if !c.is_whitespace() {
                pending = true;
            }
        }
        // A non-empty line not ending in terminal punctuation still holds
        // a sentence up to the line break.
        if pending {
            count += 1;
            pending = false;
        }
    }
    count
}

/// Paragraphs: blocks separated by one or more blank lines.
pub fn count_paragraphs(text: &str) -> u64 {
    let mut count = 0u64;
    let mut in_para = false;
    for line in text.split('\n') {
        if line.trim().is_empty() {
            in_para = false;
        } else if !in_para {
            count += 1;
            in_para = true;
        }
    }
    count
}

/// Read time as `H:MM:SS` from a word count at 200 words per minute.
pub fn read_time(words: u64) -> String {
    let seconds = words * 60 / 200;
    let h = seconds / 3600;
    let m = (seconds % 3600) / 60;
    let s = seconds % 60;
    format!("{h}:{m:02}:{s:02}")
}

// ---------------------------------------------------------------------------
// Incremental statistics (`05-statistics.md`: counting coalesces bursts and
// must stay fast on multi-megabyte documents)
// ---------------------------------------------------------------------------

/// Per-line counts. All statistics are line-decomposable over the stripped
/// text (words never span a newline; sentences flush at line end; a
/// paragraph starts at the first non-blank line after a blank one), so a
/// cache of these sums to exactly `compute()`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LineStats {
    pub chars: u64,
    pub words: u64,
    pub tokens: u64,
    pub sentences: u64,
    /// This line starts a new paragraph.
    pub para_start: bool,
    pub blank: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct StatsState {
    in_fence: bool,
    prev_blank: bool,
}

impl Default for StatsState {
    fn default() -> Self {
        // The document begins outside a fence and at a paragraph boundary.
        Self {
            in_fence: false,
            prev_blank: true,
        }
    }
}

struct StatsProc;
impl crate::editor::line_cache::LineProcessor for StatsProc {
    type State = StatsState;
    type Value = Option<LineStats>;

    fn process(
        &mut self,
        line: &str,
        in_state: &StatsState,
        _is_last: bool,
    ) -> (Option<LineStats>, StatsState) {
        let mut fence = in_state.in_fence;
        let Some(stripped) = strip_line(line, &mut fence) else {
            // Dropped lines are invisible to every counter and leave the
            // paragraph state untouched.
            return (
                None,
                StatsState {
                    in_fence: fence,
                    prev_blank: in_state.prev_blank,
                },
            );
        };
        let blank = stripped.trim().is_empty();
        let value = LineStats {
            chars: count_characters(&stripped),
            words: count_words(&stripped),
            tokens: count_tokens(&stripped),
            sentences: count_sentences(&stripped),
            para_start: !blank && in_state.prev_blank,
            blank,
        };
        (
            Some(value),
            StatsState {
                in_fence: fence,
                prev_blank: blank,
            },
        )
    }
}

/// Incrementally maintained statistics for one document. Totals stay
/// bit-for-bit identical to `compute(text)` while an edit recomputes only
/// the lines it touches (the rest is re-summed, pure arithmetic).
#[derive(Clone, Default)]
pub struct StatsDoc {
    cache: crate::editor::line_cache::LineCache<StatsProc>,
    totals: Stats,
}

impl StatsDoc {
    /// Sync with `text`; `dirty_from` is the conservative first-affected
    /// line hint (0 = rebuild). Returns true when totals were recomputed.
    pub fn update(&mut self, text: &str, dirty_from: usize) -> bool {
        let old_count = self.cache.line_count();
        let mut proc = StatsProc;
        let changed = self.cache.update(text, dirty_from, &mut proc);
        // Deleted lines vanish from the cache without any line being
        // recomputed, so the line count changing also forces a re-sum.
        if changed.is_empty() && self.cache.line_count() == old_count {
            return false;
        }
        self.retotal();
        true
    }

    fn retotal(&mut self) {
        let mut totals = Stats::default();
        for e in self.cache.entries() {
            if let Some(ls) = &e.value {
                totals.characters += ls.chars;
                totals.words += ls.words;
                totals.tokens += ls.tokens;
                totals.sentences += ls.sentences;
                totals.paragraphs += u64::from(ls.para_start);
            }
        }
        self.totals = totals;
    }

    /// The current totals, valid after the last `update`.
    pub fn totals(&self) -> Stats {
        self.totals
    }
}

// ---------------------------------------------------------------------------
// Markup stripping
// ---------------------------------------------------------------------------

/// Remove Markdown syntax so counting sees human-readable text only.
pub fn strip_markup(text: &str) -> String {
    let mut in_fence = false;
    let mut out = String::with_capacity(text.len());
    let mut first = true;
    for line in text.split('\n') {
        if let Some(stripped) = strip_line(line, &mut in_fence) {
            if !first {
                out.push('\n');
            }
            first = false;
            out.push_str(&stripped);
        }
    }
    out
}

/// Strip one line (`in_fence` carries fenced-code state across lines).
/// Returns None when the line is dropped entirely (fence delimiters,
/// horizontal rules, table separator rows); Some(stripped) otherwise.
/// The per-line core of `strip_markup` — shared with the incremental
/// `StatsDoc` so both paths strip identically.
fn strip_line<'a>(
    line: &'a str,
    in_fence: &mut bool,
) -> Option<std::borrow::Cow<'a, str>> {
    use std::borrow::Cow;
    let trimmed = line.trim_start();

    // Fenced code blocks: fence lines are dropped, content kept.
    if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
        *in_fence = !*in_fence;
        return None;
    }
    if *in_fence {
        return Some(Cow::Borrowed(line));
    }

    // Horizontal rules are removed entirely.
    if is_rule_line(trimmed) {
        return None;
    }

    // A plain prose line needs no allocation at all: every transform below
    // returns owned data only when it actually changed something.
    let mut s: Cow<'a, str> = Cow::Borrowed(line);
    if let Some(v) = strip_list_marker(s.as_ref()) {
        s = Cow::Owned(v);
    }
    if let Some(v) = strip_quote_markers(s.as_ref()) {
        s = Cow::Owned(v);
    }
    if let Some(v) = strip_heading_marker(s.as_ref()) {
        s = Cow::Owned(v);
    }
    // Table delimiters: drop separator rows, blank the pipes.
    if s.contains('|') {
        let t = s.trim();
        if !t.is_empty() && t.chars().all(|c| c == '-' || c == '|' || c == ':' || c == ' ') {
            return None;
        }
        s = Cow::Owned(s.replace('|', " "));
    }

    // Inline constructs.
    let inline = strip_inline(s.as_ref());
    if let Cow::Owned(v) = inline {
        return Some(Cow::Owned(v));
    }
    Some(s)
}

fn is_rule_line(line: &str) -> bool {
    let t = line.trim();
    let mut chars = t.chars();
    let Some(c) = chars.next() else {
        return false;
    };
    if c != '-' && c != '*' && c != '_' {
        return false;
    }
    t.chars().filter(|x| *x == c).count() >= 3
        && t.chars().all(|x| x == c || x.is_whitespace())
}

/// `Some` only when a list marker was actually removed.
fn strip_list_marker(line: &str) -> Option<String> {
    let indent_len = line.len() - line.trim_start_matches([' ', '\t']).len();
    let rest = &line[indent_len..];
    // Bullet or checklist.
    if let Some(after) = rest.strip_prefix("- ")
        .or_else(|| rest.strip_prefix("* "))
        .or_else(|| rest.strip_prefix("+ "))
    {
        // Checklist box.
        let after = after
            .strip_prefix("[ ] ")
            .or_else(|| after.strip_prefix("[x] "))
            .or_else(|| after.strip_prefix("[X] "))
            .unwrap_or(after);
        return Some(format!("{}{}", &line[..indent_len], after));
    }
    // Numbered.
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 {
        let after = &rest[digits..];
        if let Some(a2) = after.strip_prefix(". ").or_else(|| after.strip_prefix(") ")) {
            return Some(format!("{}{}", &line[..indent_len], a2));
        }
    }
    None
}

/// `Some` only when a block-quote marker was actually removed.
fn strip_quote_markers(line: &str) -> Option<String> {
    let mut s = line;
    while let Some(rest) = s.strip_prefix("> ") {
        s = rest;
    }
    let stripped = s.strip_prefix('>').unwrap_or(s);
    (stripped != line).then(|| stripped.to_owned())
}

/// `Some` only when a heading marker was actually removed.
fn strip_heading_marker(line: &str) -> Option<String> {
    let hashes = line.chars().take_while(|c| *c == '#').count();
    if (1..=6).contains(&hashes) && line[hashes..].starts_with(' ') {
        Some(line[hashes + 1..].to_owned())
    } else {
        None
    }
}

fn strip_inline(text: &str) -> std::borrow::Cow<'_, str> {
    use regex::Regex;
    use std::sync::OnceLock;
    // Compiled once: strip_markup runs per line over whole documents.
    // Plain prose has none of the characters any inline pattern can match.
    // Most lines in a large document are plain, so one byte scan beats
    // running eleven regexes over every line.
    if !text.bytes().any(|b| {
        matches!(
            b,
            b'`' | b'$' | b'!' | b'[' | b'<' | b'*' | b'~' | b'_'
        )
    }) {
        return std::borrow::Cow::Borrowed(text);
    }
    static PATTERNS: OnceLock<Vec<(Regex, &'static str)>> = OnceLock::new();
    let patterns = PATTERNS.get_or_init(|| {
        vec![
            // Images: keep alt text.
            (Regex::new(r"!\[([^\]\n]*)\]\([^)\n]*\)").unwrap(), "$1"),
            // Links: keep label.
            (Regex::new(r"\[([^\]\n]*)\]\([^)\n]*\)").unwrap(), "$1"),
            // Footnote references: removed.
            (Regex::new(r"\[\^[^\]\n]+\]").unwrap(), ""),
            // Footnote definitions: keep text.
            (Regex::new(r"\[\^[^\]\n]+\]:\s*").unwrap(), ""),
            // Math delimiters: keep content.
            (Regex::new(r"\$\$([^$\n]+)\$\$").unwrap(), "$1"),
            // Emphasis delimiters: removed.
            (Regex::new(r"\*\*\*|\*\*|__|~~|\*|_|`").unwrap(), ""),
        ]
    });
    // `replace_all` returns a borrowed Cow when the pattern did not match —
    // the overwhelmingly common case for prose lines. Only allocate when a
    // pattern actually changed the line; six blind `into_owned()` calls per
    // line made full-document stripping allocate several megabytes.
    let mut s = std::borrow::Cow::Borrowed(text);
    for (re, repl) in patterns.iter() {
        match re.replace_all(&s, *repl) {
            std::borrow::Cow::Borrowed(_) => {}
            std::borrow::Cow::Owned(changed) => s = std::borrow::Cow::Owned(changed),
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn characters_definition() {
        // Non-whitespace chars + one per whitespace run; newlines ignored.
        assert_eq!(count_characters("hello world"), 11);
        // a + ws-run + b + ws-run + c.
        assert_eq!(count_characters("a  b\t\tc"), 5);
        assert_eq!(count_characters("a\nb"), 2);
        assert_eq!(count_characters(""), 0);
    }

    #[test]
    fn grouping_never_shrinks_and_is_ascii_safe() {
        // C locale has no separator; any locale must keep every digit.
        let g = group_thousands(1_234_567);
        let digits: String = g.chars().filter(|c| c.is_ascii_digit()).collect();
        assert_eq!(digits, "1234567");
        assert_eq!(group_thousands(42), "42");
    }

    #[test]
    fn words_basic() {
        assert_eq!(count_words("hello world foo"), 3);
        assert_eq!(count_words("  spaced   out  "), 2);
        assert_eq!(count_words(""), 0);
    }

    #[test]
    fn words_cjk() {
        assert_eq!(count_words("你好世界"), 4);
        assert_eq!(count_words("你好 world"), 3);
    }

    #[test]
    fn tokens_basic() {
        // A short word is one token; whitespace is folded away.
        assert_eq!(count_tokens("hello world"), 2);
        // Longer runs split into extra subword pieces.
        assert_eq!(count_tokens("abcdefgh"), 2);
        assert_eq!(count_tokens("internationalization"), 4);
        assert_eq!(count_tokens(""), 0);
        assert_eq!(count_tokens("   \t\n "), 0);
    }

    #[test]
    fn tokens_punctuation_cjk_and_markup() {
        // Each punctuation mark is its own token.
        assert_eq!(count_tokens("Hello, world!"), 4);
        // Each CJK character is one token.
        assert_eq!(count_tokens("你好世界"), 4);
        // Emoji/symbols are single tokens too.
        assert_eq!(count_tokens("a🙂b"), 3);
        // Markup is stripped before counting.
        assert_eq!(compute("**bold** `code`").tokens, count_tokens("bold code"));
    }

    #[test]
    fn sentences() {
        assert_eq!(count_sentences("One. Two? Three!"), 3);
        assert_eq!(count_sentences("One line without end"), 1);
        assert_eq!(count_sentences("First.\nSecond line"), 2);
        assert_eq!(count_sentences("中文。句子！"), 2);
        assert_eq!(count_sentences(""), 0);
    }

    #[test]
    fn paragraphs() {
        assert_eq!(count_paragraphs("a\n\nb\nc\n\nd"), 3);
        assert_eq!(count_paragraphs(""), 0);
        assert_eq!(count_paragraphs("single"), 1);
    }

    #[test]
    fn read_time_format() {
        // 200 words = 1 minute.
        assert_eq!(read_time(200), "0:01:00");
        assert_eq!(read_time(100), "0:00:30");
        assert_eq!(read_time(12000), "1:00:00");
        assert_eq!(read_time(0), "0:00:00");
    }

    #[test]
    fn strip_emphasis_and_links() {
        assert_eq!(strip_markup("**bold** and _i_"), "bold and i");
        assert_eq!(strip_markup("see [the label](https://x.example)"), "see the label");
        assert_eq!(strip_markup("![alt text](img.png)"), "alt text");
        assert_eq!(strip_markup("~~gone~~ `code here`"), "gone code here");
    }

    #[test]
    fn strip_block_constructs() {
        let text = "# Title\n\n- one\n- two\n\n> quoted\n\n---\n\nbody";
        let stripped = strip_markup(text);
        assert!(!stripped.contains('#'));
        assert!(!stripped.contains("- one"));
        assert!(stripped.contains("one"));
        assert!(!stripped.contains('>'));
        // Horizontal rule removed entirely.
        assert!(!stripped.lines().any(|l| l.trim() == "---"));
        assert!(stripped.contains("body"));
    }

    #[test]
    fn fence_content_counted_fences_not() {
        let text = "```\ncode line\n```\ntext";
        let stripped = strip_markup(text);
        assert!(stripped.contains("code line"));
        assert!(!stripped.contains("```"));
    }

    #[test]
    fn tables_and_footnotes() {
        let text = "| a | b |\n|---|---|\n| 1 | 2 |\nsee[^1]\n\n[^1]: note";
        let stripped = strip_markup(text);
        assert!(!stripped.contains('|'));
        assert!(!stripped.contains("[^1]"));
        assert!(stripped.contains("note"));
        assert!(stripped.contains("1"));
    }

    #[test]
    fn stats_end_to_end() {
        let text = "# Hello\n\nThis is **bold** text. Second sentence!\n\n- item one\n- item two\n";
        let stats = compute(text);
        // Words: "Hello" + 5 (This is bold text Second? count: This is bold text = 4) ...
        assert_eq!(stats.paragraphs, 3); // "Hello", paragraph, "item one item two"
        assert!(stats.words >= 10);
        assert!(stats.sentences >= 2);
        assert!(stats.tokens >= stats.words);
    }

    /// The allocation-free fold in `compute()` must agree with counting the
    /// materialized stripped text (the reference semantics).
    #[test]
    fn compute_matches_stripped_join() {
        let text = "# Hello\n\n**bold** text. [link](https://x.example) 你好\n\n```\ncode here\n```\n\n---\nlast";
        let stripped = strip_markup(text);
        let expected = Stats {
            characters: count_characters(&stripped),
            words: count_words(&stripped),
            tokens: count_tokens(&stripped),
            sentences: count_sentences(&stripped),
            paragraphs: count_paragraphs(&stripped),
        };
        assert_eq!(compute(text), expected);
    }
}

/// Format `n` with the active locale's digit grouping separator
/// (`03-behavior/14-presentation-details.md` §4); read time and other
/// non-grouped numbers bypass this.
pub fn group_thousands(n: u64) -> String {
    let sep = unsafe {
        let lc = libc::localeconv();
        if lc.is_null() || (*lc).thousands_sep.is_null() {
            String::new()
        } else {
            std::ffi::CStr::from_ptr((*lc).thousands_sep)
                .to_string_lossy()
                .into_owned()
        }
    };
    let digits = n.to_string();
    let bytes = digits.as_bytes();
    if sep.is_empty() || bytes.len() <= 3 {
        return digits;
    }
    let mut out = String::with_capacity(digits.len() + sep.len() * (digits.len() / 3));
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && (bytes.len() - i) % 3 == 0 {
            out.push_str(&sep);
        }
        out.push(*b as char);
    }
    out
}

#[cfg(test)]
mod perf {
    use super::*;
    use std::time::Instant;

    /// A ~2 MB document must not be slow enough to ever block the UI:
    /// counting runs off the main thread (`spawn_blocking`). The budget is
    /// generous (real runs land near 1 s, and CI machines jitter) — its job
    /// is to catch the 100x regression, not measure the exact time.
    #[test]
    fn two_megabyte_document_counts_quickly() {
        let para = "Lorem ipsum dolor sit amet, consectetur adipiscing elit. \
                    Sed do eiusmod tempor incididunt ut labore.\n\n";
        let mut text = String::with_capacity(2_200_000);
        while text.len() < 2_000_000 {
            text.push_str(para);
        }
        let start = Instant::now();
        let stats = compute(&text);
        let elapsed = start.elapsed();
        assert!(stats.words > 100_000);
        assert!(stats.tokens > 100_000);
        assert!(
            elapsed.as_millis() < 2_500,
            "counting 2 MB took {} ms",
            elapsed.as_millis()
        );
    }

    /// The whole point of the incremental path: after a single-character
    /// edit to a 2 MB document, syncing totals must not re-scan the file.
    /// Budget is generous (real runs land near 5 ms) — it catches the 100x
    /// regression (a full recompute would take ~1 s).
    #[test]
    fn incremental_update_after_a_tiny_edit_is_fast() {
        let para = "Lorem ipsum dolor sit amet, consectetur adipiscing elit. \
                    Sed do eiusmod tempor incididunt ut labore.\n\n";
        let mut text = String::with_capacity(2_200_000);
        while text.len() < 2_000_000 {
            text.push_str(para);
        }
        let mut doc = StatsDoc::default();
        doc.update(&text, 0);
        // Type one character into the first line.
        text.insert_str(5, "x");
        let start = Instant::now();
        let changed = doc.update(&text, 0);
        let elapsed = start.elapsed();
        assert!(changed);
        assert_eq!(doc.totals(), compute(&text));
        assert!(
            elapsed.as_millis() < 250,
            "incremental update took {} ms",
            elapsed.as_millis()
        );
    }
}

#[cfg(test)]
mod incremental {
    use super::*;

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

    const WORDS: &[&str] = &[
        "lorem", "ipsum", "dolor", "sit", "amet", "你好", "世界", "em—dash",
        "a", "`code`", "**bold**", "[link](https://x.example)", "> quote", "# Heading",
        "- item", "1. one", "| a | b |", "|---|---|", "---", "…", "x!", "end.",
    ];

    fn random_doc(rng: &mut Rng, lines: usize) -> String {
        let mut out = String::new();
        for i in 0..lines {
            if i > 0 {
                out.push('\n');
            }
            match rng.below(10) {
                0 => out.push_str("```rust\nlet x = 1;\n```"),
                1 => out.push_str("---\ntitle: t\n---"),
                _ => {
                    for _ in 0..(1 + rng.below(6)) {
                        out.push_str(WORDS[rng.below(WORDS.len())]);
                        out.push(' ');
                    }
                }
            }
            if out.ends_with(' ') {
                out.pop();
            }
        }
        out
    }

    /// Perform a random edit, return the first-affected line hint (the
    /// editor wiring provides the same guarantee: the hint never exceeds
    /// the true first affected line).
    fn random_edit(rng: &mut Rng, text: &mut String) -> usize {
        let pos = rng.below(text.len() + 1);
        // Snap to a char boundary.
        let mut pos = pos;
        while !text.is_char_boundary(pos) {
            pos += 1;
        }
        let hint = text[..pos].matches('\n').count();
        match rng.below(3) {
            0 => {
                // Insert a word fragment.
                let w = WORDS[rng.below(WORDS.len())];
                text.insert_str(pos, w);
            }
            1 => {
                // Delete up to 20 chars.
                let mut end = (pos + rng.below(21)).min(text.len());
                while !text.is_char_boundary(end) {
                    end += 1;
                }
                text.replace_range(pos..end, "");
            }
            _ => {
                // Insert newlines (structural edit).
                let nl = "\n".repeat(1 + rng.below(3));
                text.insert_str(pos, &nl);
            }
        }
        hint
    }

    /// After every edit, the incrementally maintained totals must match a
    /// full `compute()` exactly — the spec pins the semantics, so the fast
    /// path may never diverge from the reference.
    #[test]
    fn incremental_totals_track_compute_through_random_edits() {
        let mut rng = Rng(0x5eed_1234_abcd_ef01);
        for round in 0..30 {
            let lines = 5 + rng.below(20);
            let mut text = random_doc(&mut rng, lines);
            let mut doc = StatsDoc::default();
            doc.update(&text, 0);
            assert_eq!(doc.totals(), compute(&text), "round {round} initial");
            for edit in 0..25 {
                let hint = random_edit(&mut rng, &mut text);
                doc.update(&text, hint);
                assert_eq!(
                    doc.totals(),
                    compute(&text),
                    "round {round} edit {edit}: {text:?}"
                );
            }
        }
    }

    /// An incremental doc rebuilt from scratch must equal one maintained
    /// through edits — no drift between the two paths.
    #[test]
    fn maintained_doc_matches_fresh_rebuild() {
        let mut rng = Rng(0xdead_beef_cafe_babe);
        let mut text = random_doc(&mut rng, 40);
        let mut maintained = StatsDoc::default();
        maintained.update(&text, 0);
        for _ in 0..40 {
            let hint = random_edit(&mut rng, &mut text);
            maintained.update(&text, hint);
        }
        let mut fresh = StatsDoc::default();
        fresh.update(&text, 0);
        assert_eq!(maintained.totals(), fresh.totals());
    }

    /// A maintained doc must also reproduce `strip_markup` line-by-line
    /// (the strip core is shared, but the fence carry is per-line state).
    #[test]
    fn strip_line_fold_matches_strip_markup() {
        let mut rng = Rng(0x0f1e_2d3c_4b5a_6978);
        for _ in 0..50 {
            let text = random_doc(&mut rng, 30);
            assert_eq!(strip_markup(&text), fold_strip(&text), "{text:?}");
        }
    }

    fn fold_strip(text: &str) -> String {
        let mut fence = false;
        text.split('\n')
            .filter_map(|l| strip_line(l, &mut fence))
            .collect::<Vec<_>>()
            .join("\n")
    }
}
