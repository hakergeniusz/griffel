//! Incremental per-line caching shared by highlighting, statistics and
//! spell checking (`01-editing.md` §9: background processing must keep
//! multi-megabyte documents responsive).
//!
//! Every consumer is line-decomposable with a small cross-line state
//! (fence/front-matter/blank): a line's value depends only on its text and
//! the state entering it. That makes an edit cheap to propagate: lines
//! before it are untouched, the edited lines recompute, and lines after it
//! reuse their cached values as soon as the carried state matches again —
//! which is exactly what the walk below detects, using content hashes so
//! shifted lines (inserts/deletes) re-align without a diff.
//!
//! The walk never needs an exact edit range for correctness: `dirty_from`
//! is a conservative hint for the first affected line (the caller records
//! the minimum event line), and a hash mismatch simply forces a recompute.
//! A lookahead window recovers alignment after inserts and deletes of up
//! to `LOOKAHEAD` lines; bigger structural edits fall back to recomputing
//! the rest of the document once, which matches the previous whole-file
//! behavior.

/// FNV-1a 64-bit content hash of a line (fast, allocation-free).
pub fn line_hash(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in s.as_bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Per-line processing: value + carried state for one line of text.
pub trait LineProcessor {
    /// Cross-line state entering a line. `Default` is the state before the
    /// first line of a document.
    type State: Clone + PartialEq + Default;
    /// Cached value for one line.
    type Value;
    /// Compute the value of `line` given the state entering it. `is_last`
    /// marks the final line of the document (a few state machines, like
    /// spell checking's front matter, care whether anything follows).
    fn process(
        &mut self,
        line: &str,
        in_state: &Self::State,
        is_last: bool,
    ) -> (Self::Value, Self::State);
}

pub struct Entry<P: LineProcessor + ?Sized> {
    pub hash: u64,
    /// State this line was computed with (reuse requires an exact match).
    pub in_state: P::State,
    pub out_state: P::State,
    pub value: P::Value,
}

/// One cached entry per line of `text.split('\n')`.
pub struct LineCache<P: LineProcessor + ?Sized> {
    entries: Vec<std::sync::Arc<Entry<P>>>,
}

impl<P: LineProcessor + ?Sized> Clone for LineCache<P> {
    /// Shallow: entries are shared via `Arc`, so cloning hands the same
    /// immutable per-line values to a background update.
    fn clone(&self) -> Self {
        Self {
            entries: self.entries.clone(),
        }
    }
}

impl<P: LineProcessor + ?Sized> Default for LineCache<P> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
        }
    }
}

/// Lookahead for re-alignment after inserts/deletes of whole lines.
const LOOKAHEAD: usize = 16;

