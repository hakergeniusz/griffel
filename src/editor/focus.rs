//! Focus Mode (`03-behavior/04-focus-and-hemingway.md` §1): dim
//! everything outside the sentence containing the cursor and keep the
//! cursor line vertically centered (typewriter scrolling).

use gtk::prelude::*;

use super::Editor;
use crate::appearance::EditorPalette;

/// Name of the dim tag this module owns.
pub const DIM_TAG: &str = "focus-dim";

/// Create the dim tag if missing and give it top priority so its gray
/// foreground wins over the highlighting tags' colors.
pub fn ensure_tag(buffer: &sourceview5::Buffer) {
    let table = buffer.tag_table();
    if table.lookup(DIM_TAG).is_none() {
        let tag = gtk::TextTag::new(Some(DIM_TAG));
        table.add(&tag);
    }
    if let Some(tag) = table.lookup(DIM_TAG) {
        tag.set_priority(table.size().saturating_sub(1));
    }
}

/// Re-color the dim tag after a scheme change.
pub fn update_palette(buffer: &sourceview5::Buffer, palette: &EditorPalette) {
    let table = buffer.tag_table();
    if let Some(tag) = table.lookup(DIM_TAG) {
        tag.set_foreground_rgba(Some(&super::highlight::parse_color(&palette.dim_focus)));
    }
}

// ---------------------------------------------------------------------------
// Sentence detection (pure; unit-tested)
// ---------------------------------------------------------------------------

fn is_terminator(c: char) -> bool {
    matches!(c, '.' | '!' | '?' | '…')
}

/// Char indices where sentences start/end in `text`. A sentence ends
/// after a run of sentence-ending punctuation followed by whitespace or
/// the end of the text; the following whitespace belongs to the ending
/// sentence. Sentences: `("One.  Two! Three?", cursor)` etc.
fn sentence_segments(text: &str) -> Vec<(usize, usize)> {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut segments = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;
    while i < len {
        if is_terminator(chars[i]) {
            let mut j = i;
            while j < len && is_terminator(chars[j]) {
                j += 1;
            }
            let mut k = j;
            while k < len && chars[k].is_whitespace() {
                k += 1;
            }
            if k >= len {
                // Trailing terminators/whitespace at end of text: the
                // last sentence runs to the end.
                break;
            }
            segments.push((start, k));
            start = k;
            i = k;
        } else {
            i += 1;
        }
    }
    segments.push((start, len));
    segments
}

/// The (char-offset) range of the sentence containing `cursor`.
/// Text outside this range is dimmed while Focus Mode is on.
pub fn sentence_bounds(text: &str, cursor: usize) -> (usize, usize) {
    let len = text.chars().count();
    // A cursor at (or past) the very end belongs to the last sentence.
    let cursor = len.saturating_sub(1).min(cursor);
    for (start, end) in sentence_segments(text) {
        if start <= cursor && cursor < end {
            return (start, end);
        }
    }
    (0, len)
}

/// Outcome of a localized sentence lookup: exact bounds, or `NeedMore`
/// when the scan hit the edge of the provided slice without a conclusion
/// (the caller may have truncated the text).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SentenceResult {
    Bounds(usize, usize),
    NeedMore,
}

