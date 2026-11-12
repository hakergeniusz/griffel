//! In-place spell checking (`03-behavior/01-editing.md` §4): a background
//! scan marks misspelled words with the platform's error underline. Words
//! inside fenced code blocks, inline code spans and YAML front matter are
//! not checked. Focus Mode temporarily disables the whole feature (the
//! window toggles `enabled`).

use gtk::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::spell::SpellChecker;

pub const TAG: &str = "misspelled";

/// Create the misspelling tag on `buffer` if missing.
pub fn ensure_tag(buffer: &sourceview5::Buffer) {
    let table = buffer.tag_table();
    if table.lookup(TAG).is_none() {
        let tag = gtk::TextTag::new(Some(TAG));
        tag.set_underline(pango::Underline::Error);
        table.add(&tag);
    }
}

/// Remove all error underlines.
pub fn clear(buffer: &sourceview5::Buffer) {
    buffer.remove_tag_by_name(TAG, &buffer.start_iter(), &buffer.end_iter());
}

#[derive(Clone)]
pub struct SpellRunner {
    generation: Arc<AtomicU64>,
    checker: Option<SpellChecker>,
}

impl Default for SpellRunner {
    fn default() -> Self {
        Self {
            generation: Arc::new(AtomicU64::new(0)),
            checker: SpellChecker::new(),
        }
    }
}

/// Incrementally maintained spell state for one document: per-line
/// misspelled ranges plus the cross-line fence/front-matter state, with a
/// per-word enchant result cache (repeated words in a multi-megabyte
/// document cost one lookup each, not one per occurrence).
pub struct SpellDoc {
    cache: crate::editor::line_cache::LineCache<SpellProc>,
    lang: Option<String>,
    words: Arc<std::sync::Mutex<std::collections::HashMap<Box<str>, bool>>>,
}

impl Clone for SpellDoc {
    /// Shallow except for the language; the word cache is shared.
    fn clone(&self) -> Self {
        Self {
            cache: self.cache.clone(),
            lang: self.lang.clone(),
            words: self.words.clone(),
        }
    }
}

impl SpellDoc {
    pub fn new(lang: Option<String>) -> Self {
        Self {
            cache: crate::editor::line_cache::LineCache::new(),
            lang,
            words: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        }
    }

    /// Sync with `text`; `dirty_from` is the conservative first-affected
    /// line hint (0 = rebuild). A language change rebuilds everything and
    /// drops the word cache. Returns the re-tagged line ranges.
    pub fn update(
        &mut self,
        text: &str,
        dirty_from: usize,
        checker: &SpellChecker,
        lang: Option<&str>,
    ) -> Vec<(usize, usize)> {
        if self.lang.as_deref() != lang {
            self.lang = lang.map(str::to_owned);
            {
                let mut words = self.words.lock().unwrap_or_else(|e| e.into_inner());
                words.clear();
            }
            return self.rebuild(text, checker, lang);
        }
        let mut proc = SpellProc {
            checker: checker.clone(),
            lang: lang.map(str::to_owned),
            words: self.words.clone(),
        };
        self.cache.update(text, dirty_from, &mut proc)
    }

    fn rebuild(
        &mut self,
        text: &str,
        checker: &SpellChecker,
        lang: Option<&str>,
    ) -> Vec<(usize, usize)> {
        let mut proc = SpellProc {
            checker: checker.clone(),
            lang: lang.map(str::to_owned),
            words: self.words.clone(),
        };
        let changed = self.cache.update(text, 0, &mut proc);
        if changed.is_empty() && self.cache.line_count() > 0 {
            return vec![(0, self.cache.line_count() - 1)];
        }
        changed
    }

    fn entry(
        &self,
        line: usize,
    ) -> Option<&std::sync::Arc<crate::editor::line_cache::Entry<SpellProc>>> {
        self.cache.entries().get(line)
    }
}

