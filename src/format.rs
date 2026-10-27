//! Formatting command implementations over the text buffer
//! (`03-behavior/02-formatting.md`). Pure decision logic lives in
//! `markdown/formatting.rs`.

use gtk::prelude::*;
use gtk::gio;
use std::path::PathBuf;

use crate::markdown::{formatting as pure, lists};

/// Insert `text` at the cursor as one undoable step.
fn insert(buffer: &sourceview5::Buffer, text: &str) {
    buffer.begin_user_action();
    buffer.insert_at_cursor(text);
    buffer.end_user_action();
}

fn selection(buffer: &sourceview5::Buffer) -> Option<(gtk::TextIter, gtk::TextIter, String)> {
    let (s, e) = buffer.selection_bounds()?;
    let text = buffer.text(&s, &e, true).into();
    Some((s, e, text))
}

/// Select [start, end) via char offsets from `anchor`.
fn select_range(buffer: &sourceview5::Buffer, anchor: &gtk::TextIter, from: i32, to: i32) {
    let start = buffer.iter_at_offset(anchor.offset() + from);
    let end = buffer.iter_at_offset(anchor.offset() + to);
    buffer.select_range(&start, &end);
}

// ---------------------------------------------------------------------------
// Wrap commands
// ---------------------------------------------------------------------------

/// Bold / italic / strikethrough: wrap the selection or insert
/// `delimiter + placeholder + delimiter` with the placeholder selected;
/// toggles off when the selection already carries the delimiters.
pub fn wrap(buffer: &sourceview5::Buffer, delim: &str, placeholder: &str) {
    buffer.begin_user_action();

    if let Some((mut s, mut e, sel_text)) = selection(buffer) {
        let n = delim.chars().count() as i32;
        // Extend by one delimiter on each side when the buffer allows.
        let before = {
            let mut b = s.clone();
            for _ in 0..n {
                if !b.backward_char() {
                    break;
                }
            }
            buffer.text(&b, &s, true).to_string()
        };
        let after = {
            let mut a = e.clone();
            for _ in 0..n {
                if !a.forward_char() {
                    break;
                }
            }
            buffer.text(&e, &a, true).to_string()
        };

        if pure::should_unwrap(&before, &sel_text, &after, delim) {
            // Remove the delimiters.
            s.backward_chars(n);
            e.forward_chars(n);
            buffer.delete(&mut s, &mut e);
            // Select the inner text where the selection now sits.
            let insert = buffer.iter_at_mark(&buffer.get_insert());
            select_range(buffer, &insert, -(sel_text.chars().count() as i32), 0);
            buffer.end_user_action();
            return;
        }

        // Keep surrounding whitespace outside the delimiters.
        let trimmed_start = sel_text.len() - sel_text.trim_start().len();
        let trimmed_end = sel_text.len() - sel_text.trim_end().len();
        let mut start = s;
        start.forward_chars(trimmed_start as i32);
        let mut end = e;
        end.backward_chars(trimmed_end as i32);
        if start.offset() > end.offset() {
            // Whitespace-only selection: treat as no selection.
            buffer.place_cursor(&start);
            insert(buffer, &format!("{delim}{placeholder}{delim}"));
            let insert = buffer.iter_at_mark(&buffer.get_insert());
            select_range(buffer, &insert, -(delim.chars().count() as i32) - placeholder.chars().count() as i32, -(delim.chars().count() as i32));
            buffer.end_user_action();
            return;
        }
        buffer.insert(&mut end, delim);
        buffer.insert(&mut start, delim);
        let inner_chars = buffer
            .text(&start, &end, true)
            .chars()
            .count() as i32;
        buffer.select_range(&start, &buffer.iter_at_offset(start.offset() + inner_chars));
        buffer.end_user_action();
        return;
    }

    insert(buffer, &format!("{delim}{placeholder}{delim}"));
    let insert_iter = buffer.iter_at_mark(&buffer.get_insert());
    let n = delim.chars().count() as i32;
    select_range(
        buffer,
        &insert_iter,
        -n - placeholder.chars().count() as i32,
        -n,
    );
    buffer.end_user_action();
}

