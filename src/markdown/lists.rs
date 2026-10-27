//! List marker parsing and transformations
//! (`03-behavior/01-editing.md` §2.2, §2.3).

/// The kind of list marker a line starts with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListKind {
    /// Bulleted with the given bullet character.
    Bullet(char),
    /// Numbered: number and delimiter (`.` or `)`).
    Numbered(u32, char),
    /// Checklist: bullet character plus box state.
    Checklist(char, bool),
}

/// A parsed list marker at the start of a line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListMarker {
    /// Leading whitespace of the line.
    pub indent: String,
    pub kind: ListKind,
    /// Full marker length in chars: indent + marker + one space.
    pub marker_len: usize,
}

impl ListMarker {
    /// The line content after the marker.
    pub fn content<'a>(&self, line: &'a str) -> &'a str {
        line.get(self.marker_len..).unwrap_or("")
    }

    pub fn is_empty_item(&self, line: &str) -> bool {
        self.content(line).trim().is_empty()
    }

    /// The prefix to insert when continuing this list with Enter.
    pub fn continuation_prefix(&self) -> String {
        match self.kind {
            ListKind::Bullet(c) => format!("{}{c} ", self.indent),
            ListKind::Numbered(n, delim) => format!("{}{}{delim} ", self.indent, n + 1),
            ListKind::Checklist(c, _) => format!("{}{c} [ ] ", self.indent),
        }
    }
}

/// Parse the list marker at the start of `line`, if any.
pub fn parse_marker(line: &str) -> Option<ListMarker> {
    let indent_len = line.len() - line.trim_start_matches([' ', '\t']).len();
    let indent = &line[..indent_len];
    let rest = &line[indent_len..];

    let mut chars = rest.chars();
    let bullet = chars.next()?;
    if !(bullet == '-' || bullet == '*' || bullet == '+') {
        return None;
    }
    let after_bullet = chars.as_str();

    // Checklist: `- [ ] ` / `- [x] ` (case-insensitive X) — the bullet is
    // followed by a space, the box, and a space.
    let checklist = after_bullet
        .strip_prefix(' ')
        .and_then(|r| r.strip_prefix('['))
        .and_then(|r| {
            let mut box_chars = r.chars();
            let box_char = box_chars.next()?;
            if box_char != ' ' && box_char != 'x' && box_char != 'X' {
                return None;
            }
            if box_chars.next()? != ']' {
                return None;
            }
            box_chars.as_str().starts_with(' ').then_some(box_char)
        });
    if let Some(box_char) = checklist {
        let marker_len = indent_len
            + 1 // bullet
            + 1 // space
            + 1 // '['
            + box_char.len_utf8()
            + 1 // ']'
            + 1; // space
        return Some(ListMarker {
            indent: indent.to_owned(),
            kind: ListKind::Checklist(bullet, box_char == 'x' || box_char == 'X'),
            marker_len,
        });
    }

    // Bulleted: requires exactly one following space.
    if let Some(_) = after_bullet.strip_prefix(' ') {
        let marker_len = indent_len + 1 + 1;
        return Some(ListMarker {
            indent: indent.to_owned(),
            kind: ListKind::Bullet(bullet),
            marker_len,
        });
    }
    None
}

/// Parse a numbered-list marker (`1.`, `12)`, any indent).
pub fn parse_numbered(line: &str) -> Option<ListMarker> {
    let indent_len = line.len() - line.trim_start_matches([' ', '\t']).len();
    let indent = &line[..indent_len];
    let rest = &line[indent_len..];
    let digits_len = rest.chars().take_while(char::is_ascii_digit).count();
    if digits_len == 0 {
        return None;
    }
    let number: u32 = rest[..digits_len].parse().ok()?;
    let after = &rest[digits_len..];
    let mut chars = after.chars();
    let delim = chars.next()?;
    if delim != '.' && delim != ')' {
        return None;
    }
    if !chars.as_str().starts_with(' ') {
        return None;
    }
    let marker_len = indent_len + digits_len + 1 + 1;
    Some(ListMarker {
        indent: indent.to_owned(),
        kind: ListKind::Numbered(number, delim),
        marker_len,
    })
}

/// Parse any list marker (bulleted, checklist or numbered).
pub fn parse_any(line: &str) -> Option<ListMarker> {
    parse_marker(line).or_else(|| parse_numbered(line))
}

/// Next bullet in the `-` → `*` → `+` cycle.
pub fn cycle_bullet(c: char) -> char {
    match c {
        '-' => '*',
        '*' => '+',
        _ => '-',
    }
}

/// Result of pressing Enter at the end of a line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Continuation {
    /// The line is an empty list item: the marker line is removed (list
    /// exited).
    ExitList,
    /// This prefix text is inserted after a newline.
    Prefix(String),
}

