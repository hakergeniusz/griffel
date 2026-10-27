//! Spell checking via the system `enchant-2` library (LGPL, dynamically
//! linked). The GTK bindings expose no spell checker, so this module talks
//! to enchant directly and the editor applies underline tags itself.

use std::collections::HashMap;
use std::ffi::{c_char, c_int, CString};
use std::sync::Mutex;

#[repr(C)]
struct EnchantBroker {
    _private: [u8; 0],
}
#[repr(C)]
struct EnchantDict {
    _private: [u8; 0],
}

unsafe extern "C" {
    fn enchant_broker_init() -> *mut EnchantBroker;
    fn enchant_broker_free(broker: *mut EnchantBroker);
    fn enchant_broker_request_dict(
        broker: *mut EnchantBroker,
        tag: *const c_char,
    ) -> *mut EnchantDict;
    fn enchant_broker_free_dict(broker: *mut EnchantBroker, dict: *mut EnchantDict);
    fn enchant_dict_check(dict: *mut EnchantDict, word: *const c_char, len: usize) -> c_int;
    fn enchant_dict_suggest(
        dict: *mut EnchantDict,
        word: *const c_char,
        len: usize,
        n_suggs: *mut usize,
    ) -> *mut *mut c_char;
    fn enchant_dict_free_string_list(dict: *mut EnchantDict, string_list: *mut *mut c_char);
    fn enchant_broker_dict_exists(broker: *mut EnchantBroker, tag: *const c_char) -> c_int;
}

struct Inner {
    broker: *mut EnchantBroker,
    dicts: HashMap<String, *mut EnchantDict>,
}

unsafe impl Send for Inner {}

/// Shared handle to the enchant broker. All calls are serialized because
/// enchant's broker is not documented as thread-safe; word checking happens
/// on highlight/statistics worker threads, so this must be shareable.
#[derive(Clone)]
pub struct SpellChecker {
    inner: std::sync::Arc<Mutex<Inner>>,
}

impl SpellChecker {
    pub fn new() -> Option<Self> {
        let broker = unsafe { enchant_broker_init() };
        if broker.is_null() {
            return None;
        }
        Some(Self {
            inner: std::sync::Arc::new(Mutex::new(Inner {
                broker,
                dicts: HashMap::new(),
            })),
        })
    }

    /// All available dictionary languages, e.g. `["en_US", "de_DE", …]`.
    ///
    /// Enchant's `list_dicts` is avoided (it crashes in some provider
    /// configurations); dictionary tags are instead enumerated from the
    /// standard hunspell/myspell dictionary directories and validated
    /// against the broker.
    pub fn available_languages(&self) -> Vec<String> {
        let mut dirs: Vec<std::path::PathBuf> = Vec::new();
        if let Ok(xdg) = std::env::var("XDG_DATA_DIRS") {
            for d in xdg.split(':').filter(|d| !d.is_empty()) {
                dirs.push(std::path::Path::new(d).join("hunspell"));
                dirs.push(std::path::Path::new(d).join("myspell/dicts"));
            }
        }
        dirs.push(std::path::PathBuf::from("/usr/share/hunspell"));
        dirs.push(std::path::PathBuf::from("/usr/share/myspell/dicts"));
        if let Ok(home) = std::env::var("HOME") {
            dirs.push(std::path::Path::new(&home).join(".local/share/hunspell"));
        }

        let mut langs: Vec<String> = Vec::new();
        let inner = self.inner.lock().unwrap();
        for dir in &dirs {
            let Ok(entries) = std::fs::read_dir(dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let name = entry.file_name();
                let Some(name) = name.to_str() else {
                    continue;
                };
                let Some(tag) = name.strip_suffix(".dic") else {
                    continue;
                };
                if tag.contains('-') || tag.is_empty() || langs.contains(&tag.to_owned()) {
                    continue;
                }
                if let Ok(tag_c) = std::ffi::CString::new(tag) {
                    let exists = unsafe {
                        enchant_broker_dict_exists(inner.broker, tag_c.as_ptr())
                    } != 0;
                    if exists {
                        langs.push(tag.to_owned());
                    }
                }
            }
        }
        langs.sort();
        langs
    }

    /// Whether `word` is considered misspelled in `lang`.
    pub fn is_misspelled(&self, lang: Option<&str>, word: &str) -> bool {
        let Ok(word_c) = CString::new(word) else {
            return false;
        };
        if word_c.as_bytes().len() > 256 {
            return false;
        }
        let mut inner = self.inner.lock().unwrap();
        let Some(dict) = Self::dict_for(&mut inner, lang) else {
            return false;
        };
        // Returns 0 when the word is in the dictionary.
        unsafe { enchant_dict_check(dict, word_c.as_ptr(), word.len()) != 0 }
    }

    /// Spelling suggestions for `word` in `lang`.
    pub fn suggestions(&self, lang: Option<&str>, word: &str) -> Vec<String> {
        let Ok(word_c) = CString::new(word) else {
            return Vec::new();
        };
        let mut inner = self.inner.lock().unwrap();
        let Some(dict) = Self::dict_for(&mut inner, lang) else {
            return Vec::new();
        };
        unsafe {
            let mut n: usize = 0;
            let list = enchant_dict_suggest(dict, word_c.as_ptr(), word.len(), &mut n);
            if list.is_null() {
                return Vec::new();
            }
            let mut out = Vec::with_capacity(n);
            for i in 0..n {
                let p = *list.add(i);
                if !p.is_null() {
                    out.push(std::ffi::CStr::from_ptr(p).to_string_lossy().into_owned());
                }
            }
            enchant_dict_free_string_list(dict, list);
            out
        }
    }

    fn dict_for(inner: &mut Inner, lang: Option<&str>) -> Option<*mut EnchantDict> {
        let lang = lang.map(str::to_owned).unwrap_or_else(Self::default_language);
        if let Some(d) = inner.dicts.get(&lang) {
            return Some(*d);
        }
        let Ok(tag) = CString::new(lang.clone()) else {
            return None;
        };
        let dict = unsafe { enchant_broker_request_dict(inner.broker, tag.as_ptr()) };
        if dict.is_null() {
            return None;
        }
        inner.dicts.insert(lang, dict);
        Some(dict)
    }

    fn default_language() -> String {
        std::env::var("LANG")
            .ok()
            .and_then(|l| l.split('.').next().map(str::to_owned))
            .unwrap_or_else(|| "en_US".into())
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        unsafe {
            for dict in self.dicts.values() {
                enchant_broker_free_dict(self.broker, *dict);
            }
            enchant_broker_free(self.broker);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_check() {
        let Some(sp) = SpellChecker::new() else {
            eprintln!("no enchant available; skipping");
            return;
        };
        let langs = sp.available_languages();
        if langs.is_empty() {
            // No provider/dictionary installed (e.g. hunspell missing):
            // enchant degrades gracefully and the check is vacuous.
            eprintln!("no enchant dictionaries available; skipping");
            return;
        }
        let lang = if langs.iter().any(|l| l.starts_with("en")) {
            Some("en_US")
        } else {
            langs.first().map(String::as_str)
        };
        assert!(!sp.is_misspelled(lang, "hello"));
        assert!(sp.is_misspelled(lang, "qqqqzzzzx"));
        let sugg = sp.suggestions(lang, "helo");
        assert!(sugg.iter().any(|s| s == "hello" || s.starts_with("hel")));
    }
}

