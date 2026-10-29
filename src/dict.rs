//! Minimal DICT protocol client (RFC 2229) used by the dictionary peek:
//! a word is looked up on an online WordNet server (`03-behavior/
//! 11-inline-previews.md` §1.5). Blocking std-network code; callers run it
//! on a background thread. Parsing is separated so it is unit-testable
//! offline.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::time::Duration;

const SERVER: (&str, u16) = ("dict.org", 2626);
const DATABASE: &str = "wn";

/// One word-class section of a WordNet entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Definition {
    /// "noun", "verb", "adjective" or "adverb" (None when unparsable).
    pub word_class: Option<String>,
    /// Numbered definitions in this class.
    pub senses: Vec<String>,
}

/// Look up `word` in the WordNet database. `Ok(vec![])` means "no entry".
pub fn lookup(word: &str) -> Result<Vec<Definition>, String> {
    let word = sanitize(word).ok_or("invalid word")?;
    let stream = TcpStream::connect(SERVER).map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .map_err(|e| e.to_string())?;
    let mut writer = stream.try_clone().map_err(|e| e.to_string())?;
    let mut reader = BufReader::new(stream);

    let mut line = String::new();
    reader.read_line(&mut line).map_err(|e| e.to_string())?;
    if !line.starts_with("220") {
        return Err(format!("server unavailable: {}", line.trim_end()));
    }
    // Greeting is multi-line if it starts with "220-".
    while line.starts_with("220-") {
        line.clear();
        reader.read_line(&mut line).map_err(|e| e.to_string())?;
    }

    write!(
        writer,
        "CLIENT griffel 1.0\r\nDEFINE {DATABASE} {word}\r\n"
    )
    .map_err(|e| e.to_string())?;

    let mut raw = String::new();
    loop {
        line.clear();
        reader.read_line(&mut line).map_err(|e| e.to_string())?;
        if line.is_empty() {
            return Err("connection closed".into());
        }
        // A "151" block is data; everything else is a final status.
        if !line.starts_with("151") {
            if line.starts_with("552") {
                return Ok(Vec::new());
            }
            if line.starts_with("5") || line.starts_with("4") {
                return Err(format!("lookup failed: {}", line.trim_end()));
            }
            if line.starts_with("250") {
                break;
            }
        }
        raw.push_str(&line);
    }
    let _ = writeln!(writer, "QUIT");

    Ok(parse_response(&raw))
}

/// Keep only characters that form a dictionary headword.
fn sanitize(word: &str) -> Option<String> {
    let cleaned: String = word
        .chars()
        .filter(|c| c.is_alphabetic() || *c == '-' || *c == '\'')
        .collect();
    if cleaned.is_empty() || cleaned.len() > 64 {
        None
    } else {
        Some(cleaned)
    }
}

/// Parse the concatenated "151" blocks of a WordNet response into
/// word-class sections. Blocks are separated by "151 ..." header lines;
/// each body ends at a line containing a single "." (already consumed by
/// the reader here, so blocks are delimited by the next "151" line or EOF).
pub fn parse_response(raw: &str) -> Vec<Definition> {
    let mut defs: Vec<Definition> = Vec::new();
    // Split into per-definition bodies on "151" lines.
    let mut bodies: Vec<String> = Vec::new();
    let mut current = String::new();
    for line in raw.lines() {
        if line.starts_with("151") {
            if !current.is_empty() {
                bodies.push(std::mem::take(&mut current));
            }
        } else {
            current.push_str(line);
            current.push('\n');
        }
    }
    if !current.is_empty() {
        bodies.push(current);
    }

    for body in bodies {
        for def in parse_wordnet_body(&body) {
            defs.push(def);
        }
    }
    defs
}

