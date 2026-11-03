//! Color schemes: chrome (via libadwaita style manager + CSS overrides),
//! editor palettes for in-place highlighting, and preview stylesheet
//! selection.

use gtk::prelude::*;
use sourceview5::prelude::*;
use std::cell::RefCell;

use crate::settings::{settings, ColorScheme};

/// Editor-specific colors for one scheme. These feed both the editor CSS
/// and the in-place highlighting tags.
#[derive(Clone, Debug)]
pub struct EditorPalette {
    pub background: String,
    pub foreground: String,
    pub accent: String,
    pub dimmed: String,
    pub code_background: String,
    pub code_foreground: String,
    pub selection: String,
    /// Color used by Focus Mode for non-active text.
    pub dim_focus: String,
    /// Code Mode token colors inside fenced code blocks.
    pub code_string: String,
    pub code_number: String,
    pub code_comment: String,
    pub code_keyword: String,
    /// Identifier of the generated GtkSourceView style scheme for this
    /// palette (Code Mode gutter and current-line highlight).
    pub scheme_key: String,
}

fn palettes(scheme: ColorScheme, system_dark: bool) -> EditorPalette {
    match (scheme, system_dark) {
        (_, true) if scheme == ColorScheme::System => EditorPalette {
            background: "#1e2126".into(),
            foreground: "#e6e6e6".into(),
            accent: "#78aeed".into(),
            dimmed: "#79808a".into(),
            code_background: "#2b3037".into(),
            code_foreground: "#cfd3d9".into(),
            selection: "#37546e".into(),
            dim_focus: "#5c636d".into(),
            code_string: "#8ff0a4".into(),
            code_number: "#ffbe6f".into(),
            code_comment: "#8a909b".into(),
            code_keyword: "#dc8add".into(),
            scheme_key: "system-dark".into(),
        },
        (ColorScheme::System, _) | (ColorScheme::Light, _) => EditorPalette {
            background: "#ffffff".into(),
            foreground: "#1e2126".into(),
            accent: "#1c71d8".into(),
            dimmed: "#8a919c".into(),
            code_background: "#f2f4f7".into(),
            code_foreground: "#3a4048".into(),
            selection: "#cfe0f5".into(),
            dim_focus: "#b3b9c2".into(),
            code_string: "#1f7a4d".into(),
            code_number: "#c64600".into(),
            code_comment: "#8a919c".into(),
            code_keyword: "#613583".into(),
            scheme_key: if scheme == ColorScheme::System {
                "system-light".into()
            } else {
                "light".into()
            },
        },
        (ColorScheme::Sepia, _) => EditorPalette {
            background: "#f4ecd8".into(),
            foreground: "#43341f".into(),
            accent: "#8f4e0f".into(),
            dimmed: "#a2916f".into(),
            code_background: "#ebe0c6".into(),
            code_foreground: "#5c4a2c".into(),
            selection: "#e3d3a8".into(),
            dim_focus: "#cbbf9e".into(),
            code_string: "#5c722d".into(),
            code_number: "#a84a10".into(),
            code_comment: "#a2916f".into(),
            code_keyword: "#7d4a8a".into(),
            scheme_key: "sepia".into(),
        },
        (ColorScheme::Dark, _) => EditorPalette {
            background: "#1e2126".into(),
            foreground: "#e6e6e6".into(),
            accent: "#78aeed".into(),
            dimmed: "#79808a".into(),
            code_background: "#2b3037".into(),
            code_foreground: "#cfd3d9".into(),
            selection: "#37546e".into(),
            dim_focus: "#5c636d".into(),
            code_string: "#8ff0a4".into(),
            code_number: "#ffbe6f".into(),
            code_comment: "#8a909b".into(),
            code_keyword: "#dc8add".into(),
            scheme_key: "dark".into(),
        },
    }
}

/// Resolve the effective scheme: `System` follows the platform preference.
pub fn effective_palette() -> EditorPalette {
    let scheme = settings().color_scheme();
    let system_dark = adw::StyleManager::default().is_dark();
    palettes(scheme, system_dark)
}

/// Whether the platform exposes a color-scheme preference ("system" entry
/// is hidden when it does not).
pub fn system_supports_color_schemes() -> bool {
    adw::StyleManager::default().system_supports_color_schemes()
}

/// GtkSourceView draws the Code Mode chrome — the line-number gutter and
/// the current-line highlight — from the buffer's style scheme, not from
/// CSS, so without a scheme they render with hard-coded light defaults and
/// break every non-light scheme. The palette stays the single source of
/// truth: the scheme XML is generated from it. Deliberately no `text` or
/// `selection` styles here — the paper, glyph and selection colors are
/// pinned by the widget-level editor CSS, and a second writer for them
/// would only invite drift.
fn scheme_xml(p: &EditorPalette) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<style-scheme id="{id}" name="Griffel {id}" version="1.0">
  <author>Griffel</author>
  <description>Generated from the Griffel editor palette.</description>
  <style name="current-line" background="{current_line}"/>
  <style name="line-numbers" foreground="{gutter_fg}" background="{paper}"/>