/// The sentence range containing `cursor`, computed by scanning outward
/// from the cursor instead of over the whole document (`01-editing.md` §9:
/// no per-keystroke full-document work). `at_text_start`/`at_text_end`
/// say whether the slice edges are the real document edges; with a window
/// that is not, a scan reaching an edge returns `NeedMore` and the caller
/// falls back to the full scan. With both edges real (a full document)
/// the result is exactly `sentence_bounds`.
pub fn sentence_bounds_around(
    text: &[char],
    cursor: usize,
    at_text_start: bool,
    at_text_end: bool,
) -> SentenceResult {
    let len = text.len();
    if len == 0 {
        return SentenceResult::Bounds(0, 0);
    }
    let cursor = cursor.min(len - 1);

    // Backward: the nearest terminator run before the cursor.
    let mut t = None;
    let mut i = cursor;
    while i > 0 {
        i -= 1;
        if is_terminator(text[i]) {
            t = Some(i);
            break;
        }
    }
    let Some(t) = t else {
        // No terminator at all before the cursor: this is the first
        // sentence — but only if the text really starts here.
        if !at_text_start {
            return SentenceResult::NeedMore;
        }
        return match sentence_end_forward(text, cursor, at_text_end) {
            SentenceResult::Bounds(end, _) => SentenceResult::Bounds(0, end),
            _ => SentenceResult::NeedMore,
        };
    };
    // The terminator run containing `t`: [rs, re).
    let mut rs = t;
    while rs > 0 && is_terminator(text[rs - 1]) {
        rs -= 1;
    }
    if rs == 0 && !at_text_start {
        // The run may extend beyond the slice.
        return SentenceResult::NeedMore;
    }
    let mut re = t + 1;
    while re < len && is_terminator(text[re]) {
        re += 1;
    }
    // The whitespace run after it: [re, k).
    let mut k = re;
    while k < len && text[k].is_whitespace() {
        k += 1;
    }
    if k >= len {
        // Trailing terminators/whitespace at the end of the text: the last
        // sentence runs to the end (sentence_segments breaks out).
        if !at_text_end {
            return SentenceResult::NeedMore;
        }
        return match boundary_start_before(text, rs, at_text_start) {
            SentenceResult::Bounds(start, _) => SentenceResult::Bounds(start, len),
            _ => SentenceResult::NeedMore,
        };
    }
    if cursor >= k {
        // The boundary completes before the cursor: the sentence starts at
        // k; the end is the next boundary forward.
        return match sentence_end_forward(text, cursor, at_text_end) {
            SentenceResult::Bounds(end, _) => SentenceResult::Bounds(k, end),
            _ => SentenceResult::NeedMore,
        };
    }
    // The cursor sits inside the terminator run or its whitespace run: it
    // belongs to the ending sentence, which closes at k.
    match boundary_start_before(text, rs, at_text_start) {
        SentenceResult::Bounds(start, _) => SentenceResult::Bounds(start, k),
        _ => SentenceResult::NeedMore,
    }
}

/// End of the sentence containing `cursor`: the end of the whitespace run
/// following the next terminator run at or after `cursor`.
fn sentence_end_forward(text: &[char], cursor: usize, at_text_end: bool) -> SentenceResult {
    let len = text.len();
    let mut i = cursor;
    loop {
        if i >= len {
            if at_text_end {
                return SentenceResult::Bounds(len, len);
            }
            return SentenceResult::NeedMore;
        }
        if is_terminator(text[i]) {
            let mut re = i + 1;
            while re < len && is_terminator(text[re]) {
                re += 1;
            }
            let mut k = re;
            while k < len && text[k].is_whitespace() {
                k += 1;
            }
            if k >= len {
                if at_text_end {
                    return SentenceResult::Bounds(len, len);
                }
                return SentenceResult::NeedMore;
            }
            return SentenceResult::Bounds(k, k);
        }
        i += 1;
    }
}

/// The `k` of the boundary event before the terminator run starting at
/// `rs` — i.e. where the previous sentence begins.
fn boundary_start_before(text: &[char], rs: usize, at_text_start: bool) -> SentenceResult {
    let mut i = rs;
    while i > 0 {
        i -= 1;
        if is_terminator(text[i]) {
            let mut re2 = i + 1;
            while re2 < rs && is_terminator(text[re2]) {
                re2 += 1;
            }
            let mut k2 = re2;
            while k2 < rs && text[k2].is_whitespace() {
                k2 += 1;
            }
            return SentenceResult::Bounds(k2, k2);
        }
    }
    if at_text_start {
        SentenceResult::Bounds(0, 0)
    } else {
        SentenceResult::NeedMore
    }
}

// ---------------------------------------------------------------------------
// Editor integration
// ---------------------------------------------------------------------------

impl Editor {
    /// Toggle Focus Mode. Switching on grabs focus to the editor and
    /// immediately applies dimming and centering (§1).
    pub fn set_focus_mode(&self, active: bool) {
        self.focus_active.set(active);
        ensure_tag(&self.buffer);
        self.focus_recolor();
        if active {
            self.saved_top_margin.set(self.view.top_margin());
            self.saved_bottom_margin.set(self.view.bottom_margin());
            self.apply_focus_margins();
            self.view.grab_focus();
            self.focus_update();
        } else {
            self.view.set_top_margin(self.saved_top_margin.get());
            self.view
                .set_bottom_margin(self.saved_bottom_margin.get());
            self.clear_dim();
            let generation = self.typewriter_generation.get() + 1;
            self.typewriter_generation.set(generation);
        }
    }

    /// Focus Mode margins: top/bottom become half the viewport height so
    /// the first/last line can also reach the center (§1).
    pub fn apply_focus_margins(&self) {
        if !self.focus_active.get() {
            return;
        }
        let half = (self.scroll.vadjustment().page_size() / 2.0).round() as i32;
        self.view.set_top_margin(half + self.bar_comp.get());
        self.view.set_bottom_margin(half);
    }

