//! Auto-pairing decisions (`03-behavior/01-editing.md` §2.1).

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
}
