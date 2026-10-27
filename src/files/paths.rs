//! Path helpers for Markdown links and images
//! (`03-behavior/01-editing.md` §3): relative-when-inside, otherwise
//! absolute; percent-encoded so the Markdown stays valid.

use percent_encoding::{percent_decode_str, utf8_percent_encode, AsciiSet, CONTROLS};

/// Characters left unencoded: unreserved plus the path separator and the
/// characters Markdown commonly tolerates. Everything else (spaces,
/// parentheses, quotes, non-ASCII, `#`, `?`) is percent-encoded.
const PATH_SET: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'(')
    .add(b')')
    .add(b'\'')
    .add(b'#')
    .add(b'%')
    .add(b'?')
    .add(b'\\')
    .add(b'<')
    .add(b'>')
    .add(b'[')
    .add(b']')
    .add(b'{')
    .add(b'}')
    .add(b'|')
    .add(b'^')
    .add(b'`');

/// Percent-encode a path for embedding in Markdown.
pub fn encode_path(path: &std::path::Path) -> String {
    utf8_percent_encode(&path.to_string_lossy(), PATH_SET).to_string()
}

/// Percent-decode a path/URL component for display as a label.
pub fn decode_label(text: &str) -> String {
    percent_decode_str(text).decode_utf8_lossy().into_owned()
}

/// The path to insert for `file` relative to `doc_dir`: relative when the
/// file lies inside the document's folder, otherwise absolute. Both forms
/// percent-encoded.
pub fn link_path(doc_dir: Option<&std::path::Path>, file: &std::path::Path) -> String {
    if let Some(dir) = doc_dir {
        if let Ok(relative) = file.strip_prefix(dir) {
            if !relative.as_os_str().is_empty() {
                return encode_path(relative);
            }
        }
    }
    encode_path(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn relative_inside_document_folder() {
        let doc = Path::new("/home/u/notes");
        assert_eq!(
            link_path(Some(doc), Path::new("/home/u/notes/imgs/pic.png")),
            "imgs/pic.png"
        );
    }

    #[test]
    fn absolute_outside_document_folder() {
        let doc = Path::new("/home/u/notes");
        assert_eq!(
            link_path(Some(doc), Path::new("/elsewhere/pic.png")),
            "/elsewhere/pic.png"
        );
    }

    #[test]
    fn no_document_folder_uses_absolute() {
        assert_eq!(link_path(None, Path::new("/x/p.png")), "/x/p.png");
    }

    #[test]
    fn spaces_and_unicode_are_encoded() {
        let doc = Path::new("/home/u");
        assert_eq!(
            link_path(Some(doc), Path::new("/home/u/my pics/héllo (1).png")),
            "my%20pics/h%C3%A9llo%20%281%29.png"
        );
    }

    #[test]
    fn labels_are_decoded() {
        assert_eq!(decode_label("my%20pics/h%C3%A9llo.png"), "my pics/héllo.png");
        assert_eq!(decode_label("plain.png"), "plain.png");
    }
}