/// Code command: a fenced block on an empty line, otherwise inline ticks.
pub fn code(buffer: &sourceview5::Buffer) {
    let cursor_iter = buffer.iter_at_mark(&buffer.get_insert());
    let line_no = cursor_iter.line();
    let line = crate::editor::input::line_text_pub(buffer, line_no);

    if line.trim().is_empty() {
        insert(buffer, "```\ncode\n```");
        let cursor = buffer.iter_at_mark(&buffer.get_insert());
        select_range(
            buffer,
            &cursor,
            -("code\n```".chars().count() as i32),
            -("```".chars().count() as i32),
        );
    } else {
        wrap(buffer, "`", "code");
    }
}

// ---------------------------------------------------------------------------
// Heading command
// ---------------------------------------------------------------------------

fn heading_level_of(line: &str) -> Option<u8> {
    let hashes = line.chars().take_while(|c| *c == '#').count();
    if (1..=6).contains(&hashes)
        && (line.len() == hashes || line[hashes..].starts_with(' ') || line[hashes..].starts_with('\t'))
    {
        Some(hashes as u8)
    } else {
        None
    }
}

/// The heading command. `level` selects exact-level mode (Ctrl+1..6,
/// toggling off at the same level) or cycles when None.
pub fn heading(buffer: &sourceview5::Buffer, level: Option<u8>) {
    let cursor_iter = buffer.iter_at_mark(&buffer.get_insert());
    let line_no = cursor_iter.line();
    let line = crate::editor::input::line_text_pub(buffer, line_no);
    let current = heading_level_of(&line);

    let target = match level {
        Some(n) => {
            if current == Some(n) {
                None
            } else {
                Some(n)
            }
        }
        None => pure::next_heading_level(current),
    };

    buffer.begin_user_action();

    match (current, target) {
        (Some(cur), Some(new)) if cur != new => {
            // Rewrite the marker.
            let rest = line[(cur as usize)..].trim_start().to_owned();
            rewrite_line(buffer, line_no, &format!("{}{} {rest}", "#".repeat(new as usize), ""));
            place_at_line_end(buffer, line_no);
        }
        (Some(cur), None) => {
            // Remove the heading marker.
            let rest = line[(cur as usize)..].trim_start().to_owned();
            rewrite_line(buffer, line_no, &rest);
            place_at_line_end(buffer, line_no);
        }
        (None, Some(new)) => {
            let marker = format!("{} ", "#".repeat(new as usize));
            let prev_has_content = line_no > 0
                && !crate::editor::input::line_text_pub(buffer, line_no - 1)
                    .trim()
                    .is_empty();
            if line.trim().is_empty() && cursor_iter.starts_line() {
                // Empty line: marker plus placeholder, selected.
                insert(buffer, &format!("{marker}Header"));
                let cursor = buffer.iter_at_mark(&buffer.get_insert());
                select_range(buffer, &cursor, -(("Header".chars().count()) as i32), 0);
            } else if cursor_iter.starts_line() {
                // Prefix the line; blank line first when the previous line
                // has content.
                let mut start = buffer.iter_at_line(line_no).unwrap_or_else(|| buffer.start_iter());
                if prev_has_content {
                    buffer.insert(&mut start, "\n");
                    let mut start = buffer.iter_at_line(line_no + 1).unwrap_or_else(|| buffer.start_iter());
                    buffer.insert(&mut start, &marker);
                } else {
                    buffer.insert(&mut start, &marker);
                }
                place_at_line_end(buffer, line_no);
            } else {
                // Mid-line: heading on a new line below, blank line between.
                insert(buffer, &format!("\n\n{marker}Header"));
                let cursor = buffer.iter_at_mark(&buffer.get_insert());
                select_range(buffer, &cursor, -(("Header".chars().count()) as i32), 0);
            }
        }
        (Some(_), Some(_)) => unreachable!(),
        (None, None) => {}
    }

    buffer.end_user_action();
}