    /// Update the bar-space compensation and re-apply the top margin
    /// (`14-presentation-details.md` §2).
    pub fn set_bar_comp(&self, h: i32) {
        let delta = h - self.bar_comp.get();
        if delta == 0 {
            return;
        }
        self.bar_comp.set(h);
        if self.focus_active.get() {
            self.apply_focus_margins();
        } else {
            self.view.set_top_margin(self.view.top_margin() + delta);
        }
    }

    /// Re-evaluate dimming and typewriter scrolling (cursor moved or the
    /// text changed). No-op while Focus Mode is off.
    ///
    /// The sentence is found by scanning outward from the cursor over a
    /// bounded window (`01-editing.md` §9: no per-keystroke whole-document
    /// work). If a sentence runs past the window, the window grows
    /// geometrically up to a cap. Documents smaller than the cap are always
    /// exact; for an even larger windowless sentence we dim to the window
    /// edges instead of copying the whole multi-megabyte buffer on every
    /// keystroke.
    pub fn focus_update(&self) {
        if !self.focus_active.get() {
            return;
        }
        const WINDOW: i32 = 64 * 1024;
        // Upper bound on the localized scan. At this size a scan costs a
        // few milliseconds; growing without bound reintroduced exactly the
        // per-keystroke full-document copy this design avoids.
        const MAX_WINDOW: i32 = 2 * 1024 * 1024;
        let cursor_i = self
            .buffer
            .iter_at_mark(&self.buffer.get_insert())
            .offset()
            .max(0);
        let total = self.buffer.char_count();
        let mut half = WINDOW;
        let (start, end) = loop {
            let w0 = (cursor_i - half).max(0);
            let w1 = (cursor_i + half).min(total);
            let window: Vec<char> = self
                .buffer
                .text(
                    &self.buffer.iter_at_offset(w0),
                    &self.buffer.iter_at_offset(w1),
                    false,
                )
                .chars()
                .collect();
            match sentence_bounds_around(
                &window,
                (cursor_i - w0) as usize,
                w0 == 0,
                w1 == total,
            ) {
                SentenceResult::Bounds(s, e) => break ((w0 as usize) + s, (w0 as usize) + e),
                SentenceResult::NeedMore => {
                    if w0 == 0 && w1 == total {
                        // The whole document is in the window: the pure
                        // scanner always resolves it, so this is defensive.
                        break (0, total as usize);
                    }
                    if half >= MAX_WINDOW {
                        // Cap the work: dim only outside the window.
                        break (w0 as usize, w1 as usize);
                    }
                    half = (half.saturating_mul(4)).min(MAX_WINDOW).max(half + 1);
                }
            }
        };
        self.clear_dim();
        if start > 0 {
            let s = self.buffer.start_iter();
            let e = self.buffer.iter_at_offset(start as i32);
            self.buffer.apply_tag_by_name(DIM_TAG, &s, &e);
        }
        if (end as i32) < total {
            let s = self.buffer.iter_at_offset(end as i32);
            let e = self.buffer.end_iter();
            self.buffer.apply_tag_by_name(DIM_TAG, &s, &e);
        }
        self.typewriter_scroll();
    }

    fn clear_dim(&self) {
        let start = self.buffer.start_iter();
        let end = self.buffer.end_iter();
        self.buffer.remove_tag_by_name(DIM_TAG, &start, &end);
    }

    pub fn focus_recolor(&self) {
        update_palette(&self.buffer, &crate::appearance::effective_palette());
    }

