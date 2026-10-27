//! Pure decision logic behind the formatting commands
//! (`03-behavior/02-formatting.md`).

/// Whether invoking a wrap command on the current selection should unwrap:
/// the selection extended by one delimiter width on each side already
/// begins and ends with the delimiter.
pub fn should_unwrap(before: &str, selection: &str, after: &str, delim: &str) -> bool {
    if before.len() < delim.len() || after.len() < delim.len() {
        return false;
    }
    before.ends_with(delim) && after.starts_with(delim) && !selection.is_empty()
}

/// Heading level after applying the heading command: 1 → 2 → 3 → none;
/// manually typed levels 4–6 cycle back to 1.
pub fn next_heading_level(current: Option<u8>) -> Option<u8> {
    match current {
        None => Some(1),
        Some(1) => Some(2),
        Some(2) => Some(3),
        Some(3) => None,
        Some(_) => Some(1),
    }
}

/// Whether the selection text looks like a URL (for the link command).
pub fn is_url(text: &str) -> bool {
    let t = text.trim();
    (t.starts_with("http://")
        || t.starts_with("https://")
        || t.starts_with("ftp://")
        || t.starts_with("mailto:")
        || t.starts_with("file://"))
        && !t.contains(char::is_whitespace)
}

/// Build a fixed-width plain-text table (`02-formatting.md` §7): a blank
/// header row, a dash separator row, then blank data rows. Cell width is
/// chosen so the table fits `chars_per_line`, capped at 20 characters.
pub fn build_table(rows: usize, cols: usize, chars_per_line: usize) -> String {
    let cols = cols.max(1);
    let rows = rows.max(1);
    // | c | c |  → cols+1 separator bars plus one space around each cell.
    let overhead = cols + 1 + 2 * cols;
    let cell = ((chars_per_line.saturating_sub(overhead)) / cols).clamp(1, 20);

    let blank_row = {
        let mut r = String::new();
        r.push('|');
        for _ in 0..cols {
            r.push_str(&" ".repeat(cell + 2));
            r.push('|');
        }
        r
    };
    let sep_row = {
        let mut r = String::new();
        r.push('|');
        for _ in 0..cols {
            r.push_str(&"-".repeat(cell + 2));
            r.push('|');
        }
        r
    };
    let mut out = Vec::with_capacity(rows + 1);
    out.push(blank_row.clone());
    out.push(sep_row);
    for _ in 1..rows {
        out.push(blank_row.clone());
    }
    out.join("\n")
}

/// Re-align a fixed-width table block so every column is as wide as its
/// widest cell. Returns None when `lines` do not form a table.
pub fn fix_table(lines: &[&str]) -> Option<Vec<String>> {
    if lines.len() < 2 {
        return None;
    }
    // Parse rows: lines made of | … | cells (or +-+ corners).
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut separator_row: Option<usize> = None;
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim();
        if t.starts_with('|') && t.ends_with('|') {
            let inner = &t[1..t.len() - 1];
            let cells: Vec<String> = inner.split('|').map(str::trim).map(str::to_owned).collect();
            // Separator row: all cells are dash runs.
            if cells.iter().all(|c| !c.is_empty() && c.chars().all(|ch| ch == '-')) {
                separator_row = Some(i);
            }
            rows.push(cells);
        } else if t.starts_with('+') && t.ends_with('+') {
            // Alternative style corner row: treat as separator.
            let cells: Vec<String> = t
                .trim_matches('+')
                .split('+')
                .map(|c| "-".repeat(c.len()))
                .collect();
            separator_row = Some(i);
            rows.push(cells);
        } else {
            return None;
        }
    }
    let cols = rows.first()?.len();
    if rows.iter().any(|r| r.len() != cols) || cols == 0 {
        return None;
    }

    // Widest content per column (excluding separator dashes).
    let mut widths = vec![0usize; cols];
    for (i, row) in rows.iter().enumerate() {
        if separator_row == Some(i) {
            continue;
        }
        for (c, cell) in row.iter().enumerate() {
            widths[c] = widths[c].max(cell.chars().count());
        }
    }
    if widths.iter().all(|w| *w == 0) {
        return None;
    }

    let mut out = Vec::with_capacity(rows.len());
    for (i, row) in rows.iter().enumerate() {
        if separator_row == Some(i) {
            let mut r = String::new();
            r.push('|');
            for w in &widths {
                r.push(' ');
                r.push_str(&"-".repeat(*w));
                r.push(' ');
                r.push('|');
            }
            out.push(r);
        } else {
            let mut r = String::new();
            r.push('|');
            for (c, cell) in row.iter().enumerate() {
                let pad = widths[c] - cell.chars().count();
                r.push(' ');
                r.push_str(cell);
                r.push_str(&" ".repeat(pad + 1));
                r.push('|');
            }
            out.push(r);
        }
    }
    Some(out)
}