fn rewrite_line(buffer: &sourceview5::Buffer, line_no: i32, new_text: &str) {
    let Some(mut start) = buffer.iter_at_line(line_no) else {
        return;
    };
    let mut end = start;
    end.forward_to_line_end();
    buffer.delete(&mut start, &mut end);
    buffer.insert(&mut start, new_text);
}

fn place_at_line_end(buffer: &sourceview5::Buffer, line_no: i32) {
    if let Some(mut iter) = buffer.iter_at_line(line_no) {
        iter.forward_to_line_end();
        buffer.place_cursor(&iter);
    }
}

// ---------------------------------------------------------------------------
// List commands
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
pub enum ListCommand {
    Bullet,
    Numbered,
    Checklist,
}

pub fn list(buffer: &sourceview5::Buffer, kind: ListCommand) {
    let cursor_iter = buffer.iter_at_mark(&buffer.get_insert());
    let line_no = cursor_iter.line();
    let line = crate::editor::input::line_text_pub(buffer, line_no);
    let at_start = cursor_iter.starts_line();

    // Toggle when the line already carries this kind of marker.
    let marker = lists::parse_any(&line);
    let same_kind = marker.as_ref().is_some_and(|m| {
        matches!(
            (&m.kind, kind),
            (lists::ListKind::Bullet(_), ListCommand::Bullet)
                | (lists::ListKind::Numbered(_, _), ListCommand::Numbered)
                | (lists::ListKind::Checklist(_, _), ListCommand::Checklist)
        )
    });

    if same_kind {
        if let Some(m) = marker {
            buffer.begin_user_action();
            match m.kind {
                lists::ListKind::Checklist(bullet, checked) => {
                    // Flip the checkbox state instead of removing.
                    let box_str = if checked { "[ ]" } else { "[x]" };
                    let new_line = format!(
                        "{}{} {} {}",
                        m.indent,
                        bullet,
                        box_str,
                        m.content(&line)
                    );
                    rewrite_line(buffer, line_no, &new_line);
                }
                _ => {
                    let rest = m.content(&line).to_owned();
                    rewrite_line(buffer, line_no, &rest);
                }
            }
            buffer.end_user_action();
            return;
        }
    }

    // Continuation: inherit from the previous line when it is an item of
    // the same kind.
    let prev = if line_no > 0 {
        crate::editor::input::line_text_pub(buffer, line_no - 1)
    } else {
        String::new()
    };
    let inherited: Option<String> = (|| match kind {
        ListCommand::Bullet => lists::parse_any(&prev).and_then(|m| match m.kind {
            lists::ListKind::Bullet(c) | lists::ListKind::Checklist(c, _) => {
                Some(format!("{}{c} ", m.indent))
            }
            _ => None,
        }),
        ListCommand::Numbered => lists::parse_any(&prev).and_then(|m| match m.kind {
            lists::ListKind::Numbered(n, d) => Some(format!("{}{}{d} ", m.indent, n + 1)),
            _ => None,
        }),
        ListCommand::Checklist => lists::parse_any(&prev).and_then(|m| match m.kind {
            lists::ListKind::Bullet(c) | lists::ListKind::Checklist(c, _) => {
                Some(format!("{}{c} [ ] ", m.indent))
            }
            _ => None,
        }),
    })();
    let default_marker = match kind {
        ListCommand::Bullet => "- ".to_owned(),
        ListCommand::Numbered => "1. ".to_owned(),
        ListCommand::Checklist => "- [ ] ".to_owned(),
    };
    let marker_text = inherited.unwrap_or(default_marker);

    if line.trim().is_empty() && at_start {
        insert(buffer, &format!("{marker_text}Item"));
        let cursor = buffer.iter_at_mark(&buffer.get_insert());
        select_range(buffer, &cursor, -(("Item".chars().count()) as i32), 0);
        return;
    }
    if at_start {
        // Prefix the current line.
        let mut start = buffer.iter_at_line(line_no).unwrap_or_else(|| buffer.start_iter());
        buffer.begin_user_action();
        buffer.insert(&mut start, &marker_text);
        buffer.end_user_action();
        return;
    }
    // Mid-line: blank line, then the marker on the next line.
    insert(buffer, &format!("\n\n{marker_text}Item"));
    let cursor = buffer.iter_at_mark(&buffer.get_insert());
    select_range(buffer, &cursor, -(("Item".chars().count()) as i32), 0);
}

