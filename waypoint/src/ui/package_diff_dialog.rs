use crate::packages::{diff_packages, Package};
use gtk::prelude::*;
use gtk::{Label, Orientation, ScrolledWindow};
use libadwaita as adw;
use adw::prelude::*;

/// Show a package diff dialog comparing two snapshots
pub fn show_package_diff_dialog(
    parent: &adw::ApplicationWindow,
    snapshot1_name: &str,
    snapshot1_packages: &[Package],
    snapshot2_name: &str,
    snapshot2_packages: &[Package],
) {
    let diff = diff_packages(snapshot1_packages, snapshot2_packages);

    let dialog = adw::Window::new();
    dialog.set_title(Some("Package Comparison"));
    dialog.set_default_size(700, 600);
    dialog.set_modal(true);
    dialog.set_transient_for(Some(parent));

    // Main content
    let content = gtk::Box::new(Orientation::Vertical, 0);

    // Header
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&adw::WindowTitle::new("Package Comparison", "")));
    content.append(&header);

    // Title with snapshot names
    let title_box = gtk::Box::new(Orientation::Vertical, 6);
    title_box.set_margin_top(12);
    title_box.set_margin_start(12);
    title_box.set_margin_end(12);

    let title = Label::new(Some(&format!(
        "Comparing: {} → {}",
        snapshot1_name, snapshot2_name
    )));
    title.add_css_class("title-2");
    title_box.append(&title);

    let summary = Label::new(Some(&format!(
        "{} changes: {} added, {} removed, {} updated",
        diff.added.len() + diff.removed.len() + diff.updated.len(),
        diff.added.len(),
        diff.removed.len(),
        diff.updated.len()
    )));
    summary.add_css_class("dim-label");
    title_box.append(&summary);

    content.append(&title_box);

    // Scrollable content
    let scrolled = ScrolledWindow::new();
    scrolled.set_vexpand(true);
    scrolled.set_margin_top(12);
    scrolled.set_margin_start(12);
    scrolled.set_margin_end(12);
    scrolled.set_margin_bottom(12);

    let list_box = gtk::Box::new(Orientation::Vertical, 12);

    // Added packages
    if !diff.added.is_empty() {
        let added_section = create_section(
            "📦 Packages Added",
            &diff
                .added
                .iter()
                .map(|p| format!("• {} ({})", p.name, p.version))
                .collect::<Vec<_>>(),
            "success",
        );
        list_box.append(&added_section);
    }

    // Removed packages
    if !diff.removed.is_empty() {
        let removed_section = create_section(
            "📦 Packages Removed",
            &diff
                .removed
                .iter()
                .map(|p| format!("• {} ({})", p.name, p.version))
                .collect::<Vec<_>>(),
            "error",
        );
        list_box.append(&removed_section);
    }

    // Updated packages
    if !diff.updated.is_empty() {
        let updated_section = create_section(
            "📦 Packages Updated",
            &diff
                .updated
                .iter()
                .map(|p| format!("• {} ({} → {})", p.name, p.old_version, p.new_version))
                .collect::<Vec<_>>(),
            "accent",
        );
        list_box.append(&updated_section);
    }

    // No changes
    if diff.added.is_empty() && diff.removed.is_empty() && diff.updated.is_empty() {
        let status = adw::StatusPage::new();
        status.set_title("No Differences");
        status.set_description(Some("Both snapshots have identical packages"));
        status.set_icon_name(Some("emblem-ok-symbolic"));
        list_box.append(&status);
    }

    scrolled.set_child(Some(&list_box));
    content.append(&scrolled);

    dialog.set_content(Some(&content));
    dialog.present();
}

/// Create a section with a title and list of items
fn create_section(title: &str, items: &[String], style: &str) -> gtk::Box {
    let section = gtk::Box::new(Orientation::Vertical, 6);

    let title_label = Label::new(Some(title));
    title_label.set_halign(gtk::Align::Start);
    title_label.add_css_class("title-4");
    section.append(&title_label);

    let card = gtk::Box::new(Orientation::Vertical, 0);
    card.add_css_class("card");

    for (i, item) in items.iter().enumerate() {
        let row = gtk::Box::new(Orientation::Horizontal, 12);
        row.set_margin_top(8);
        row.set_margin_bottom(8);
        row.set_margin_start(12);
        row.set_margin_end(12);

        let label = Label::new(Some(item));
        label.set_halign(gtk::Align::Start);
        label.set_wrap(true);
        label.set_wrap_mode(gtk::pango::WrapMode::WordChar);

        match style {
            "success" => label.add_css_class("success"),
            "error" => label.add_css_class("error"),
            "accent" => label.add_css_class("accent"),
            _ => {}
        }

        row.append(&label);
        card.append(&row);

        // Add separator between items (except last)
        if i < items.len() - 1 {
            let separator = gtk::Separator::new(Orientation::Horizontal);
            card.append(&separator);
        }
    }

    section.append(&card);
    section
}