/// Parse one WordNet definition body. Expected shape (dictd + WordNet):
/// a "From WordNet ..." line, the headword, then indented sense lines
/// where each class change starts with `n|v|adj|adv <n>:`.
fn parse_wordnet_body(body: &str) -> Vec<Definition> {
    let mut defs: Vec<Definition> = Vec::new();
    let mut in_senses = false;
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("From WordNet") {
            continue;
        }
        if let Some((class, rest)) = parse_class_line(trimmed) {
            let word_class = Some(expand_class(class).to_owned());
            defs.push(Definition {
                word_class,
                senses: vec![rest.to_owned()],
            });
            in_senses = true;
        } else if in_senses {
            if let Some(rest) = sense_continuation(trimmed) {
                if let Some(last) = defs.last_mut() {
                    last.senses.push(rest.to_owned());
                }
            } else if !defs.is_empty() {
                // Wrapped line of the previous sense.
                if let Some(s) = defs.last_mut().and_then(|d| d.senses.last_mut()) {
                    s.push(' ');
                    s.push_str(trimmed);
                }
            }
        }
    }
    defs
}

/// `n 1: text` / `v 2: text` / `adj 1: text` → (class marker, text).
fn parse_class_line(line: &str) -> Option<(&str, &str)> {
    let (class, rest) = match line.split_once(' ') {
        Some((a, b)) => (a, b),
        None => return None,
    };
    if !matches!(class, "n" | "v" | "adj" | "adv") {
        return None;
    }
    let (_, text) = rest.split_once(':')?;
    Some((class, text.trim()))
}

/// A continuation sense line like `2: text`.
fn sense_continuation(line: &str) -> Option<&str> {
    let (num, text) = line.split_once(':')?;
    if num.trim().chars().all(|c| c.is_ascii_digit()) && !num.trim().is_empty() {
        Some(text.trim())
    } else {
        None
    }
}

fn expand_class(marker: &str) -> &'static str {
    match marker {
        "n" => "noun",
        "v" => "verb",
        "adj" => "adjective",
        _ => "adverb",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_wordnet_sections() {
        let raw = "151 \"test\" wn \"WordNet (r) 3.0 (2006)\"\n\
                   From WordNet (r) 3.0 (2006) [wn]:\n\
                   \n\
                   test\n\
                   \x20   n 1: the act of subjecting to an experimental test\n\
                   \x20   2: a standardized procedure\n\
                   \x20   v 1: put to the test\n\
                   151 \"test\" wn \"WordNet (r) 3.0 (2006)\"\n\
                   From WordNet (r) 3.0 (2006) [wn]:\n\
                   \n\
                   test\n\
                   \x20   adj 1: requiring effort\n";
        let defs = parse_response(raw);
        assert_eq!(defs.len(), 3);
        assert_eq!(defs[0].word_class.as_deref(), Some("noun"));
        assert_eq!(defs[0].senses.len(), 2);
        assert_eq!(defs[0].senses[0], "the act of subjecting to an experimental test");
        assert_eq!(defs[1].word_class.as_deref(), Some("verb"));
        assert_eq!(defs[2].word_class.as_deref(), Some("adjective"));
    }

    #[test]
    fn wrapped_sense_line_joins_previous() {
        let body = "From WordNet (r) 3.0 (2006) [wn]:\n\
                    \n\
                    word\n\
                    \x20   n 1: first part of a very long\n\
                    \x20      sense wrapped over lines\n";
        let defs = parse_wordnet_body(body);
        assert_eq!(defs.len(), 1);
        assert_eq!(
            defs[0].senses[0],
            "first part of a very long sense wrapped over lines"
        );
    }

    #[test]
    fn empty_and_garbage_input() {
        assert!(parse_response("").is_empty());
        assert!(parse_response("150 no definitions").is_empty());
        assert!(sanitize("!!!").is_none());
        assert!(sanitize("he-llo's").is_some());
    }

    #[test]
    fn offline_lookup_rejects_bad_words() {
        // Network-free error path.
        assert!(lookup("!!!").is_err());
    }
}
