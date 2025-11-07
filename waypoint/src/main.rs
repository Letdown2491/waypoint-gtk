mod btrfs;
mod dbus_client;
mod packages;
mod retention;
mod snapshot;
mod subvolume;
mod ui;

use gtk::prelude::*;
use gtk::{glib, Application};

const APP_ID: &str = "com.voidlinux.Waypoint";

fn main() -> glib::ExitCode {
    // Initialize GTK
    let app = Application::builder()
        .application_id(APP_ID)
        .build();

    app.connect_activate(build_ui);
    app.run()
}

fn build_ui(app: &Application) {
    let window = ui::MainWindow::new(app);
    window.present();
}
