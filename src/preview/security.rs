//! Preview security model (`03-behavior/03-live-preview.md` §5).

/// Whether the text contains raw HTML element markup (any tag,
/// case-insensitive). Used when the policy is "always ask": files without
/// HTML are previewed restricted automatically.
pub fn contains_raw_html(text: &str) -> bool {
    // Any `<tag`, `</tag`, or comment; autolinks like `<https://…>` do not
    // match because `:`/`/` interrupt the tag-name pattern.
    let re = regex::Regex::new(r"(?i)</?[a-zA-Z][a-zA-Z0-9-]*(\s|>|/>|/)").unwrap();
    // Cheap pre-filter to avoid building/running the regex on plain text.
    if !text.contains('<') {
        return false;
    }
    re.is_match(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_tags() {
        assert!(contains_raw_html("<script>alert(1)</script>"));
        assert!(contains_raw_html("<div class=\"x\">"));
        assert!(contains_raw_html("<br/>"));
        assert!(contains_raw_html("<BR>"));
        assert!(contains_raw_html("a < b\n<span>x</span>"));
    }

    #[test]
    fn ignores_autolinks_and_plain_text() {
        assert!(!contains_raw_html("visit <https://example.com> now"));
        assert!(!contains_raw_html("a < b and c > d"));
        assert!(!contains_raw_html("1 < 2"));
        assert!(!contains_raw_html("plain text with no markup"));
    }

    #[test]
    fn empty_and_new_documents() {
        assert!(!contains_raw_html(""));
    }
}
