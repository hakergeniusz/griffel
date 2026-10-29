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

impl SpellRunner {
    /// Schedule a scan of `text`; ranges are applied to `buffer` on the
    /// main loop if the text has not changed since the scan started.
    pub fn schedule(
        &self,
        buffer: &sourceview5::Buffer,
        text: String,
        lang: Option<String>,
        enabled: bool,
    ) {
        let _generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        if !enabled || self.checker.is_none() {
            clear(buffer);
            return;
        }
        let cb_buffer = buffer.clone();
        let checker = self.checker.clone().unwrap();
        glib::spawn_future_local(async move {
            let len = text.chars().count();
            let handle =
                gio::spawn_blocking(move || misspelled_ranges(&text, &checker, lang.as_deref()));
            let ranges = handle.await.unwrap_or_default();
            if cb_buffer.char_count() as usize != len {
                return;
            }
            apply(&cb_buffer, &ranges);
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
