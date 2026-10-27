//! Live preview pane: WebKit web view, conversion pipeline, security
//! model, layout modes and scroll sync (`03-behavior/03-live-preview.md`).

pub mod convert;
pub mod security;

use webkit6::prelude::*;
use gtk::prelude::*;
use gtk::gio;
use gtk::glib;
use std::cell::Cell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::settings::{self, settings, ColorScheme, PreviewMode, PreviewSecurity};

/// Security level decided for the current document.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SecurityLevel {
    Unrestricted,
    Restricted,
    /// The user must choose (file contains raw HTML, policy is "ask").
    Ask,
}

pub struct PreviewPane {
    pub root: gtk::Stack,
    webview: webkit6::WebView,
    spinner_page: gtk::Box,
    warning_page: gtk::Box,
    generation: Arc<AtomicU64>,
    /// Guard against sync feedback loops.
    syncing: Cell<bool>,
    /// The security level chosen for the current document (Ask until
    /// decided).
    level: Cell<SecurityLevel>,
}

impl Clone for PreviewPane {
    fn clone(&self) -> Self {
        Self {
            root: self.root.clone(),
            webview: self.webview.clone(),
            spinner_page: self.spinner_page.clone(),
            warning_page: self.warning_page.clone(),
            generation: self.generation.clone(),
            syncing: Cell::new(false),
            level: Cell::new(self.level.get()),
        }
    }
}

fn resource_string(path: &str) -> String {
    gio::resources_lookup_data(
        &format!("/io/github/hakergeniusz/Griffel/{path}"),
        gio::ResourceLookupFlags::NONE,
    )
    .map(|b| String::from_utf8_lossy(&b).into_owned())
    .unwrap_or_default()
}

impl PreviewPane {
    pub fn new() -> Self {
        let root = gtk::Stack::new();
        root.set_transition_type(gtk::StackTransitionType::Crossfade);
        root.set_transition_duration(150);
        root.set_vexpand(true);
        root.set_hexpand(true);

        let spinner_page = gtk::Box::new(gtk::Orientation::Vertical, 12);
        spinner_page.set_valign(gtk::Align::Center);
        spinner_page.set_halign(gtk::Align::Center);
        let spinner = gtk::Spinner::new();
        spinner.set_size_request(32, 32);
        spinner.start();
        let label = gtk::Label::new(Some("Rendering preview…"));
        label.add_css_class("dim-label");
        spinner_page.append(&spinner);
        spinner_page.append(&label);

        let webview = webkit6::WebView::new();

        root.add_named(&spinner_page, Some("spinner"));
        root.add_named(&webview, Some("webview"));
        root.add_named(&gtk::Box::new(gtk::Orientation::Horizontal, 0), Some("empty"));

        let warning_page = build_warning_page();
        // The warning page is added lazily (it needs the pane handle).

        let pane = Self {
            root,
            webview,
            spinner_page,
            warning_page,
            generation: Arc::new(AtomicU64::new(0)),
            syncing: Cell::new(false),
            level: Cell::new(SecurityLevel::Ask),
        };
        pane.setup_webview_policy();
        pane
    }

    fn setup_webview_policy(&self) {
        // Web links open in the external browser; the preview never
        // navigates itself (D5).
        self.webview.connect_decide_policy(move |_wv, decision, kind| {
            if kind == webkit6::PolicyDecisionType::NavigationAction {
                if let Some(nav) = decision
                    .downcast_ref::<webkit6::NavigationPolicyDecision>()
                    .and_then(|d| d.navigation_action())
                {
                    if let Some(request) = {
                        let mut nav_mut = nav.clone();
                        nav_mut.request()
                    } {
                        let uri = request.uri().map(|u| u.to_string()).unwrap_or_default();
                        let scheme = uri.split(':').next().unwrap_or("").to_ascii_lowercase();
                        if matches!(scheme.as_str(), "http" | "https" | "ftp" | "mailto") {
                            gio::AppInfo::launch_default_for_uri(
                                &uri,
                                None::<&gio::AppLaunchContext>,
                            )
                            .ok();
                            decision.ignore();
                            return true;
                        }
                    }
                }
            }
            false
        });

        // Context menu without navigation entries (§5).
        self.webview.connect_context_menu(|_wv, menu, _event| {
            use webkit6::ContextMenuAction::*;
            let items = menu.items();
            let to_remove: Vec<_> = items
                .iter()
                .filter(|item| {
                    matches!(item.stock_action(), Reload | GoBack | GoForward | Stop)
                })
                .cloned()
                .collect();
            for item in to_remove {
                menu.remove(&item);
            }
            false
        });
    }