// ---------------------------------------------------------------------------
// Block quote
// ---------------------------------------------------------------------------

pub fn block_quote(buffer: &sourceview5::Buffer) {
    let cursor_iter = buffer.iter_at_mark(&buffer.get_insert());
    let line_no = cursor_iter.line();
    let line = crate::editor::input::line_text_pub(buffer, line_no);

    if cursor_iter.starts_line() {
        if let Some(rest) = line.strip_prefix("> ").or_else(|| line.strip_prefix('>')) {
            // Toggle off.
            let rest = rest.strip_prefix(' ').unwrap_or(rest).to_owned();
            buffer.begin_user_action();
            rewrite_line(buffer, line_no, &rest);
            buffer.end_user_action();
            return;
        }
        let prev_has_content = line_no > 0
            && !crate::editor::input::line_text_pub(buffer, line_no - 1)
                .trim()
                .is_empty();
        buffer.begin_user_action();
        let mut start = buffer.iter_at_line(line_no).unwrap_or_else(|| buffer.start_iter());
        if prev_has_content {
            buffer.insert(&mut start, "\n");
            let mut start = buffer.iter_at_line(line_no + 1).unwrap_or_else(|| buffer.start_iter());
            buffer.insert(&mut start, "> ");
        } else {
            buffer.insert(&mut start, "> ");
        }
        buffer.end_user_action();
        return;
    }
    if line.trim().is_empty() {
        insert(buffer, "> Quote");
        let cursor = buffer.iter_at_mark(&buffer.get_insert());
        select_range(buffer, &cursor, -(("Quote".chars().count()) as i32), 0);
        return;
    }
    insert(buffer, "\n\n> Quote");
    let cursor = buffer.iter_at_mark(&buffer.get_insert());
    select_range(buffer, &cursor, -(("Quote".chars().count()) as i32), 0);
}

// ---------------------------------------------------------------------------
// Link and image
// ---------------------------------------------------------------------------

const URL_PLACEHOLDER: &str = "https://www.example.com";

pub fn link(buffer: &sourceview5::Buffer) {
    match selection(buffer) {
        None => {
            let text = format!("[link text]({URL_PLACEHOLDER})");
            insert(buffer, &text);
            let cursor = buffer.iter_at_mark(&buffer.get_insert());
            select_range(buffer, &cursor, -(URL_PLACEHOLDER.chars().count() as i32), 0);
        }
        Some((s, e, sel_text)) => {
            if pure::is_url(&sel_text) {
                buffer.begin_user_action();
                let text = format!("[{sel_text}]({sel_text})");
                let mut s2 = s;
                let mut e2 = e;
                buffer.delete(&mut s2, &mut e2);
                buffer.insert(&mut s2, &text);
                let label_len = sel_text.chars().count() as i32;
                buffer.select_range(&s2, &buffer.iter_at_offset(s2.offset() + label_len));
                buffer.end_user_action();
            } else {
                buffer.begin_user_action();
                let text = format!("[{sel_text}]({URL_PLACEHOLDER})");
                let mut s3 = s;
                let mut e3 = e;
                buffer.delete(&mut s3, &mut e3);
                buffer.insert(&mut s3, &text);
                let start = s3.offset() + 1 + sel_text.chars().count() as i32 + 2;
                buffer.select_range(
                    &buffer.iter_at_offset(start),
                    &buffer.iter_at_offset(start + URL_PLACEHOLDER.chars().count() as i32),
                );
                buffer.end_user_action();
            }
        }
    }
}

