//! Color schemes: chrome (via libadwaita style manager + CSS overrides),
//! editor palettes for in-place highlighting, and preview stylesheet
//! selection.

use gtk::prelude::*;
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

thread_local! {
    static PROVIDERS: RefCell<Vec<gtk::CssProvider>> = const { RefCell::new(Vec::new()) };
}

/// Apply the current scheme to the whole application: chrome style manager
/// state plus the sepia chrome override stylesheet and the editor palette.
/// Re-run on scheme change; replaces previously applied providers.
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
        ColorScheme::Sepia => SEPIA_CHROME_CSS,
        _ => "",
    };
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
        if !chrome_css.is_empty() {
            let p = gtk::CssProvider::new();
            p.load_from_string(chrome_css);
            gtk::style_context_add_provider_for_display(
                &gdk::Display::default().expect("no display"),
                &p,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1,
            );
            list.push(p);
        }
        let editor = gtk::CssProvider::new();
        editor.load_from_string(&editor_css(&effective_palette()));
        gtk::style_context_add_provider_for_display(
            &gdk::Display::default().expect("no display"),
            &editor,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 2,
        );
        list.push(editor);
    });
}

fn editor_css(p: &EditorPalette) -> String {
    format!(
        ".editor-view {{ background-color: {bg}; color: {fg}; }}\n\
         .editor-view selection {{ background-color: {sel}; }}\n\
         .editor-gutter {{ background-color: {bg}; }}",
        bg = p.background,
        fg = p.foreground,
        sel = p.selection,
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
