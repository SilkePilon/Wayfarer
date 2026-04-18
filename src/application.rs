//! GApplication subclass.

use adw::prelude::*;
use gtk4::prelude::*;

use crate::config;
use crate::window;

pub struct WayfarerApp {
    app: adw::Application,
}

impl WayfarerApp {
    pub fn new() -> Self {
        let app = adw::Application::builder()
            .application_id(config::APP_ID)
            .flags(gio::ApplicationFlags::FLAGS_NONE)
            .build();

        app.connect_activate(|app| {
            // If we already have a window, just present it.
            if let Some(win) = app.windows().first() {
                win.present();
                return;
            }
            window::build_window(app);
        });

        Self { app }
    }

    pub fn run(&self) -> i32 {
        self.app.run().value()
    }
}
