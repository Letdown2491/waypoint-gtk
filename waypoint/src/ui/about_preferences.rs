use gtk::{Label, Orientation};
use gtk::prelude::*;
use libadwaita as adw;
use adw::prelude::*;

/// Show about dialog with app information
pub fn show_about_dialog(window: &adw::ApplicationWindow) {
    let dialog = adw::Window::new();
    dialog.set_title(Some("About Waypoint"));
    dialog.set_default_size(400, 380);
    dialog.set_modal(true);
    dialog.set_transient_for(Some(window));

    // Create header bar with close button
    let header = adw::HeaderBar::new();
    header.set_show_end_title_buttons(true);

    // Main container
    let main_box = gtk::Box::new(Orientation::Vertical, 0);
    main_box.append(&header);

    // Main content box
    let content = gtk::Box::new(Orientation::Vertical, 18);
    content.set_margin_top(24);
    content.set_margin_bottom(24);
    content.set_margin_start(32);
    content.set_margin_end(32);
    content.set_valign(gtk::Align::Center);

    // Application icon
    let icon = if let Ok(icon_path) = std::fs::canonicalize("assets/icons/hicolor/scalable/waypoint.svg") {
        gtk::Image::from_file(&icon_path)
    } else {
        gtk::Image::from_icon_name("waypoint")
    };
    icon.set_pixel_size(96);
    content.append(&icon);

    // Application name
    let name_label = Label::new(Some("Waypoint"));
    name_label.add_css_class("title-1");
    content.append(&name_label);

    // Version
    let version_label = Label::new(Some(&format!("Version {}", env!("CARGO_PKG_VERSION"))));
    version_label.add_css_class("dim-label");
    content.append(&version_label);

    // Description
    let description = Label::new(Some(
        "A GTK-based snapshot and rollback tool for Btrfs filesystems on Void Linux."
    ));
    description.set_wrap(true);
    description.set_justify(gtk::Justification::Center);
    description.set_max_width_chars(40);
    content.append(&description);

    // Links section
    let links_box = gtk::Box::new(Orientation::Vertical, 12);
    links_box.set_margin_top(12);

    // GitHub link
    let github_btn = gtk::Button::with_label("View on GitHub");
    github_btn.add_css_class("flat");
    github_btn.connect_clicked(|_| {
        let _ = std::process::Command::new("xdg-open")
            .arg("https://github.com/Letdown2491/waypoint-gtk/")
            .spawn();
    });
    links_box.append(&github_btn);

    // Report issue link
    let issue_btn = gtk::Button::with_label("Report an issue");
    issue_btn.add_css_class("flat");
    issue_btn.connect_clicked(|_| {
        let _ = std::process::Command::new("xdg-open")
            .arg("https://github.com/Letdown2491/waypoint-gtk/issues")
            .spawn();
    });
    links_box.append(&issue_btn);

    content.append(&links_box);

    main_box.append(&content);
    dialog.set_content(Some(&main_box));
    dialog.present();
}