/// Image command: file chooser, then `![caption](path)` with the caption
/// selected (`02-formatting.md` §6).
pub fn image(buffer: &sourceview5::Buffer, doc_dir: Option<PathBuf>, parent: &impl IsA<gtk::Window>) {
    let dialog = gtk::FileDialog::new();
    dialog.set_title("Insert Image");

    let images = gtk::FileFilter::new();
    images.set_name(Some("Images"));
    for mime in ["image/png", "image/jpeg", "image/gif", "image/webp", "image/svg+xml"] {
        images.add_mime_type(mime);
    }
    let filters = gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&images);
    dialog.set_filters(Some(&filters));
    if let Some(dir) = doc_dir.clone() {
        dialog.set_initial_folder(Some(&gio::File::for_path(dir)));
    }

    let buffer = buffer.clone();
    dialog.open(
        Some(parent),
        gio::Cancellable::NONE,
        move |result| {
            let Ok(file) = result else {
                return; // Cancelled: focus returns to the editor.
            };
            let Some(path) = file.path() else {
                return;
            };
            let caption = selection(&buffer)
                .map(|(_, _, t)| t.trim().to_owned())
                .filter(|t| !t.is_empty())
                .unwrap_or_else(|| "image caption".into());
            let link = crate::files::paths::link_path(doc_dir.as_deref(), &path);
            let text = format!("![{caption}]({link})");
            buffer.begin_user_action();
            buffer.insert_at_cursor(&text);
            let cursor = buffer.iter_at_mark(&buffer.get_insert());
            let total = text.chars().count() as i32;
            buffer.select_range(
                &buffer.iter_at_offset(cursor.offset() - total + 2),
                &buffer.iter_at_offset(cursor.offset() - total + 2 + caption.chars().count() as i32),
            );
            buffer.end_user_action();
        },
    );
}

// ---------------------------------------------------------------------------
// Horizontal rule and tables
// ---------------------------------------------------------------------------

pub fn horizontal_rule(buffer: &sourceview5::Buffer) {
    let cursor_iter = buffer.iter_at_mark(&buffer.get_insert());
    let text = if cursor_iter.starts_line() {
        "---\n\n"
    } else {
        "\n---\n\n"
    };
    insert(buffer, text);
}

pub fn insert_table(buffer: &sourceview5::Buffer, rows: usize, cols: usize, chars_per_line: i32) {
    let at_cursor = buffer.iter_at_mark(&buffer.get_insert());
    let prefix = if at_cursor.starts_line() { "" } else { "\n" };
    let text = pure::build_table(rows, cols, chars_per_line.max(20) as usize);
    insert(buffer, &format!("{prefix}{text}\n"));
    // Place the cursor in the first header cell.
    let cursor = buffer.iter_at_mark(&buffer.get_insert());
    let back = (text.lines().next().unwrap_or("").chars().count() + 1) as i32;
    buffer.place_cursor(&buffer.iter_at_offset(cursor.offset() - back + 2));
}

/// Re-align the table surrounding the cursor (`02-formatting.md` §7).
/// Returns false when the cursor is not inside a table.
pub fn fix_table(buffer: &sourceview5::Buffer) -> bool {
    let text: String = buffer
        .text(&buffer.start_iter(), &buffer.end_iter(), true)
        .into();
    let lines: Vec<&str> = text.split('\n').collect();
    let insert = buffer.iter_at_mark(&buffer.get_insert());
    let cursor_line = insert.line() as usize;
    let Some((start, end)) = pure::table_block_at(&lines, cursor_line) else {
        return false;
    };
    let Some(fixed) = pure::fix_table(&lines[start..=end]) else {
        return false;
    };
    buffer.begin_user_action();
    let mut from = buffer.iter_at_line(start as i32).unwrap_or_else(|| buffer.start_iter());
    let mut to = buffer
        .iter_at_line(end as i32 + 1)
        .unwrap_or_else(|| buffer.end_iter());
    buffer.delete(&mut from, &mut to);
    buffer.insert(&mut from, &format!("{}\n", fixed.join("\n")));
    buffer.end_user_action();
    true
}
