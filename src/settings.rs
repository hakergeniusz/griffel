//! Application settings: typed wrapper over GSettings with the schema
//! embedded in the binary's GResource bundle.

use gio::prelude::*;
use std::sync::OnceLock;

pub const SCHEMA_ID: &str = "io.github.hakergeniusz.Griffel";

static SETTINGS: OnceLock<Settings> = OnceLock::new();

pub fn settings() -> &'static Settings {
    SETTINGS.get().expect("settings not initialized")
}

/// Install the embedded schema as a chained schema source and construct the
/// application settings. Must be called once before any `settings()` use.
pub fn init() -> Result<(), String> {
    if SETTINGS.get().is_some() {
        return Ok(());
    }
    let set = Settings::new_embedded().map_err(|e| e.to_string())?;
    let _ = SETTINGS.set(set);
    Ok(())
}

#[derive(Clone)]
pub struct Settings {
    s: gio::Settings,
}

// GSettings is thread-safe on the C side; all callers use it read-mostly
// from the main thread, but worker threads read too.
unsafe impl Send for Settings {}
unsafe impl Sync for Settings {}

impl Settings {
    fn new_embedded() -> Result<Self, glib::Error> {
        // The compiled schema is embedded in the binary; this glib has no
        // schema-source-from-resource constructor, so materialize it into a
        // cache directory and load the source from there, chaining the
        // system default as parent.
        let bytes: &'static [u8] =
            include_bytes!(concat!(env!("OUT_DIR"), "/gschemas.compiled"));
        let dir = glib::user_cache_dir().join("griffel").join("schemas");
        std::fs::create_dir_all(&dir)
            .map_err(|e| glib::Error::new(gio::IOErrorEnum::Failed, &e.to_string()))?;
        let path = dir.join("gschemas.compiled");
        std::fs::write(&path, bytes)
            .map_err(|e| glib::Error::new(gio::IOErrorEnum::Failed, &e.to_string()))?;

        let parent = gio::SettingsSchemaSource::default();
        let source = gio::SettingsSchemaSource::from_directory(&dir, parent.as_ref(), true)?;
        let schema = source
            .lookup(SCHEMA_ID, true)
            .ok_or_else(|| glib::Error::new(gio::IOErrorEnum::Failed, "schema not found"))?;
        Ok(Self {
            s: gio::Settings::new_full(&schema, None::<&gio::SettingsBackend>, None),
        })
    }

    pub fn bind_widget(&self, key: &str, object: &impl IsA<glib::Object>, property: &str) {
        self.s.bind(key, object, property);
    }

    pub fn changed<F: Fn(&Settings, &str) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.s.connect_changed(None::<&str>, move |_, key| f(settings(), key))
    }

    pub fn inner(&self) -> &gio::Settings {
        &self.s
    }

    // ---- typed accessors -------------------------------------------------

    pub fn string(&self, key: &str) -> String {
        self.s.string(key).into()
    }
    pub fn set_string(&self, key: &str, value: &str) {
        let _ = self.s.set_string(key, value);
    }
    pub fn boolean(&self, key: &str) -> bool {
        self.s.boolean(key)
    }
    pub fn set_boolean(&self, key: &str, value: bool) {
        let _ = self.s.set_boolean(key, value);
    }
    pub fn integer(&self, key: &str) -> i32 {
        self.s.int(key)
    }
    pub fn set_integer(&self, key: &str, value: i32) {
        let _ = self.s.set_int(key, value);
    }

    pub fn color_scheme(&self) -> ColorScheme {
        ColorScheme::parse(&self.string("color-scheme")).unwrap_or(ColorScheme::System)
    }
    pub fn set_color_scheme(&self, v: ColorScheme) {
        self.set_string("color-scheme", v.as_str());
    }

    pub fn input_format(&self) -> InputFormat {
        InputFormat::parse(&self.string("input-format")).unwrap_or(InputFormat::Markdown)
    }
    pub fn set_input_format(&self, v: InputFormat) {
        self.set_string("input-format", v.as_str());
    }

    pub fn preview_mode(&self) -> PreviewMode {
        PreviewMode::parse(&self.string("preview-mode")).unwrap_or(PreviewMode::FullWidth)
    }
    pub fn set_preview_mode(&self, v: PreviewMode) {
        self.set_string("preview-mode", v.as_str());
    }

    pub fn preview_security(&self) -> PreviewSecurity {
        PreviewSecurity::parse(&self.string("preview-security"))
            .unwrap_or(PreviewSecurity::Ask)
    }
    pub fn set_preview_security(&self, v: PreviewSecurity) {
        self.set_string("preview-security", v.as_str());
    }

    pub fn default_statistic(&self) -> Statistic {
        Statistic::parse(&self.string("default-statistic")).unwrap_or(Statistic::Words)
    }
    pub fn set_default_statistic(&self, v: Statistic) {
        self.set_string("default-statistic", v.as_str());
    }
}