/// What Enter should do at the end of `line` (§2.2). `None` for a plain
/// newline.
pub fn continuation(line: &str) -> Option<Continuation> {
    let marker = parse_any(line)?;
    if marker.is_empty_item(line) {
        Some(Continuation::ExitList)
    } else {
        Some(Continuation::Prefix(marker.continuation_prefix()))
    }
}

/// Result of pressing Tab on an empty list item (§2.3): the line is
/// rewritten one indent level deeper.
pub fn indent_empty_item(line: &str) -> Option<String> {
    let marker = parse_any(line)?;
    if !marker.is_empty_item(line) {
        return None;
    }
    let new_marker = match &marker.kind {
        ListKind::Bullet(c) => {
            let c = cycle_bullet(*c);
            format!("{c} ")
        }
        ListKind::Numbered(_, delim) => format!("1{delim} "),
        ListKind::Checklist(c, _) => format!("{c} [ ] "),
    };
    Some(format!("{}\t{new_marker}", marker.indent))
}

/// What Shift+Tab should do to `line` with the cursor at `cursor_col`
/// (§2.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outdent {
    /// Replace the line with this text and move the cursor to this column.
    Rewritten(String, usize),
    /// Remove one tab before the cursor.
    RemoveTabBefore,
    /// Nothing to remove.
    Noop,
}

/// Compute the Shift+Tab behavior. `previous_lines` are the lines above the
/// current one (nearest first), searched up to 50 lines for a list line
/// with less indentation whose bullet is adopted.
pub fn outdent(line: &str, cursor_col: usize, previous_lines: &[&str]) -> Outdent {
    if let Some(marker) = parse_any(line) {
        let Some(new_indent) = remove_one_level(&marker.indent) else {
            // No indentation to remove: fall through to the generic case.
            return generic_outdent(line, cursor_col);
        };
        let rest = &line[marker.indent.len()..];
        let new_bullet = match &marker.kind {
            ListKind::Bullet(_) | ListKind::Checklist(_, _) => {
                adopt_bullet(&marker.indent, previous_lines).unwrap_or(match &marker.kind {
                    ListKind::Checklist(c, _) => *c,
                    _ => rest.chars().next().unwrap_or('-'),
                })
            }
            ListKind::Numbered(_, _) => '-',
        };
        let new_rest = match &marker.kind {
            ListKind::Bullet(c) => rest.replacen(*c, new_bullet.to_string().as_str(), 1),
            ListKind::Checklist(c, _) => rest.replacen(*c, new_bullet.to_string().as_str(), 1),
            ListKind::Numbered(_, _) => rest.to_owned(),
        };
        let new_line = format!("{new_indent}{new_rest}");
        let delta = marker.indent.chars().count() - new_indent.chars().count();
        let new_col = cursor_col.saturating_sub(delta).max(new_indent.chars().count());
        Outdent::Rewritten(new_line, new_col)
    } else {
        generic_outdent(line, cursor_col)
    }
}

fn generic_outdent(line: &str, cursor_col: usize) -> Outdent {
    // One tab is removed from the current position, if present.
    let col_bytes = byte_col(line, cursor_col);
    if col_bytes > 0 && line.as_bytes().get(col_bytes - 1) == Some(&b'\t') {
        Outdent::RemoveTabBefore
    } else {
        Outdent::Noop
    }
}

fn byte_col(line: &str, col: usize) -> usize {
    line.char_indices().nth(col).map(|(i, _)| i).unwrap_or(line.len())
}

/// Adopt the bullet of the nearest preceding list line with less
/// indentation (searching up to 50 lines back).
fn adopt_bullet(current_indent: &str, previous_lines: &[&str]) -> Option<char> {
    for prev in previous_lines.iter().take(50) {
        if let Some(m) = parse_any(prev) {
            if m.indent.len() < current_indent.len() {
                return match m.kind {
                    ListKind::Bullet(c) | ListKind::Checklist(c, _) => Some(c),
                    ListKind::Numbered(_, _) => None,
                };
            }
        }
    }
    None
}

