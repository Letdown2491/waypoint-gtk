mod btrfs;
mod cache;
mod dbus_client;
mod packages;
mod retention;
mod signal_listener;
mod snapshot;
mod subvolume;
mod ui;

use gtk::prelude::*;
use gtk::{glib, Application};

const APP_ID: &str = "tech.geektoshi.waypoint";

fn main() -> glib::ExitCode {
    // Initialize GTK
    let app = Application::builder()
        .application_id(APP_ID)
        .build();

    app.connect_activate(build_ui);
    app.run()
}

fn build_ui(app: &Application) {
    // Initialize filesystem cache
    btrfs::init_cache();

    // Start D-Bus signal listener for snapshot creation events
    signal_listener::start_signal_listener(app.clone());

    let window = ui::MainWindow::new(app);
    window.present();
}
