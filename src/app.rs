//! Application object: single instance, actions, accelerators, file open
//! routing.

use gtk::prelude::*;
use gtk::gio;
use gtk::glib;

use crate::window::AppWindow;

pub const APP_ID: &str = "io.github.hakergeniusz.Griffel";
pub const ICON_NAME: &str = APP_ID;

/// Registry of live document windows. `AppWindow` is a plain struct, so
/// strong clones are kept here; entries are removed when their window
/// closes. The registry never blocks the process from exiting (the app
/// quits when the last window closes).
thread_local! {
    static WINDOWS: std::cell::RefCell<Vec<crate::window::AppWindow>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

pub fn register_window(win: crate::window::AppWindow) {
    WINDOWS.with(|w| w.borrow_mut().push(win));
}

pub fn unregister_window(win: &crate::window::AppWindow) {
    WINDOWS.with(|w| {
        w.borrow_mut()
            .retain(|other| !other.same_window(win))
    });
}

pub fn window_registry() -> Vec<crate::window::AppWindow> {
    WINDOWS.with(|w| w.borrow().clone())
}

pub struct GriffelApp {
    app: gtk::Application,
}

impl GriffelApp {
    pub fn new() -> Self {
        let app = gtk::Application::builder()
            .application_id(APP_ID)
            .flags(gio::ApplicationFlags::HANDLES_OPEN)
            .build();

        app.add_main_option(
            "verbose",
            glib::Char::from(b'v'),
            glib::OptionFlags::NONE,
            glib::OptionArg::None,
            "Enable verbose console logging",
            None,
        );
        app.add_main_option(
            "new-window",
            glib::Char::from(b'w'),
            glib::OptionFlags::NONE,
            glib::OptionArg::None,
            "Open a new empty window",
            None,
        );

        app.connect_handle_local_options(|_, dict| {
            if dict.contains("verbose") {
                crate::verbose::enable();
            }
            // Continue: in a secondary instance the request is forwarded to
            // the primary (activate/open signals).
            std::ops::ControlFlow::Continue(())
        });

        app.connect_startup(|_app| {
            gtk::Window::set_default_icon_name(ICON_NAME);
            if let Some(display) = gdk::Display::default() {
                gtk::IconTheme::for_display(&display)
                    .add_resource_path(&format!("/{}/icons", APP_ID));
            }
            crate::appearance::apply();
            // Scheme changes restyle every open editor immediately.
            crate::settings::settings().changed(move |_s, key| {
                if key == "color-scheme" {
                    crate::appearance::apply();
                    for w in window_registry() {
                        w.editor.recolor();
                        w.editor.highlight_now();
                    }
                }
            });
        });

        app.connect_activate(|app| {
            // Every activation creates a new empty document window
            // (`03-behavior/12-windows.md` §1). Snapshot recovery happens
            // once, on the first activation of the primary instance.
            if !app.is_remote() {
                crate::files::recover_snapshots(app);
            }
            let win = AppWindow::new(app);
            if std::env::var("GRIFFEL_OPEN").ok().as_deref() == Some("tutorial") {
                crate::files::open_tutorial(app);
            } else {
                win.present_window();
            }
            register_accels(app);
        });

        app.connect_open(|app, files, _hint| {
            crate::files::open_files(app, &files);
            register_accels(app);
        });

        let new_window = gio::SimpleAction::new("new-window", None);
        {
            let app = app.clone();
            new_window.connect_activate(move |_, _| {
                let win = AppWindow::new(&app);
                win.present_window();
            });
        }
        app.add_action(&new_window);

        let open_tutorial = gio::SimpleAction::new("open-tutorial", None);
        {
            let app = app.clone();
            open_tutorial.connect_activate(move |_, _| {
                crate::files::open_tutorial(&app);
            });
        }
        app.add_action(&open_tutorial);

        let quit = gio::SimpleAction::new("quit", None);
        {
            let app = app.clone();
            quit.connect_activate(move |_, _| {
                // Every window runs the unsaved-changes close sequence; the
                // app quits only if all windows close (M11 wires the guard).
                for window in app.windows() {
                    window.close();
                }
            });
        }
        app.add_action(&quit);

        Self { app }
    }

    pub fn gtk_app(&self) -> &gtk::Application {
        &self.app
    }

    pub fn run(self) -> glib::ExitCode {
        self.app.run()
    }
}

/// While any document is dirty, inhibit session logout (K2). Released when
/// all documents are clean.
pub fn update_logout_inhibition() {
    thread_local! {
        static COOKIE: std::cell::Cell<Option<u32>> = const { std::cell::Cell::new(None) };
    }
    let dirty = window_registry()
        .iter()
        .any(|w| w.document.borrow().dirty);
    let app = gtk::Application::default();
    let current = COOKIE.get();
    if dirty && current.is_none() {
        let cookie = app.inhibit(
            None::<&gtk::Window>,
            gtk::ApplicationInhibitFlags::LOGOUT,
            Some("A document has unsaved changes"),
        );
        COOKIE.set(Some(cookie));
    } else if !dirty && current.is_some() {
        app.uninhibit(current.unwrap());
        COOKIE.set(None);
    }
}

fn register_accels(app: &gtk::Application) {
    for (action, accels) in [
        ("app.new-window", vec!["<primary>n"]),
        ("app.quit", vec!["<primary>q"]),
        ("win.open", vec!["<primary>o"]),
        ("win.save", vec!["<primary>s"]),
        ("win.save-as", vec!["<primary><shift>s"]),
        ("win.close", vec!["<primary>w"]),
        ("win.find", vec!["<primary>f"]),
        ("win.find-replace", vec!["<primary>h"]),
        ("win.toggle-preview", vec!["<primary>p"]),
        ("win.focus-mode", vec!["<primary>d"]),
        ("win.hemingway-mode", vec!["<primary>t"]),
        ("win.preferences", vec!["<primary>comma"]),
        // Markdown shortcuts (advertised — implemented per open question 1,
        // with Ctrl+1..6 per open question 13).
        ("win.format-bold", vec!["<primary>b"]),
        ("win.format-italic", vec!["<primary>i"]),
        ("win.format-strike", vec!["<primary><shift>d"]),
        ("win.format-rule", vec!["<primary>r"]),
        ("win.format-bullet", vec!["<primary>u"]),
        ("win.format-heading-1", vec!["<primary>1"]),
        ("win.format-heading-2", vec!["<primary>2"]),
        ("win.format-heading-3", vec!["<primary>3"]),
        ("win.format-heading-4", vec!["<primary>4"]),
        ("win.format-heading-5", vec!["<primary>5"]),
        ("win.format-heading-6", vec!["<primary>6"]),
    ] {
        app.set_accels_for_action(action, &accels);
    }
}