/// Per-line processing: misspelled ranges (line-local char offsets) plus
/// the fence/front-matter state — the incremental counterpart of `words`.
struct SpellProc {
    checker: SpellChecker,
    lang: Option<String>,
    words: Arc<std::sync::Mutex<std::collections::HashMap<Box<str>, bool>>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SpellState {
    in_fence: bool,
    fence_marker: char,
    in_front_matter: bool,
    at_start: bool,
}

impl Default for SpellState {
    fn default() -> Self {
        Self {
            in_fence: false,
            fence_marker: ' ',
            in_front_matter: false,
            at_start: true,
        }
    }
}

impl crate::editor::line_cache::LineProcessor for SpellProc {
    type State = SpellState;
    type Value = Vec<(usize, usize)>;

    fn process(
        &mut self,
        line: &str,
        st: &SpellState,
        is_last: bool,
    ) -> (Vec<(usize, usize)>, SpellState) {
        let mut out = *st;
        out.at_start = false;
        let t = line.trim_start();

        if st.in_front_matter {
            if t == "---" || t == "..." {
                out.in_front_matter = false;
            }
            return (Vec::new(), out);
        }
        // The opening delimiter must be the very first line and there must
        // be something after it (`words`: text.starts_with("---\n")).
        if st.at_start && t == "---" && !is_last {
            out.in_front_matter = true;
            return (Vec::new(), out);
        }

        // Fence toggles: a line of 3+ backticks/tildes opens (optionally
        // with an info string) or closes the block; the line itself is
        // never scanned.
        let marker = t.chars().next().unwrap_or(' ');
        let marker_count = t.chars().take_while(|&c| c == marker).count();
        if (marker == '`' || marker == '~') && marker_count >= 3 {
            let rest: String = t.chars().skip(marker_count).collect();
            if st.in_fence && marker == st.fence_marker && rest.trim().is_empty() {
                out.in_fence = false;
            } else if !st.in_fence {
                out.in_fence = true;
                out.fence_marker = marker;
            }
            return (Vec::new(), out);
        }

        if st.in_fence {
            return (Vec::new(), out);
        }

        let mut ranges = Vec::new();
        scan_line_checked(line, 0, &mut ranges, &self.checker, self.lang.as_deref(), &self.words);
        (ranges, out)
    }
}

/// `scan_line` plus the enchant lookup with the per-word cache.
fn scan_line_checked(
    content: &str,
    base: usize,
    out: &mut Vec<(usize, usize)>,
    checker: &SpellChecker,
    lang: Option<&str>,
    cache: &std::sync::Mutex<std::collections::HashMap<Box<str>, bool>>,
) {
    for (start, end, word) in {
        let mut words = Vec::new();
        scan_line(content, base, &mut words);
        words
    } {
        let misspelled = misspelled_cached(cache, checker, lang, &word);
        if misspelled {
            out.push((start, end));
        }
    }
}

fn misspelled_cached(
    cache: &std::sync::Mutex<std::collections::HashMap<Box<str>, bool>>,
    checker: &SpellChecker,
    lang: Option<&str>,
    word: &str,
) -> bool {
    let key: Box<str> = word.into();
    if let Ok(map) = cache.lock() {
        if let Some(&v) = map.get(&key) {
            return v;
        }
    }
    let v = checker.is_misspelled(lang, word);
    if let Ok(mut map) = cache.lock() {
        map.insert(key, v);
    }
    v
}

impl SpellRunner {
    /// Schedule an incremental scan; `cache` is the current main-thread
    /// document state, `dirty_from` the conservative first-affected line.
    /// Ranges are applied to `buffer` on the main loop only if the scan is
    /// still current (stale results dropped whole).
    pub fn schedule(
        &self,
        buffer: &sourceview5::Buffer,
        text: String,
        cache: Arc<SpellDoc>,
        dirty_from: usize,
        lang: Option<String>,
        enabled: bool,
        adopt: impl Fn(Arc<SpellDoc>) + 'static,
    ) {
        let generation_id = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        if !enabled || self.checker.is_none() {
            clear(buffer);
            adopt(Arc::new(SpellDoc::new(lang)));
            return;
        }
        let generation = self.generation.clone();
        let cb_buffer = buffer.clone();
        let checker = self.checker.clone().unwrap();
        glib::spawn_future_local(async move {
            let handle = gio::spawn_blocking(move || {
                let mut doc = (*cache).clone();
                let changed = doc.update(&text, dirty_from, &checker, lang.as_deref());
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

    /// Suggestions for the context menu (called on the main thread; a few
    /// enchant calls are fast enough to be interactive).
    pub fn suggestions(&self, lang: Option<&str>, word: &str) -> Vec<String> {
        self.checker
            .as_ref()
            .map(|c| c.suggestions(lang, word))
            .unwrap_or_default()
    }

    /// Available dictionary languages for the language submenu.
    pub fn languages(&self) -> Vec<String> {
        self.checker
            .as_ref()
            .map(|c| c.available_languages())
            .unwrap_or_default()
    }
}

/// Apply the scanned ranges.
pub fn apply(buffer: &sourceview5::Buffer, ranges: &[(usize, usize)]) {
    clear(buffer);
    for (start, end) in ranges {
        buffer.apply_tag_by_name(
            TAG,
            &buffer.iter_at_offset(*start as i32),
            &buffer.iter_at_offset(*end as i32),
        );
    }
}

/// Re-tag only the given line ranges (inclusive) from the document state;
/// untouched lines keep their underlines.
pub fn apply_ranges(buffer: &sourceview5::Buffer, doc: &SpellDoc, ranges: &[(usize, usize)]) {
    for (first, last) in ranges {
        for line in *first..=*last {
            let Some(entry) = doc.entry(line) else {
                continue;
            };
            let Some(mut start) = buffer.iter_at_line(line as i32) else {
                continue;
            };
            let mut end = start.clone();
            end.forward_to_line_end();
            buffer.remove_tag_by_name(TAG, &start, &end);
            for (w_start, w_end) in &entry.value {
                let Some(s) = buffer.iter_at_line_offset(line as i32, *w_start as i32) else {
                    continue;
                };
                let Some(e) = buffer.iter_at_line_offset(line as i32, *w_end as i32) else {
                    continue;
                };
                buffer.apply_tag_by_name(TAG, &s, &e);
            }
        }
    }
}

/// All misspelled word ranges in `text` (char offsets, end exclusive).
pub fn misspelled_ranges(
    text: &str,
    checker: &SpellChecker,
    lang: Option<&str>,
) -> Vec<(usize, usize)> {
    words(text)
        .into_iter()
        .filter(|(_, _, word)| checker.is_misspelled(lang, word))
        .map(|(s, e, _)| (s, e))
        .collect()
}

/// Split `text` into checkable words: runs of alphanumeric characters plus
/// internal apostrophes/hyphens, with non-alphabetic edges trimmed. Words
/// inside fenced code blocks, inline code spans and YAML front matter are
/// skipped, as are pure numbers, single characters and very long tokens.
/// Returns (char-offset start, char-offset end, word).
pub fn words(text: &str) -> Vec<(usize, usize, String)> {
    let mut out = Vec::new();
    let mut line_char_start = 0usize;
    let mut in_fence = false;
    let mut fence_marker = '`';
    let mut in_front_matter = text.starts_with("---\n");
    let mut fm_first_line = in_front_matter;

    for line in text.split_inclusive('\n') {
        let content = line.strip_suffix('\n').unwrap_or(line);
        let newline = if line.ends_with('\n') { 1 } else { 0 };
        let t = content.trim_start();

        if in_front_matter {
            // The opening delimiter line itself must not close the block.
            if fm_first_line {
                fm_first_line = false;
            } else if t == "---" || t == "..." {
                in_front_matter = false;
            }
            line_char_start += content.chars().count() + newline;
            continue;
        }

        // Fence toggles: a line of 3+ backticks/tildes opens (optionally
        // with an info string) or closes the block.
        let marker = t.chars().next().unwrap_or(' ');
        let marker_count = t.chars().take_while(|&c| c == marker).count();
        if (marker == '`' || marker == '~') && marker_count >= 3 {
            let rest = t.chars().skip(marker_count).collect::<String>();
            if in_fence && marker == fence_marker && rest.trim().is_empty() {
                in_fence = false;
            } else if !in_fence {
                in_fence = true;
                fence_marker = marker;
            }
            line_char_start += content.chars().count() + newline;
            continue;
        }

        if !in_fence {
            scan_line(content, line_char_start, &mut out);
        }
        line_char_start += content.chars().count() + newline;
    }
    out
}

fn scan_line(content: &str, base: usize, out: &mut Vec<(usize, usize, String)>) {
    let mut in_code = false;
    let mut word_start: Option<usize> = None;
    let mut word = String::new();
    for (i, c) in content.chars().enumerate() {
        if c == '`' {
            in_code = !in_code;
            push_word(&mut word, &mut word_start, out);
            continue;
        }
        if in_code {
            continue;
        }
        if c.is_alphanumeric() || matches!(c, '\'' | '’' | '-') {
            if word_start.is_none() {
                word_start = Some(base + i);
                word.clear();
            }
            word.push(c);
        } else {
            push_word(&mut word, &mut word_start, out);
        }
    }
    push_word(&mut word, &mut word_start, out);
}

/// Finish the word accumulated in `word`, which started at `*word_start`
/// (global char offset). Trims apostrophes and hyphens from the edges;
/// requires 2–64 characters with at least one letter.
fn push_word(
    word: &mut String,
    word_start: &mut Option<usize>,
    out: &mut Vec<(usize, usize, String)>,
) {
    if let Some(start) = word_start.take() {
        let lead = word
            .chars()
            .take_while(|c| matches!(c, '\'' | '’' | '-'))
            .count();
        let trail = word
            .chars()
            .rev()
            .take_while(|c| matches!(c, '\'' | '’' | '-'))
            .count();
        let len = word.chars().count();
        if len >= lead + trail + 2 && len - lead - trail <= 64 {
            let core: String = word.chars().skip(lead).take(len - lead - trail).collect();
            if core.chars().any(char::is_alphabetic) {
                out.push((start + lead, start + lead + (len - lead - trail), core));
            }
        }
        word.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words_of(text: &str) -> Vec<String> {
        words(text).into_iter().map(|(_, _, s)| s).collect()
    }

    #[test]
    fn words_split_on_punctuation() {
        let got = words_of("Hello, world! don't e-mail x a the 2024");
        assert!(got.contains(&"Hello".into()));
        assert!(got.contains(&"world".into()));
        assert!(got.contains(&"don't".into()));
        assert!(got.contains(&"e-mail".into()));
        assert!(got.contains(&"the".into()));
        // Single characters and pure numbers are skipped.
        assert!(!got.contains(&"x".into()));
        assert!(!got.contains(&"a".into()));
        assert!(!got.contains(&"2024".into()));
    }

    #[test]
    fn backtick_fences_skipped() {
        let got = words_of("real words\n```\ncode qqqq\n```\nafter qqqq");
        assert!(got.contains(&"real".into()));
        assert!(got.contains(&"after".into()));
        // Inside the fence: neither word.
        assert!(!got.contains(&"code".into()));
        // "qqqq" appears both inside (skipped) and after (kept); verify
        // precisely by offsets: only the occurrence on the last line.
        let ranges = words("real words\n```\ncode qqqq\n```\nafter qqqq");
        assert!(ranges
            .iter()
            .all(|(s, _, w)| w != "qqqq" || *s > 30),
            "{ranges:?}");
    }

    #[test]
    fn fence_with_info_string_opens() {
        let got = words_of("```rust\nfn qqqq() {}\n```\nplain");
        assert!(!got.contains(&"fn".into()));
        assert!(!got.contains(&"qqqq".into()));
        assert!(got.contains(&"plain".into()));
    }

    #[test]
    fn tilde_fences_skipped() {
        let got = words_of("~~~\nqqqq\n~~~\nplain");
        assert!(!got.contains(&"qqqq".into()));
        assert!(got.contains(&"plain".into()));
    }

    #[test]
    fn inline_code_skipped() {
        let got = words_of("plain `qqq` more\nsecond `zzz` line");
        assert!(!got.contains(&"qqq".into()));
        assert!(!got.contains(&"zzz".into()));
        assert!(got.contains(&"plain".into()));
        assert!(got.contains(&"more".into()));
        assert!(got.contains(&"second".into()));
    }

    #[test]
    fn front_matter_skipped() {
        let got = words_of("---\ntitle: qqqq\nlayout: zzzz\n---\nbody qqqq");
        assert!(!got.contains(&"title".into()));
        assert!(!got.contains(&"layout".into()));
        assert!(got.contains(&"body".into()));
    }

    #[test]
    fn offsets_are_char_based() {
        // Multi-byte characters before the word must not shift offsets.
        let text = "héllo wörld „qqqq”";
        for (start, end, word) in words(text) {
            let extracted = text
                .chars()
                .skip(start)
                .take(end - start)
                .collect::<String>();
            assert_eq!(extracted, word, "offsets for {word:?}");
        }
    }

    #[test]
    fn misspelled_ranges_uses_checker() {
        let Some(sp) = SpellChecker::new() else {
            eprintln!("no enchant available; skipping");
            return;
        };
        let langs = sp.available_languages();
        if langs.is_empty() {
            eprintln!("no dictionaries; skipping");
            return;
        }
        let lang = langs
            .iter()
            .find(|l| l.starts_with("en"))
            .cloned()
            .or_else(|| langs.first().cloned());
        let ranges = misspelled_ranges("hello qqqqzzzzx", &sp, lang.as_deref());
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0], (6, 15));
    }
}

#[cfg(test)]
mod incremental {
    use super::*;
    use crate::editor::line_cache::LineProcessor as _;

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
        "hello world this is prose",
        "```",
        "code qqquzzzz not checked",
        "```",
        "plain `qqqzz inline` more words",
        "---",
        "title: front matter qqquzz",
        "---",
        "body text after the fence",
        "don't e-mail caffè 2024 x",
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
                let mut end = (pos + rng.below(20)).min(text.len());
                while !text.is_char_boundary(end) {
                    end += 1;
                }
                text.replace_range(pos..end, "");
            }
            _ => text.insert_str(pos, "\n"),
        }
        hint
    }

    /// Expand the per-line cache to document-absolute ranges.
    fn doc_ranges(doc: &SpellDoc, text: &str) -> Vec<(usize, usize)> {
        let lines: Vec<&str> = text.split('\n').collect();
        let mut out = Vec::new();
        let mut pos = 0usize;
        for (i, line) in lines.iter().enumerate() {
            if let Some(entry) = doc.entry(i) {
                for (s, e) in &entry.value {
                    out.push((pos + s, pos + e));
                }
            }
            pos += line.chars().count() + 1;
        }
        out
    }

    /// The incremental spell state must agree with the reference
    /// `misspelled_ranges` after every edit.
    #[test]
    fn incremental_matches_reference_through_random_edits() {
        let Some(sp) = SpellChecker::new() else {
            eprintln!("no enchant available; skipping");
            return;
        };
        let langs = sp.available_languages();
        if langs.is_empty() {
            eprintln!("no dictionaries; skipping");
            return;
        }
        let lang = langs
            .iter()
            .find(|l| l.starts_with("en"))
            .cloned()
            .or_else(|| langs.first().cloned());
        let mut rng = Rng(0x5ce1_1d0c_0ffe_e001);
        for round in 0..10 {
            let lines = 5 + rng.below(15);
            let mut text = random_doc(&mut rng, lines);
            let mut doc = SpellDoc::new(lang.clone());
            doc.update(&text, 0, &sp, lang.as_deref());
            assert_eq!(
                doc_ranges(&doc, &text),
                misspelled_ranges(&text, &sp, lang.as_deref()),
                "round {round} initial: {text:?}"
            );
            for edit in 0..15 {
                let hint = random_edit(&mut rng, &mut text);
                doc.update(&text, hint, &sp, lang.as_deref());
                assert_eq!(
                    doc_ranges(&doc, &text),
                    misspelled_ranges(&text, &sp, lang.as_deref()),
                    "round {round} edit {edit}: {text:?}"
                );
            }
        }
    }
}