</style-scheme>
"#,
        id = p.scheme_key,
        current_line = p.code_background,
        gutter_fg = p.dimmed,
        paper = p.background,
    )
}

/// Generate and load the style scheme for `p`. The XML is materialized
/// under the Griffel cache directory (same pattern as the embedded
/// GSettings schema) and re-written on every call, so palette updates are
/// never served stale from a previous run. Returns None when the file
/// cannot be written; the editor then keeps GtkSourceView's defaults.
pub fn editor_style_scheme(p: &EditorPalette) -> Option<sourceview5::StyleScheme> {
    let dir = glib::user_cache_dir().join("griffel").join("schemes");
    std::fs::create_dir_all(&dir).ok()?;
    let path = dir.join(format!("{}.xml", p.scheme_key));
    std::fs::write(&path, scheme_xml(p)).ok()?;
    let manager = sourceview5::StyleSchemeManager::new();
    manager.append_search_path(dir.to_str()?);
    manager.scheme(&p.scheme_key)
}

thread_local! {
    static PROVIDERS: RefCell<Vec<gtk::CssProvider>> = const { RefCell::new(Vec::new()) };
}

/// Apply the current scheme to the whole application: chrome style manager
/// state, the palette-driven bottom-bar stylesheet, and the sepia chrome
/// override. Re-run on scheme change; replaces previously applied providers.
pub fn apply() {
    let scheme = settings().color_scheme();
    let manager = adw::StyleManager::default();

    match scheme {
        ColorScheme::System => {
            manager.set_color_scheme(adw::ColorScheme::Default);
        }
        ColorScheme::Light | ColorScheme::Sepia => {
            manager.set_color_scheme(adw::ColorScheme::ForceLight);
        }
        ColorScheme::Dark => {
            manager.set_color_scheme(adw::ColorScheme::ForceDark);
        }
    }

    // Chrome override for sepia: warm paper tones over the light style.
    let chrome_css = match scheme {
        ColorScheme::Sepia => SEPIA_CHROME_CSS.to_owned(),
        _ => String::new(),
    };
    let css = format!("{}\n{}", bottom_bar_css(&effective_palette()), chrome_css);
    PROVIDERS.with(|providers| {
        {
            let mut list = providers.borrow_mut();
            for p in list.drain(..) {
                gtk::style_context_remove_provider_for_display(
                    &gdk::Display::default().expect("no display"),
                    &p,
                );
            }
        }
        let mut list = providers.borrow_mut();
        let p = gtk::CssProvider::new();
        p.load_from_string(&css);
        gtk::style_context_add_provider_for_display(
            &gdk::Display::default().expect("no display"),
            &p,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1,
        );
        list.push(p);
    });
}

/// The bottom toolbar row sits on a subtle card (`02-ui-layout.md` §5);
/// its colors follow the editor palette so every scheme matches.
fn bottom_bar_css(p: &EditorPalette) -> String {
    format!(
        ".toolbar-row {{\
            background-color: {bg};\
            border: 1px solid alpha({border}, 0.55);\
            border-radius: 10px;\
            padding: 3px 4px;\
        }}",
        bg = p.code_background,
        border = p.dimmed,
    )
}

const SEPIA_CHROME_CSS: &str = r#"
:root, window { }
window, headerbar, actionbar, dialog {
    background-color: #efe4c9;
}
headerbar {
    background-color: #e8dbba;
    color: #43341f;
}
label, button { color: inherit; }
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scheme_xml_carries_palette_colors_and_id() {
        for (scheme, system_dark) in [
            (ColorScheme::System, true),
            (ColorScheme::System, false),
            (ColorScheme::Light, false),
            (ColorScheme::Sepia, false),
            (ColorScheme::Dark, false),
        ] {
            let p = palettes(scheme, system_dark);
            let xml = scheme_xml(&p);
            assert!(xml.starts_with("<?xml"), "scheme {}", p.scheme_key);
            assert!(xml.contains(&format!("id=\"{}\"", p.scheme_key)));
            assert!(xml.contains("name=\"current-line\""));
            assert!(xml.contains(&format!("background=\"{}\"", p.code_background)));
            assert!(xml.contains("name=\"line-numbers\""));
            assert!(xml.contains(&format!("foreground=\"{}\"", p.dimmed)));
            assert!(xml.contains(&format!("background=\"{}\"", p.background)));
            // Text and selection stay owned by the widget-level editor CSS.
            assert!(!xml.contains("name=\"text\""));
            assert!(!xml.contains("name=\"selection\""));
        }
    }

    #[test]
    fn scheme_keys_distinguish_system_dark_from_explicit_dark() {
        assert_ne!(
            palettes(ColorScheme::System, true).scheme_key,
            palettes(ColorScheme::Dark, true).scheme_key
        );
        assert_ne!(
            palettes(ColorScheme::System, false).scheme_key,
            palettes(ColorScheme::Light, false).scheme_key
        );
        // Sepia ignores the platform preference entirely.
        assert_eq!(
            palettes(ColorScheme::Sepia, true).scheme_key,
            palettes(ColorScheme::Sepia, false).scheme_key
        );
    }
}
