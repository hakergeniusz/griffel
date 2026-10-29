//! Per-file extended metadata (`05-file-formats-and-persistence.md` §4):
//! the spellcheck language is stored as GVFS metadata on the document file
//! and re-applied on load. Unsaved documents keep the choice for the
//! session only (`03-behavior/01-editing.md` §4).

use gio::prelude::*;
use gtk::gio;

/// GVFS metadata key (the `metadata::` namespace is handled by gvfsd).
const KEY: &str = "metadata::griffel-spell-language";

/// The spellcheck language stored on `file`, if any. An empty stored value
/// counts as unset.
pub fn load_language(file: &gio::File) -> Option<String> {
    let info = file
        .query_info(KEY, gio::FileQueryInfoFlags::NONE, gio::Cancellable::NONE)
        .ok()?;
    let value = info.attribute_as_string(KEY)?;
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_owned())
    }
}

/// Store `lang` on `file`. Remote files have no metadata (open question 9:
/// skip silently); failures are ignored — the app works without persistence.
pub fn store_language(file: &gio::File, lang: &str) {
    let _ = file.set_attribute_string(
        KEY,
        lang,
        gio::FileQueryInfoFlags::NONE,
        gio::Cancellable::NONE,
    );
}

#[cfg(test)]
mod tests {
    // GVFS metadata needs a session bus + gvfsd; only the key format is
    // checked statically here.
    #[test]
    fn key_is_in_metadata_namespace() {
        assert!(super::KEY.starts_with("metadata::"));
        assert!(!super::KEY.contains(' '));
    }
}