    /// Decide the security level for a newly loaded document (§5).
    pub fn evaluate_security(&self, text: &str, is_new_document: bool) {
        let policy = match settings().string("preview-security").as_str() {
            "always-restricted" => PreviewSecurity::AlwaysRestricted,
            "always-unrestricted" => PreviewSecurity::AlwaysUnrestricted,
            _ => PreviewSecurity::Ask,
        };
        let level = match policy {
            PreviewSecurity::AlwaysRestricted => SecurityLevel::Restricted,
            PreviewSecurity::AlwaysUnrestricted => SecurityLevel::Unrestricted,
            PreviewSecurity::Ask => {
                if is_new_document {
                    SecurityLevel::Unrestricted
                } else if security::contains_raw_html(text) {
                    SecurityLevel::Ask
                } else {
                    SecurityLevel::Restricted
                }
            }
        };
        self.level.set(level);
    }

    pub fn choose_level(&self, level: SecurityLevel) {
        self.level.set(level);
    }

    pub fn level(&self) -> SecurityLevel {
        self.level.get()
    }

    /// Trigger a (debounced by caller) re-render of `text`.
    pub fn update(&self, text: &str, doc_dir: Option<std::path::PathBuf>) {
        let flavor = settings().input_format();
        let dark_scheme = match settings().color_scheme() {
            ColorScheme::Dark => true,
            ColorScheme::System => adw::StyleManager::default().is_dark(),
            _ => false,
        };
        let restricted = self.level.get() != SecurityLevel::Unrestricted;

        if self.level.get() == SecurityLevel::Ask {
            self.show_warning_page();
            return;
        }

        // Show the spinner only while waiting for the FIRST render or
        // when the level was just decided; later updates keep the old
        // frame visible (§3).
        let generation_id = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let first = self.root.visible_child_name().map(|n| n == "empty" || n == "spinner").unwrap_or(true);
        if first {
            self.root.set_visible_child_name("spinner");
        }

        let text = text.to_owned();
        let doc_dir_for_cb = doc_dir.clone();
        let pane = self.clone();
        glib::spawn_future_local(async move {
            let handle = gio::spawn_blocking(move || {
                convert::to_html_fragment(&text, flavor, restricted, doc_dir.as_deref())
            });
            let result = handle.await.unwrap_or_else(|_| Err("conversion cancelled".to_owned()));
            // Drop stale results.
            if pane.generation.load(Ordering::SeqCst) != generation_id {
                return;
            }
            match result {
                Err(e) => {
                    pane.show_error_page(&e);
                }
                Ok(fragment) => {
                    let base = resource_string("styles/base.css");
                    let scheme_css = resource_string(
                        &settings().color_scheme().preview_css(dark_scheme),
                    );
                    let doc = convert::preview_document(&fragment, &base, &scheme_css, dark_scheme);
                    let base_uri: Option<String> = doc_dir_for_cb
                        .as_ref()
                        .map(|d| gio::File::for_path(d).uri().to_string());
                    pane.webview.load_html(&doc, base_uri.as_deref());
                    pane.root.set_visible_child_name("webview");
                }
            }
        });
    }

    fn show_warning_page(&self) {
        if self.warning_page.parent().is_none() {
            self.root.add_named(&self.warning_page, Some("warning"));
        }
        self.root.set_visible_child_name("warning");
    }

    fn show_error_page(&self, message: &str) {
        if self.warning_page.parent().is_none() {
            self.root.add_named(&self.warning_page, Some("warning"));
        }
        // Reuse the warning page structure with an error message.
        set_warning_content(&self.warning_page, "Conversion failed", message, None);
        self.root.set_visible_child_name("warning");
    }

    pub fn show_spinner(&self) {
        self.root.set_visible_child_name("spinner");
    }

    /// Install the warning page with its two choice buttons wired.
    pub fn setup_warning_page(&self, on_choice: std::rc::Rc<dyn Fn(SecurityLevel)>) {
        build_warning_content(
            &self.warning_page,
            "This file contains HTML",
            "The document includes raw HTML. Loading it unrestricted allows scripts and full HTML; loading it restricted renders HTML as plain text.",
            Some(("Load Unrestricted", SecurityLevel::Unrestricted, false)),
            Some(("Load Restricted", SecurityLevel::Restricted, true)),
            on_choice,
        );
    }

    /// Editor → preview scroll sync (proportional).
    pub fn sync_from_editor(&self, fraction: f64) {
        if self.syncing.get() {
            return;
        }
        if !settings().boolean("sync-scroll") {
            return;
        }
        self.syncing.set(true);
        let js = format!(
            "window.scrollTo(0, ({f}) * (document.documentElement.scrollHeight - window.innerHeight)); true;",
            f = fraction
        );
        self.webview.evaluate_javascript(&js, None::<&str>, None::<&str>, None::<&gio::Cancellable>, |_| {});
        // Release the guard on the next idle cycle.
        let pane = self.clone();
        glib::idle_add_local_once(move || {
            pane.syncing.set(false);
        });
    }