macro_rules! string_enum {
    ($(#[$meta:meta])* $name:ident { $($variant:ident => $str:expr),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum $name { $($variant),+ }

        impl $name {
            pub fn as_str(&self) -> &'static str {
                match self { $($name::$variant => $str),+ }
            }
            pub fn parse(s: &str) -> Option<Self> {
                match s { $($str => Some($name::$variant),)+ _ => None }
            }
            pub const ALL: &'static [$name] = &[$($name::$variant),+];
        }
    };
}

string_enum!(
    /// Editor/preview/chrome color scheme.
    ColorScheme {
        System => "system",
        Light => "light",
        Sepia => "sepia",
        Dark => "dark",
    }
);

string_enum!(
    /// Converter input flavor (`05-file-formats-and-persistence.md` §2).
    InputFormat {
        Markdown => "markdown",
        CommonMark => "commonmark",
        Gfm => "gfm",
        MarkdownMmd => "markdown_mmd",
        MarkdownStrict => "markdown_strict",
    }
);

string_enum!(
    /// Preview layout mode.
    PreviewMode {
        FullWidth => "full-width",
        HalfWidth => "half-width",
        HalfHeight => "half-height",
        Windowed => "windowed",
    }
);

string_enum!(
    /// Preview security policy.
    PreviewSecurity {
        Ask => "ask",
        AlwaysRestricted => "always-restricted",
        AlwaysUnrestricted => "always-unrestricted",
    }
);

string_enum!(
    /// Which statistic the status button shows.
    Statistic {
        Characters => "characters",
        Words => "words",
        Sentences => "sentences",
        Paragraphs => "paragraphs",
        ReadTime => "read_time",
    }
);

impl InputFormat {
    /// Identifier passed to the external converter.
    pub fn converter_id(&self) -> &'static str {
        self.as_str()
    }

    /// Human-readable name for the preferences dropdown.
    pub fn display_name(&self) -> &'static str {
        match self {
            InputFormat::Markdown => "Pandoc's Markdown",
            InputFormat::CommonMark => "CommonMark",
            InputFormat::Gfm => "GitHub-Flavored Markdown",
            InputFormat::MarkdownMmd => "MultiMarkdown",
            InputFormat::MarkdownStrict => "Original Markdown (strict)",
        }
    }

    /// Documentation website opened by the help button.
    pub fn docs_url(&self) -> &'static str {
        match self {
            InputFormat::Markdown => "https://pandoc.org/MANUAL.html#pandocs-markdown",
            InputFormat::CommonMark => "https://commonmark.org/",
            InputFormat::Gfm => "https://github.github.com/gfm/",
            InputFormat::MarkdownMmd => "https://fletcher.github.io/MultiMarkdown-6/",
            InputFormat::MarkdownStrict => "https://daringfireball.net/projects/markdown/syntax",
        }
    }
}

impl ColorScheme {
    /// Resource basename of the preview stylesheet for this scheme; `System`
    /// resolves at call time via the platform style manager.
    pub fn preview_css(self, system_prefers_dark: bool) -> &'static str {
        match self {
            ColorScheme::System => {
                if system_prefers_dark {
                    "styles/system-dark.css"
                } else {
                    "styles/system-light.css"
                }
            }
            ColorScheme::Light => "styles/light.css",
            ColorScheme::Sepia => "styles/sepia.css",
            ColorScheme::Dark => "styles/dark.css",
        }
    }
}