impl<P: LineProcessor + ?Sized> LineCache<P> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn line_count(&self) -> usize {
        self.entries.len()
    }

    pub fn entries(&self) -> &[std::sync::Arc<Entry<P>>] {
        &self.entries
    }

    /// Bring the cache in sync with `text`. `dirty_from` is a conservative
    /// hint for the first line whose content may have changed (0 = rebuild
    /// everything). Returns the inclusive ranges of lines whose values were
    /// recomputed; untouched lines are guaranteed to have identical values.
    pub fn update(
        &mut self,
        text: &str,
        dirty_from: usize,
        proc: &mut P,
    ) -> Vec<(usize, usize)> {
        self.update_with(text, dirty_from, proc, &P::State::default())
    }

    /// `update` with an explicit state before the first line — for
    /// consumers whose value computation depends on more than the carried
    /// document state (e.g. Code Mode token coloring).
    pub fn update_with(
        &mut self,
        text: &str,
        dirty_from: usize,
        proc: &mut P,
        initial_state: &P::State,
    ) -> Vec<(usize, usize)> {
        let old = std::mem::take(&mut self.entries);
        let lines: Vec<&str> = text.split('\n').collect();
        let prefix_len = dirty_from.min(old.len()).min(lines.len());

        let mut entries: Vec<std::sync::Arc<Entry<P>>> = Vec::with_capacity(lines.len());
        entries.extend_from_slice(&old[..prefix_len]);
        let mut in_state = entries
            .last()
            .map(|e| e.out_state.clone())
            .unwrap_or_else(|| initial_state.clone());

        let mut changed: Vec<(usize, usize)> = Vec::new();
        let mut run_start: Option<usize> = None;
        let mut j = prefix_len;

        for (i, line) in lines.iter().enumerate().skip(prefix_len) {
            let h = line_hash(line);
            // Re-alignment: an old entry with the same content hash and the
            // same entering state is valid to reuse, however far the edit
            // shifted it (within the window).
            let window_end = (j + LOOKAHEAD).min(old.len());
            let reused = old[j..window_end].iter().enumerate().find_map(|(off, e)| {
                (e.hash == h && e.in_state == in_state).then_some(off + j)
            });
            match reused {
                Some(k) => {
                    if let Some(s) = run_start.take() {
                        changed.push((s, i - 1));
                    }
                    let e = old[k].clone();
                    in_state = e.out_state.clone();
                    entries.push(e);
                    j = k + 1;
                }
                None => {
                    let (value, out) = proc.process(line, &in_state, i + 1 == lines.len());
                    entries.push(std::sync::Arc::new(Entry {
                        hash: h,
                        in_state: in_state.clone(),
                        out_state: out.clone(),
                        value,
                    }));
                    in_state = out;
                    run_start.get_or_insert(i);
                }
            }
        }
        if let Some(s) = run_start.take() {
            changed.push((s, lines.len().saturating_sub(1)));
        }
        self.entries = entries;
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Uppercase the line and carry whether the previous line was blank.
    struct Upper;
    impl LineProcessor for Upper {
        type State = bool;
        type Value = String;
        fn process(&mut self, line: &str, prev_blank: &bool, _is_last: bool) -> (String, bool) {
            (format!("{line}{}", if *prev_blank { "!" } else { "" }), line.trim().is_empty())
        }
    }

    fn values(cache: &LineCache<Upper>) -> Vec<String> {
        cache.entries().iter().map(|e| e.value.clone()).collect()
    }

    #[test]
    fn update_matches_full_rebuild() {
        let text = "one\ntwo\n\nthree";
        let mut a = LineCache::new();
        a.update(text, 0, &mut Upper);
        let mut b = LineCache::new();
        b.update(text, 2, &mut Upper); // hint beyond an empty cache is fine
        assert_eq!(values(&a), values(&b));
    }

    #[test]
    fn edit_one_line_reuses_the_rest() {
        let text = "a\nb\nc\nd\ne";
        let mut cache = LineCache::new();
        cache.update(text, 0, &mut Upper);
        // Change only line 2.
        let edited = "a\nb\nC\nd\ne";
        let changed = cache.update(edited, 2, &mut Upper);
        assert_eq!(changed, vec![(2, 2)]);
        assert_eq!(values(&cache)[2], "C");
        assert_eq!(values(&cache), values_of(edited));
    }

    #[test]
    fn insert_and_delete_lines_re_align() {
        let text = "a\nb\nc\nd";
        let mut cache = LineCache::new();
        cache.update(text, 0, &mut Upper);
        // Insert two lines after "b".
        let inserted = "a\nb\nx\ny\nc\nd";
        let changed = cache.update(inserted, 2, &mut Upper);
        assert_eq!(values(&cache), values_of(inserted));
        assert!(changed.contains(&(2, 3)), "{changed:?}");
        // Delete them again: the shifted "c"/"d" entries re-align and are
        // reused, so nothing needs re-applying (their tags move with the
        // text).
        let changed = cache.update(text, 2, &mut Upper);
        assert_eq!(values(&cache), values_of(text));
        assert!(changed.is_empty(), "{changed:?}");
    }

    #[test]
    fn state_change_propagates_until_it_stabilizes() {
        // The value of a line depends on whether the previous line was
        // blank, so blanking one line affects the next non-blank line too.
        let text = "a\nb\nc\nd";
        let mut cache = LineCache::new();
        cache.update(text, 0, &mut Upper);
        let edited = "a\n\n\nc\nd";
        let changed = cache.update(edited, 1, &mut Upper);
        assert_eq!(values(&cache), values_of(edited));
        assert_eq!(changed, vec![(1, 3)]);
    }

    #[test]
    fn shrinking_text_drops_entries() {
        let text = "a\nb\nc";
        let mut cache = LineCache::new();
        cache.update(text, 0, &mut Upper);
        let changed = cache.update("a", 1, &mut Upper);
        assert_eq!(cache.line_count(), 1);
        assert_eq!(values(&cache), vec!["a".to_owned()]);
        // The deleted lines' tags vanish with their text; nothing to redo.
        assert!(changed.is_empty());
    }

    #[test]
    fn stale_hint_beyond_the_document_is_safe() {
        let text = "a\nb\nc";
        let mut cache = LineCache::new();
        cache.update(text, 0, &mut Upper);
        let changed = cache.update(text, 99, &mut Upper);
        assert!(changed.is_empty());
        assert_eq!(values(&cache), values_of(text));
    }

    fn values_of(text: &str) -> Vec<String> {
        let mut cache = LineCache::new();
        cache.update(text, 0, &mut Upper);
        values(&cache)
    }
}
