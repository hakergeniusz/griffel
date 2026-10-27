//! Application object: single instance, actions, accelerators, file open
//! routing.

use gtk::prelude::*;
use gtk::gio;
use gtk::glib;

use crate::window::AppWindow;

pub const APP_ID: &str = "io.github.hakergeniusz.Griffel";
pub const ICON_NAME: &str = APP_ID;

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
        });

        app.connect_activate(|app| {
            // Every activation creates a new empty document window
            // (`03-behavior/12-windows.md` §1). Snapshot recovery happens
            // once, on the first activation of the primary instance.
            if !app.is_remote() {
                crate::files::recover_snapshots(app);
            }
            let win = AppWindow::new(app);
            win.present_window();
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
    ] {
        app.set_accels_for_action(action, &accels);
    }
}