    /// Smooth-scroll the view so the cursor line sits vertically
    /// centered (typewriter scrolling, §1).
    pub fn typewriter_scroll(&self) {
        let iter = self.buffer.iter_at_mark(&self.buffer.get_insert());
        let rect = self.view.iter_location(&iter);
        let vadj = self.scroll.vadjustment();
        let target =
            f64::from(rect.y()) + f64::from(rect.height()) / 2.0 - vadj.page_size() / 2.0;
        let current = vadj.value();
        if (current - target).abs() < 1.0 {
            return;
        }

        // Short eased animation; a newer scroll request cancels this one.
        let generation = self.typewriter_generation.get() + 1;
        self.typewriter_generation.set(generation);
        let editor = self.clone();
        let steps = 8;
        let mut i = 0u32;
        glib::timeout_add_local(std::time::Duration::from_millis(16), move || {
            if editor.typewriter_generation.get() != generation {
                return glib::ControlFlow::Break;
            }
            i += 1;
            let vadj = editor.scroll.vadjustment();
            let t = i as f64 / steps as f64;
            let eased = 1.0 - (1.0 - t) * (1.0 - t);
            vadj.set_value(current + (target - current) * eased);
            if i >= steps {
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sentence_bounds_basic() {
        //        01234567890123456
        let text = "One. Two! Three?";
        // Cursor inside "Two".
        assert_eq!(sentence_bounds(text, 7), (5, 10));
        // Cursor at the very start.
        assert_eq!(sentence_bounds(text, 0), (0, 5));
        // Cursor at the end (inside "Three?").
        assert_eq!(sentence_bounds(text, text.chars().count() - 1), (10, 16));
    }

    #[test]
    fn sentence_bounds_cursor_in_gap_belongs_to_ending_sentence() {
        let text = "One. Two!";
        // Cursor on the space after "One." (index 4).
        assert_eq!(sentence_bounds(text, 4), (0, 5));
        // Cursor right after the period (index 3).
        assert_eq!(sentence_bounds(text, 3), (0, 5));
        // Cursor just before the period.
        assert_eq!(sentence_bounds(text, 2), (0, 5));
    }

    #[test]
    fn sentence_bounds_runs_of_terminators() {
        let text = "Really?! Yes...";
        // "Really?! " spans 0..9, "Yes..." is 9..15.
        assert_eq!(sentence_bounds(text, 2), (0, 9));
        assert_eq!(sentence_bounds(text, 11), (9, 15));
    }

    #[test]
    fn sentence_bounds_no_terminators() {
        let text = "just some text";
        assert_eq!(sentence_bounds(text, 5), (0, 14));
        assert_eq!(sentence_bounds(text, 0), (0, 14));
    }

    #[test]
    fn sentence_bounds_empty_and_oversized_cursor() {
        assert_eq!(sentence_bounds("", 0), (0, 0));
        assert_eq!(sentence_bounds("Hi.", 100), (0, 3));
    }

    #[test]
    fn sentence_bounds_trailing_newline() {
        let text = "One.\nTwo.";
        // Cursor on the second line.
        assert_eq!(sentence_bounds(text, 6), (5, 9));
        // Cursor on the newline belongs to the first sentence.
        assert_eq!(sentence_bounds(text, 4), (0, 5));
    }

    #[test]
    fn sentence_bounds_multiline_paragraph() {
        let text = "First sentence here.\nAnd another one!\nThird";
        assert_eq!(sentence_bounds(text, 10), (0, 21));
        assert_eq!(sentence_bounds(text, 30), (21, 38));
        assert_eq!(sentence_bounds(text, 45), (38, 43));
    }

    #[test]
    fn localized_bounds_match_reference() {
        let cases = [
            ("One. Two! Three?", 7),
            ("One.\nTwo.", 6),
            ("Really?! Yes...", 11),
            ("just some text", 5),
            ("Hi.", 100),
            ("No terminator run here at all", 20),
            ("Trailing only.  ", 14),
            ("ends with period.", 16),
            ("??? leading run", 1),
            ("a!b?c.d e", 8),
        ];
        for (text, cursor) in cases {
            let chars: Vec<char> = text.chars().collect();
            for c in 0..=chars.len() {
                assert_eq!(
                    sentence_bounds_around(&chars, c, true, true),
                    into_result(sentence_bounds(text, c)),
                    "{text:?} cursor {c}"
                );
            }
        }
    }

    /// Windowed lookups with generous edges are exact; a window that is too
    /// small to contain a boundary honestly reports `NeedMore` instead of
    /// guessing.
    #[test]
    fn windowed_bounds_exact_or_honest() {
        let text = "First. Second. Third.";
        let chars: Vec<char> = text.chars().collect();
        for c in 0..chars.len() {
            // Window around the cursor, ±4 chars, never reaching either
            // document edge for interior cursors.
            let w0 = c.saturating_sub(4);
            let w1 = (c + 4).min(chars.len());
            let slice = &chars[w0..w1];
            match sentence_bounds_around(slice, c - w0, w0 == 0, w1 == chars.len()) {
                SentenceResult::Bounds(s, e) => {
                    assert_eq!(
                        (w0 + s, w0 + e),
                        sentence_bounds(text, c),
                        "{text:?} cursor {c}"
                    );
                }
                SentenceResult::NeedMore => {}
            }
        }
    }

    #[test]
    fn windowed_fallback_happens_for_giant_sentences() {
        // A sentence far longer than any window: the localized scan cannot
        // conclude and reports NeedMore (the editor then does a full scan).
        let text = "x".repeat(100_000);
        let chars: Vec<char> = text.chars().collect();
        assert_eq!(
            sentence_bounds_around(&chars, 50_000, false, false),
            SentenceResult::NeedMore
        );
        // With real document edges it is exact.
        assert_eq!(
            sentence_bounds_around(&chars, 50_000, true, true),
            SentenceResult::Bounds(0, 100_000)
        );
    }

    fn into_result(b: (usize, usize)) -> SentenceResult {
        SentenceResult::Bounds(b.0, b.1)
    }
}