/// Whether the cursor's line and its neighbours look like a table block
/// (`02-formatting.md` §7: lines containing `|` or `+` adjacent to the
/// cursor's line).
pub fn table_block_at(lines: &[&str], cursor_line: usize) -> Option<(usize, usize)> {
    let is_table_line = |l: &str| l.contains('|') || l.contains('+');
    if !lines.get(cursor_line).is_some_and(|l| is_table_line(l)) {
        return None;
    }
    let mut start = cursor_line;
    while start > 0 && lines.get(start - 1).is_some_and(|l| is_table_line(l)) {
        start -= 1;
    }
    let mut end = cursor_line;
    while end + 1 < lines.len() && lines.get(end + 1).is_some_and(|l| is_table_line(l)) {
        end += 1;
    }
    Some((start, end))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unwrap_detection() {
        assert!(should_unwrap("**", "word", "**", "**"));
        assert!(!should_unwrap("*", "word", "*", "**"));
        assert!(!should_unwrap("**", "word", "", "**"));
        assert!(!should_unwrap("", "word", "**", "**"));
        // Extension requires enough characters on both sides.
        assert!(!should_unwrap("*", "word", "**", "**"));
    }

    #[test]
    fn heading_cycling() {
        assert_eq!(next_heading_level(None), Some(1));
        assert_eq!(next_heading_level(Some(1)), Some(2));
        assert_eq!(next_heading_level(Some(2)), Some(3));
        assert_eq!(next_heading_level(Some(3)), None);
        assert_eq!(next_heading_level(Some(4)), Some(1));
        assert_eq!(next_heading_level(Some(6)), Some(1));
    }

    #[test]
    fn url_detection() {
        assert!(is_url("https://example.com"));
        assert!(is_url("ftp://files.example.org/x"));
        assert!(!is_url("hello world"));
        assert!(!is_url("example.com"));
    }

    #[test]
    fn table_fits_column() {
        let t = build_table(3, 2, 66);
        for line in t.lines() {
            assert!(line.chars().count() <= 66, "line too wide: {line}");
        }
        let lines: Vec<&str> = t.lines().collect();
        // Header + separator + rows-1 data rows.
        assert_eq!(lines.len(), 4);
        assert!(lines[0].starts_with('|'));
        assert!(lines[1].chars().all(|c| c == '-' || c == '|'));
    }

    #[test]
    fn table_cell_cap() {
        // Narrow budget: cells cannot exceed 20 chars.
        let t = build_table(2, 1, 1000);
        assert!(t.lines().next().unwrap().chars().count() <= 24);
    }

    #[test]
    fn fix_table_realigns() {
        let ragged = ["| a | bb |", "| ccc | d |", "| - | -- |"];
        let fixed = fix_table(&ragged).unwrap();
        assert_eq!(fixed[0], "| a   | bb |");
        assert_eq!(fixed[1], "| ccc | d  |");
        assert_eq!(fixed[2], "| --- | -- |");
    }

    #[test]
    fn fix_table_rejects_non_table() {
        assert!(fix_table(&["hello", "world"]).is_none());
        assert!(fix_table(&[]).is_none());
    }

    #[test]
    fn table_block_detection() {
        let lines = ["x", "| a | b |", "| - | - |", "y"];
        assert_eq!(table_block_at(&lines, 0), None);
        assert_eq!(table_block_at(&lines, 1), Some((1, 2)));
        assert_eq!(table_block_at(&lines, 2), Some((1, 2)));
        assert_eq!(table_block_at(&lines, 3), None);
    }
}
