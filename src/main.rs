//! Griffel — a distraction-free Markdown editor.

mod app;
mod appearance;
mod editor;
mod files;
mod markdown;
mod settings;
mod spell;
mod window;

use gtk::prelude::*;

pub mod verbose {
    use std::cell::Cell;
    thread_local! {
        static ENABLED: Cell<bool> = const { Cell::new(false) };
    }
    pub fn enable() {
        ENABLED.with(|e| e.set(true));
        gtk::glib::g_info!("griffel", "verbose logging enabled");
    }
    pub fn enabled() -> bool {
        ENABLED.with(|e| e.get())
    }
}

fn main() -> glib::ExitCode {
    glib::set_application_name("Griffel");
    glib::set_prgname(Some(app::APP_ID));

    // Register the embedded resource bundle (settings schema, styles,
    // tutorial, icons) before anything reads it.
    let resource_bytes = include_bytes!(concat!(env!("OUT_DIR"), "/griffel.gresource"));
    let resource = gio::Resource::from_data(&glib::Bytes::from_static(resource_bytes))
        .expect("embedded resource bundle is corrupt");
    gio::resources_register(&resource);

    settings::init().expect("settings schema failed to load");

    let application = app::GriffelApp::new();
    application.run()
}