/// Remove one indent level from leading whitespace: one tab, or up to four
/// leading spaces.
fn remove_one_level(indent: &str) -> Option<String> {
    if indent.starts_with('\t') {
        Some(indent[1..].to_owned())
    } else if indent.starts_with(' ') {
        let spaces = indent.chars().take_while(|c| *c == ' ').count();
        if spaces == 0 {
            None
        } else {
            Some(indent[spaces.min(4)..].to_owned())
        }
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_bullets() {
        let m = parse_marker("- item").unwrap();
        assert_eq!(m.kind, ListKind::Bullet('-'));
        assert_eq!(m.marker_len, 2);
        assert_eq!(m.content("- item"), "item");

        let m = parse_marker("  * item").unwrap();
        assert_eq!(m.kind, ListKind::Bullet('*'));
        assert_eq!(m.indent, "  ");

        let m = parse_marker("+ x").unwrap();
        assert_eq!(m.kind, ListKind::Bullet('+'));

        assert!(parse_marker("-no space").is_none());
        assert!(parse_marker("hello").is_none());
        assert!(parse_marker("").is_none());
    }

    #[test]
    fn parse_numbered() {
        let m = parse_any("2. item").unwrap();
        assert_eq!(m.kind, ListKind::Numbered(2, '.'));
        assert_eq!(m.marker_len, 3);

        let m = parse_any("12) item").unwrap();
        assert_eq!(m.kind, ListKind::Numbered(12, ')'));

        assert!(parse_any("2.item").is_none());
        assert!(parse_any("2 item").is_none());
    }

    #[test]
    fn parse_checklist() {
        let m = parse_any("- [x] done").unwrap();
        assert_eq!(m.kind, ListKind::Checklist('-', true));
        assert_eq!(m.marker_len, 6);

        let m = parse_any("* [ ] todo").unwrap();
        assert_eq!(m.kind, ListKind::Checklist('*', false));

        // "[X]" uppercase also counts as checked.
        let m = parse_any("- [X] done").unwrap();
        assert_eq!(m.kind, ListKind::Checklist('-', true));

        // Without the trailing space it is a plain bullet, not a checklist.
        let m = parse_any("- [x]no").unwrap();
        assert_eq!(m.kind, ListKind::Bullet('-'));
    }

    #[test]
    fn continuation_bullet() {
        assert_eq!(
            continuation("- item"),
            Some(Continuation::Prefix("- ".into()))
        );
        assert_eq!(
            continuation("  * item"),
            Some(Continuation::Prefix("  * ".into()))
        );
    }

    #[test]
    fn continuation_number_increments() {
        assert_eq!(
            continuation("2. item"),
            Some(Continuation::Prefix("3. ".into()))
        );
        assert_eq!(
            continuation("12) item"),
            Some(Continuation::Prefix("13) ".into()))
        );
    }

    #[test]
    fn continuation_checklist_resets() {
        assert_eq!(
            continuation("- [x] done"),
            Some(Continuation::Prefix("- [ ] ".into()))
        );
    }

    #[test]
    fn continuation_exits_empty_item() {
        assert_eq!(continuation("- "), Some(Continuation::ExitList));
        assert_eq!(continuation("  3.  "), Some(Continuation::ExitList));
        assert_eq!(continuation("-   \t"), Some(Continuation::ExitList));
    }

    #[test]
    fn continuation_plain_lines() {
        assert_eq!(continuation("plain text"), None);
        assert_eq!(continuation(""), None);
    }

    #[test]
    fn tab_indents_and_cycles_bullet() {
        assert_eq!(indent_empty_item("- ").unwrap(), "\t* ");
        assert_eq!(indent_empty_item("* ").unwrap(), "\t+ ");
        assert_eq!(indent_empty_item("+ ").unwrap(), "\t- ");
        assert_eq!(indent_empty_item("  - ").unwrap(), "  \t* ");
        assert_eq!(indent_empty_item("2. ").unwrap(), "\t1. ");
        assert_eq!(indent_empty_item("3) ").unwrap(), "\t1) ");
        assert_eq!(indent_empty_item("- [ ] ").unwrap(), "\t- [ ] ");
        // Non-empty item: no magic.
        assert_eq!(indent_empty_item("- item"), None);
    }

    #[test]
    fn outdent_bullet_adopts_previous_bullet() {
        // Previous line at a shallower level has `+`; outdenting adopts it.
        let prev = ["+ parent"];
        assert_eq!(
            outdent("\t* child", 3, &prev),
            Outdent::Rewritten("+ child".into(), 2)
        );
    }


    #[test]
    fn outdent_searches_up_to_50_lines() {
        let mut prev: Vec<&str> = vec!["plain"; 40];
        prev.push("+ target");
        prev.push("plain");
        assert_eq!(
            outdent("    - deep", 6, &prev),
            Outdent::Rewritten("+ deep".into(), 2)
        );
    }

    #[test]
    fn outdent_numbered_keeps_delimiter() {
        assert_eq!(
            outdent("\t3. item", 4, &[]),
            Outdent::Rewritten("3. item".into(), 3)
        );
        assert_eq!(
            outdent("\t3) item", 4, &[]),
            Outdent::Rewritten("3) item".into(), 3)
        );
    }

    #[test]
    fn outdent_generic_removes_tab() {
        assert_eq!(outdent("\ttext", 1, &[]), Outdent::RemoveTabBefore);
        assert_eq!(outdent("text", 0, &[]), Outdent::Noop);
        assert_eq!(outdent("  text", 2, &[]), Outdent::Noop);
    }

    #[test]
    fn outdent_no_indent_noop() {
        // A bulleted item at column 0 has no indent level to remove.
        assert_eq!(outdent("- item", 2, &[]), Outdent::Noop);
    }
}
