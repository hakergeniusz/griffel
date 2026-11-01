//! Auto-pairing decisions (`03-behavior/01-editing.md` §2.1). The
//! markdown set pairs conservatively (before whitespace or end of line
//! only); the Code Mode set pairs like code editors (VS Code style).

/// The closing delimiter for an opening one, if `c` is one of the
/// auto-paired openers: `(`, `[`, `{`, `"`, `<`.
pub fn closing_for(c: char) -> Option<char> {
    match c {
        '(' => Some(')'),
        '[' => Some(']'),
        '{' => Some('}'),
        '"' => Some('"'),
        '<' => Some('>'),
        _ => None,
    }
}

/// Whether `c` is a closing delimiter that can skip over an existing
/// partner: `)`, `]`, `}`, `"`, `>`.
pub fn is_closing(c: char) -> bool {
    matches!(c, ')' | ']' | '}' | '"' | '>')
}

/// The closing delimiter for an opening one in Code Mode: brackets,
/// double/single quotes and the backtick. `<` is excluded — in code it is
/// a comparison, not a construct opener.
pub fn code_closing_for(c: char) -> Option<char> {
    match c {
        '(' => Some(')'),
        '[' => Some(']'),
        '{' => Some('}'),
        '"' => Some('"'),
        '\'' => Some('\''),
        '`' => Some('`'),
        _ => None,
    }
}

/// Whether `c` is a Code Mode closing delimiter that can skip over an
/// existing partner.
pub fn code_is_closing(c: char) -> bool {
    matches!(c, ')' | ']' | '}' | '"' | '\'' | '`')
}

/// Whether typing the Code Mode opener `opener` should insert the pair
/// before `next` (None = end of line). Brackets pair everywhere except
/// before a word character; quotes and the backtick only before
/// whitespace, a closing delimiter, or end of line.
pub fn code_should_pair(next: Option<char>, opener: char) -> bool {
    let is_quote = matches!(opener, '"' | '\'' | '`');
    match next {
        None => true,
        Some(n) if n.is_whitespace() => true,
        Some(n) if code_is_closing(n) => true,
        Some(n) => !is_quote && !n.is_alphanumeric(),
    }
}

/// Whether backspace with the cursor between an empty auto-paired pair
/// (`()` and friends) should delete both halves at once.
pub fn code_empty_pair(before: char, after: char) -> bool {
    code_closing_for(before) == Some(after)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairs() {
        assert_eq!(closing_for('('), Some(')'));
        assert_eq!(closing_for('['), Some(']'));
        assert_eq!(closing_for('{'), Some('}'));
        assert_eq!(closing_for('"'), Some('"'));
        assert_eq!(closing_for('<'), Some('>'));
        assert_eq!(closing_for('a'), None);
        assert_eq!(closing_for(')'), None);
    }

    #[test]
    fn closers() {
        for c in [')', ']', '}', '"', '>'] {
            assert!(is_closing(c));
        }
        assert!(!is_closing('('));
        assert!(!is_closing('a'));
    }

    #[test]
    fn code_pairs() {
        assert_eq!(code_closing_for('('), Some(')'));
        assert_eq!(code_closing_for('\''), Some('\''));
        assert_eq!(code_closing_for('`'), Some('`'));
        assert_eq!(code_closing_for('<'), None);
        assert_eq!(code_closing_for('a'), None);
        for c in [')', ']', '}', '"', '\'', '`'] {
            assert!(code_is_closing(c));
        }
        assert!(!code_is_closing('>'));
        assert!(!code_is_closing('('));
    }

    #[test]
    fn code_pair_placement() {
        // Brackets pair before punctuation but not before words.
        assert!(code_should_pair(None, '('));
        assert!(code_should_pair(Some(' '), '('));
        assert!(code_should_pair(Some(')'), '('));
        assert!(code_should_pair(Some(','), '('));
        assert!(!code_should_pair(Some('a'), '('));
        // Quotes only before whitespace, closers, or end of line.
        assert!(code_should_pair(None, '"'));
        assert!(code_should_pair(Some(' '), '"'));
        assert!(code_should_pair(Some(')'), '"'));
        assert!(!code_should_pair(Some('a'), '"'));
        assert!(!code_should_pair(Some('t'), '\'')); // don't
        assert!(!code_should_pair(Some(','), '`'));
    }

    #[test]
    fn empty_pairs() {
        assert!(code_empty_pair('(', ')'));
        assert!(code_empty_pair('`', '`'));
        assert!(!code_empty_pair('(', ']'));
        assert!(!code_empty_pair('a', ')'));
    }
}
