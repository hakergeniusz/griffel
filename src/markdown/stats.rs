//! Document statistics (`03-behavior/05-statistics.md`): counts are taken
//! from the text with Markdown markup stripped.

/// One of the five counted statistics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Statistic {
    Characters,
    Words,
    Sentences,
    Paragraphs,
    ReadTime,
}

/// All counts at once.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub characters: u64,
    pub words: u64,
    pub sentences: u64,
    pub paragraphs: u64,
}

impl Statistic {
    /// Parse the settings identifier (`read_time`, …).
    pub fn parse_name(s: &str) -> Option<Statistic> {
        match s {
            "characters" => Some(Statistic::Characters),
            "words" => Some(Statistic::Words),
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
            Statistic::Sentences => "sentences",
            Statistic::Paragraphs => "paragraphs",
            Statistic::ReadTime => "read_time",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Statistic::Characters => "Characters",
            Statistic::Words => "Words",
            Statistic::Sentences => "Sentences",
            Statistic::Paragraphs => "Paragraphs",
            Statistic::ReadTime => "Read Time",
        }
    }

    /// The bare value: a number or an `H:MM:SS` duration.
    pub fn value(&self, st: &Stats) -> String {
        match self {
            Statistic::Characters => group_thousands(st.characters),
            Statistic::Words => group_thousands(st.words),
            Statistic::Sentences => group_thousands(st.sentences),
            Statistic::Paragraphs => group_thousands(st.paragraphs),
            Statistic::ReadTime => read_time(st.words),
        }
    }

    /// The full button label, e.g. "512 Words".
    pub fn label(&self, st: &Stats) -> String {
        format!("{} {}", self.value(st), self.display_name())
    }
}

/// Compute all statistics for `text` (markup stripped first).
pub fn compute(text: &str) -> Stats {
    let stripped = strip_markup(text);

    Stats {
        characters: count_characters(&stripped),
        words: count_words(&stripped),
        sentences: count_sentences(&stripped),
        paragraphs: count_paragraphs(&stripped),
    }
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
// Markup stripping
// ---------------------------------------------------------------------------

/// Remove Markdown syntax so counting sees human-readable text only.
pub fn strip_markup(text: &str) -> String {
    let mut out_lines: Vec<String> = Vec::new();
    let mut in_fence = false;

    for line in text.split('\n') {
        let trimmed = line.trim_start();

        // Fenced code blocks: fence lines are dropped, content kept.
        if let Some(rest) = trimmed.strip_prefix("```").or_else(|| trimmed.strip_prefix("~~~")) {
            let _ = rest;
            if !in_fence {
                in_fence = true;
                continue;
            } else {
                in_fence = false;
                continue;
            }
        }
        if in_fence {
            out_lines.push(line.to_owned());
            continue;
        }

        // Horizontal rules are removed entirely.
        if is_rule_line(trimmed) {
            continue;
        }

        let mut s = line.to_owned();

        // List markers at line start.
        s = strip_list_marker(&s);
        // Block quote markers.
        s = strip_quote_markers(&s);
        // Heading markers.
        s = strip_heading_marker(&s);
        // Table delimiters: drop separator rows, blank the pipes.
        if s.contains('|') {
            let t = s.trim();
            if !t.is_empty() && t.chars().all(|c| c == '-' || c == '|' || c == ':' || c == ' ') {
                continue;
            }
            s = s.replace('|', " ");
        }

        // Inline constructs.
        s = strip_inline(&s);

        out_lines.push(s);
    }
    out_lines.join("\n")
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

fn strip_list_marker(line: &str) -> String {
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
        return format!("{}{}", &line[..indent_len], after);
    }
    // Numbered.
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 {
        let after = &rest[digits..];
        if let Some(a2) = after.strip_prefix(". ").or_else(|| after.strip_prefix(") ")) {
            return format!("{}{}", &line[..indent_len], a2);
        }
    }
    line.to_owned()
}

fn strip_quote_markers(line: &str) -> String {
    let mut s = line;
    while let Some(rest) = s.strip_prefix("> ") {
        s = rest;
    }
    s.strip_prefix('>').unwrap_or(s).to_owned()
}

fn strip_heading_marker(line: &str) -> String {
    let hashes = line.chars().take_while(|c| *c == '#').count();
    if (1..=6).contains(&hashes) && line[hashes..].starts_with(' ') {
        line[hashes + 1..].to_owned()
    } else {
        line.to_owned()
    }
}

fn strip_inline(text: &str) -> String {
    use regex::Regex;
    use std::sync::OnceLock;
    // Compiled once: strip_markup runs per line over whole documents.
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
    let mut s = text.to_owned();
    for (re, repl) in patterns.iter() {
        s = re.replace_all(&s, *repl).into_owned();
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
    /// counting runs off the main thread (`spawn_blocking`), but the pure
    /// work itself stays well under a second (implementer-prompt
    /// responsiveness requirement).
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
        assert!(
            elapsed.as_millis() < 1_000,
            "counting 2 MB took {} ms",
            elapsed.as_millis()
        );
    }
}