    /// Preview → editor sync hook: called from the injected JS handler.
    pub fn sync_from_preview(&self, fraction: f64, apply: impl Fn(f64)) {
        if self.syncing.get() {
            return;
        }
        if !settings().boolean("sync-scroll") {
            return;
        }
        self.syncing.set(true);
        apply(fraction);
        let pane = self.clone();
        glib::idle_add_local_once(move || {
            pane.syncing.set(false);
        });
    }

    /// Install the scroll-reporting script in the page (after each load).
    pub fn install_scroll_script(&self, on_fraction: impl Fn(f64) + 'static) {
        let Some(manager) = self.webview.user_content_manager() else {
            return;
        };
        let script = webkit6::UserScript::new(
            "window.addEventListener('scroll', () => { \
                 const max = document.documentElement.scrollHeight - window.innerHeight; \
                 const f = max > 0 ? window.scrollY / max : 0; \
                 window.webkit.messageHandlers.previewScroll.postMessage(f); \
             }, { passive: true });",
            webkit6::UserContentInjectedFrames::TopFrame,
            webkit6::UserScriptInjectionTime::End,
            &[],
            &[],
        );
        manager.add_script(&script);
        manager.register_script_message_handler("previewScroll", None::<&str>);
        let manager = manager.clone();
        manager.connect_script_message_received(Some("previewScroll"), move |_m, value| {
            let f = value.to_string().trim().parse::<f64>().unwrap_or(-1.0);
            if (0.0..=1.0).contains(&f) {
                on_fraction(f);
            }
        });
    }

    pub fn webview(&self) -> &webkit6::WebView {
        &self.webview
    }
}

impl Default for PreviewPane {
    fn default() -> Self {
        Self::new()
    }
}

fn build_warning_page() -> gtk::Box {
    gtk::Box::new(gtk::Orientation::Vertical, 12)
}

#[allow(clippy::too_many_arguments)]
fn build_warning_content(
    page: &gtk::Box,
    title: &str,
    body: &str,
    left: Option<(&str, SecurityLevel, bool)>,
    right: Option<(&str, SecurityLevel, bool)>,
    on_choice: std::rc::Rc<dyn Fn(SecurityLevel)>,
) {
    while let Some(child) = page.first_child() {
        page.remove(&child);
    }
    page.set_valign(gtk::Align::Center);
    page.set_halign(gtk::Align::Center);
    page.set_margin_all_pub(24);

    let icon = gtk::Image::from_icon_name("dialog-warning-symbolic");
    icon.set_pixel_size(48);
    let title_label = gtk::Label::new(Some(title));
    title_label.add_css_class("title-2");
    let body_label = gtk::Label::new(Some(body));
    body_label.add_css_class("dim-label");
    body_label.set_justify(gtk::Justification::Center);
    body_label.set_wrap(true);
    body_label.set_width_chars(46);

    page.append(&icon);
    page.append(&title_label);
    page.append(&body_label);

    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row.set_halign(gtk::Align::Center);
    for (label_text, level, emphasized) in [left, right].into_iter().flatten() {
        let b = gtk::Button::with_label(label_text);
        if emphasized {
            b.add_css_class("suggested-action");
        }
        b.add_css_class("pill");
        let on_choice = on_choice.clone();
        b.connect_clicked(move |_| on_choice(level));
        row.append(&b);
    }
    page.append(&row);
}

fn set_warning_content(page: &gtk::Box, title: &str, body: &str, buttons: Option<(&str, SecurityLevel)>) {
    while let Some(child) = page.first_child() {
        page.remove(&child);
    }
    page.set_valign(gtk::Align::Center);
    page.set_halign(gtk::Align::Center);
    page.set_margin_top(24);
    page.set_margin_bottom(24);
    page.set_margin_start(24);
    page.set_margin_end(24);
    let icon = gtk::Image::from_icon_name("dialog-warning-symbolic");
    icon.set_pixel_size(48);
    let title_label = gtk::Label::new(Some(title));
    title_label.add_css_class("title-2");
    let body_label = gtk::Label::new(Some(body));
    body_label.add_css_class("dim-label");
    body_label.set_justify(gtk::Justification::Center);
    body_label.set_wrap(true);
    body_label.set_width_chars(46);
    page.append(&icon);
    page.append(&title_label);
    page.append(&body_label);
    if let Some((text, _level)) = buttons {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        row.set_halign(gtk::Align::Center);
        row.append(&gtk::Button::with_label(text));
        page.append(&row);
    }
}

/// Small helper since set_margin_all is not available in gtk4 0.10.
trait MarginAll {
    fn set_margin_all_pub(&self, m: i32);
}
impl MarginAll for gtk::Box {
    fn set_margin_all_pub(&self, m: i32) {
        self.set_margin_top(m);
        self.set_margin_bottom(m);
        self.set_margin_start(m);
        self.set_margin_end(m);
    }
}
