//! Package diff dialog for comparing packages between two snapshots
//! NOTE: This is the old implementation, kept for reference.
//! The new implementation uses comparison_view.rs with NavigationView.

#![allow(dead_code)]

use crate::packages::{Package, PackageDiff, diff_packages};
use adw::prelude::*;
use gtk::prelude::*;
use gtk::{Label, Orientation, ScrolledWindow};
use libadwaita as adw;
use std::cell::RefCell;
use std::rc::Rc;

/// Filter type for package changes
#[derive(Debug, Clone, Copy, PartialEq)]
enum ChangeFilter {
    All,
    Added,
    Removed,
    Updated,
}

/// Package display item for enhanced UI
struct PackageDisplayItem {
    name: String,
    version_info: String,
    change_type: &'static str,
}

/// Show a package diff dialog comparing two snapshots
pub fn show_package_diff_dialog(
    parent: &adw::ApplicationWindow,
    snapshot1_name: &str,
    snapshot1_packages: &[Package],
    snapshot2_name: &str,
    snapshot2_packages: &[Package],
) {
    let diff = diff_packages(snapshot1_packages, snapshot2_packages);
    let diff_rc = Rc::new(diff);

    let dialog = adw::Window::new();
    dialog.set_title(Some("Package Comparison"));
    dialog.set_default_size(900, 750);
    dialog.set_modal(true);
    dialog.set_transient_for(Some(parent));

    // Main content
    let content = gtk::Box::new(Orientation::Vertical, 0);

    // Header with export button
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&adw::WindowTitle::new("Package Comparison", "")));

    let export_btn = gtk::Button::from_icon_name("document-save-symbolic");
    export_btn.set_tooltip_text(Some("Export to file"));
    header.pack_end(&export_btn);

    content.append(&header);

    // Title with snapshot names - improved visual hierarchy
    let title_section = gtk::Box::new(Orientation::Vertical, 12);
    title_section.set_margin_top(24);
    title_section.set_margin_start(24);
    title_section.set_margin_end(24);
    title_section.set_margin_bottom(18);

    let comparison_header = gtk::Box::new(Orientation::Horizontal, 12);
    comparison_header.set_halign(gtk::Align::Center);

    let snap1_label = Label::new(Some(snapshot1_name));
    snap1_label.add_css_class("title-3");
    snap1_label.add_css_class("accent");
    comparison_header.append(&snap1_label);

    let arrow_icon = gtk::Image::from_icon_name("go-next-symbolic");
    arrow_icon.set_pixel_size(20);
    arrow_icon.add_css_class("dim-label");
    comparison_header.append(&arrow_icon);

    let snap2_label = Label::new(Some(snapshot2_name));
    snap2_label.add_css_class("title-3");
    snap2_label.add_css_class("success");
    comparison_header.append(&snap2_label);

    title_section.append(&comparison_header);
    content.append(&title_section);

    // Filter and search controls
    let controls_box = gtk::Box::new(Orientation::Vertical, 12);
    controls_box.set_margin_top(0);
    controls_box.set_margin_start(24);
    controls_box.set_margin_end(24);
    controls_box.set_margin_bottom(18);

    // Filter buttons - professional style without emoji
    let filter_box = gtk::Box::new(Orientation::Horizontal, 6);
    filter_box.set_halign(gtk::Align::Start);

    let total_changes = diff_rc.added.len() + diff_rc.removed.len() + diff_rc.updated.len();
    let btn_all = gtk::ToggleButton::with_label(&format!("All ({})", total_changes));
    btn_all.set_active(true);
    btn_all.add_css_class("pill");
    filter_box.append(&btn_all);

    let btn_added = gtk::ToggleButton::with_label(&format!("Added ({})", diff_rc.added.len()));
    btn_added.add_css_class("pill");
    filter_box.append(&btn_added);

    let btn_removed =
        gtk::ToggleButton::with_label(&format!("Removed ({})", diff_rc.removed.len()));
    btn_removed.add_css_class("pill");
    filter_box.append(&btn_removed);

    let btn_updated =
        gtk::ToggleButton::with_label(&format!("Updated ({})", diff_rc.updated.len()));
    btn_updated.add_css_class("pill");
    filter_box.append(&btn_updated);

    controls_box.append(&filter_box);

    // Search entry - below filters
    let search_entry = gtk::SearchEntry::new();
    search_entry.set_placeholder_text(Some("Search packages..."));
    search_entry.set_margin_top(6);
    controls_box.append(&search_entry);

    content.append(&controls_box);

    // Scrollable content
    let scrolled = ScrolledWindow::new();
    scrolled.set_vexpand(true);
    scrolled.set_margin_top(0);
    scrolled.set_margin_start(24);
    scrolled.set_margin_end(24);
    scrolled.set_margin_bottom(0);

    let list_box = gtk::Box::new(Orientation::Vertical, 12);
    scrolled.set_child(Some(&list_box));
    content.append(&scrolled);

    // Status bar showing results
    let status_bar = gtk::Box::new(Orientation::Horizontal, 12);
    status_bar.set_margin_top(12);
    status_bar.set_margin_bottom(18);
    status_bar.set_margin_start(24);
    status_bar.set_margin_end(24);
    status_bar.set_halign(gtk::Align::Center);

    let status_label = Label::new(Some(""));
    status_label.add_css_class("dim-label");
    status_label.add_css_class("caption");
    status_bar.append(&status_label);

    content.append(&status_bar);

    // Current filter state
    let current_filter = Rc::new(RefCell::new(ChangeFilter::All));
    let search_text = Rc::new(RefCell::new(String::new()));

    // Function to refresh the list based on current filters
    let refresh_list = {
        let list_box = list_box.clone();
        let diff_rc = diff_rc.clone();
        let current_filter = current_filter.clone();
        let search_text = search_text.clone();
        let status_label = status_label.clone();

        Rc::new(move || {
            // Clear existing items
            while let Some(child) = list_box.first_child() {
                list_box.remove(&child);
            }

            let filter = *current_filter.borrow();
            let search = search_text.borrow().to_lowercase();

            // Filter packages based on search
            let filter_pkg =
                |pkg: &Package| search.is_empty() || pkg.name.to_lowercase().contains(&search);

            let filter_update = |upd: &crate::packages::PackageUpdate| {
                search.is_empty() || upd.name.to_lowercase().contains(&search)
            };

            // Determine which sections to show
            let show_added = filter == ChangeFilter::All || filter == ChangeFilter::Added;
            let show_removed = filter == ChangeFilter::All || filter == ChangeFilter::Removed;
            let show_updated = filter == ChangeFilter::All || filter == ChangeFilter::Updated;

            let mut shown_items = 0;

            // Added packages
            if show_added && !diff_rc.added.is_empty() {
                let filtered_added: Vec<_> =
                    diff_rc.added.iter().filter(|p| filter_pkg(p)).collect();
                if !filtered_added.is_empty() {
                    let added_section = create_enhanced_section(
                        "Packages Added",
                        &filtered_added
                            .iter()
                            .map(|p| PackageDisplayItem {
                                name: p.name.clone(),
                                version_info: p.version.clone(),
                                change_type: "added",
                            })
                            .collect::<Vec<_>>(),
                    );
                    list_box.append(&added_section);
                    shown_items += filtered_added.len();
                }
            }

            // Removed packages
            if show_removed && !diff_rc.removed.is_empty() {
                let filtered_removed: Vec<_> =
                    diff_rc.removed.iter().filter(|p| filter_pkg(p)).collect();
                if !filtered_removed.is_empty() {
                    let removed_section = create_enhanced_section(
                        "Packages Removed",
                        &filtered_removed
                            .iter()
                            .map(|p| PackageDisplayItem {
                                name: p.name.clone(),
                                version_info: p.version.clone(),
                                change_type: "removed",
                            })
                            .collect::<Vec<_>>(),
                    );
                    list_box.append(&removed_section);
                    shown_items += filtered_removed.len();
                }
            }

            // Updated packages
            if show_updated && !diff_rc.updated.is_empty() {
                let filtered_updated: Vec<_> = diff_rc
                    .updated
                    .iter()
                    .filter(|u| filter_update(u))
                    .collect();
                if !filtered_updated.is_empty() {
                    let updated_section = create_enhanced_section(
                        "Packages Updated",
                        &filtered_updated
                            .iter()
                            .map(|p| PackageDisplayItem {
                                name: p.name.clone(),
                                version_info: format!("{} → {}", p.old_version, p.new_version),
                                change_type: "updated",
                            })
                            .collect::<Vec<_>>(),
                    );
                    list_box.append(&updated_section);
                    shown_items += filtered_updated.len();
                }
            }

            // No results
            if shown_items == 0 {
                let status = adw::StatusPage::new();
                if diff_rc.added.is_empty()
                    && diff_rc.removed.is_empty()
                    && diff_rc.updated.is_empty()
                {
                    status.set_title("No Differences");
                    status.set_description(Some("Both snapshots have identical packages"));
                    status.set_icon_name(Some("emblem-ok-symbolic"));
                } else {
                    status.set_title("No Matching Packages");
                    status.set_description(Some("Try adjusting your search or filter"));
                    status.set_icon_name(Some("edit-find-symbolic"));
                }
                list_box.append(&status);
            }

            // Update status label
            let total_items = diff_rc.added.len() + diff_rc.removed.len() + diff_rc.updated.len();
            if shown_items == total_items {
                if total_items == 0 {
                    status_label.set_text("No package differences");
                } else {
                    status_label.set_text(&format!("Showing all {} package changes", total_items));
                }
            } else if shown_items == 0 {
                status_label.set_text("No packages match current filters");
            } else {
                status_label.set_text(&format!(
                    "Showing {} of {} package changes",
                    shown_items, total_items
                ));
            }
        })
    };

    // Initial population
    refresh_list();

    // Filter button handlers - make mutually exclusive
    let refresh_for_all = refresh_list.clone();
    let current_filter_for_all = current_filter.clone();
    btn_all.connect_toggled(move |btn| {
        if btn.is_active() {
            *current_filter_for_all.borrow_mut() = ChangeFilter::All;
            refresh_for_all();
        }
    });

    let refresh_for_added = refresh_list.clone();
    let current_filter_for_added = current_filter.clone();
    let btn_all_for_added = btn_all.clone();
    btn_added.connect_toggled(move |btn| {
        if btn.is_active() {
            *current_filter_for_added.borrow_mut() = ChangeFilter::Added;
            btn_all_for_added.set_active(false);
            refresh_for_added();
        }
    });

    let refresh_for_removed = refresh_list.clone();
    let current_filter_for_removed = current_filter.clone();
    let btn_all_for_removed = btn_all.clone();
    btn_removed.connect_toggled(move |btn| {
        if btn.is_active() {
            *current_filter_for_removed.borrow_mut() = ChangeFilter::Removed;
            btn_all_for_removed.set_active(false);
            refresh_for_removed();
        }
    });

    let refresh_for_updated = refresh_list.clone();
    let current_filter_for_updated = current_filter.clone();
    let btn_all_for_updated = btn_all.clone();
    btn_updated.connect_toggled(move |btn| {
        if btn.is_active() {
            *current_filter_for_updated.borrow_mut() = ChangeFilter::Updated;
            btn_all_for_updated.set_active(false);
            refresh_for_updated();
        }
    });

    // Search handler
    let refresh_for_search = refresh_list.clone();
    let search_text_for_handler = search_text.clone();
    search_entry.connect_search_changed(move |entry| {
        *search_text_for_handler.borrow_mut() = entry.text().to_string();
        refresh_for_search();
    });

    // Export button handler
    let diff_for_export = diff_rc.clone();
    let dialog_for_export = dialog.clone();
    let snap1 = snapshot1_name.to_string();
    let snap2 = snapshot2_name.to_string();
    export_btn.connect_clicked(move |_| {
        export_comparison(&dialog_for_export, &snap1, &snap2, &diff_for_export);
    });

    dialog.set_content(Some(&content));
    dialog.present();
}

/// Create an enhanced section with better visual design
fn create_enhanced_section(title: &str, items: &[PackageDisplayItem]) -> gtk::Box {
    let section = gtk::Box::new(Orientation::Vertical, 12);
    section.set_margin_bottom(18);

    // Section header with icon and count - improved spacing and hierarchy
    let header_box = gtk::Box::new(Orientation::Horizontal, 12);
    header_box.set_margin_start(4);
    header_box.set_margin_bottom(12);

    let (icon_name, style_class) = match items.first().map(|i| i.change_type) {
        Some("added") => ("list-add-symbolic", "success"),
        Some("removed") => ("list-remove-symbolic", "error"),
        Some("updated") => ("view-refresh-symbolic", "warning"),
        _ => ("package-x-generic-symbolic", ""),
    };

    let icon = gtk::Image::from_icon_name(icon_name);
    icon.set_pixel_size(18);
    icon.add_css_class(style_class);
    header_box.append(&icon);

    let title_label = Label::new(Some(title));
    title_label.set_halign(gtk::Align::Start);
    title_label.add_css_class("title-4");
    header_box.append(&title_label);

    let count_label = Label::new(Some(&format!("({})", items.len())));
    count_label.add_css_class("caption");
    count_label.add_css_class("dim-label");
    count_label.set_halign(gtk::Align::Start);
    header_box.append(&count_label);

    section.append(&header_box);

    // Package list in a card
    let card = gtk::ListBox::new();
    card.add_css_class("boxed-list");
    card.set_selection_mode(gtk::SelectionMode::None);

    for item in items {
        let row = adw::ActionRow::new();
        row.set_title(&item.name);
        row.set_subtitle(&item.version_info);

        // Add icon based on change type
        let icon = match item.change_type {
            "added" => gtk::Image::from_icon_name("emblem-ok-symbolic"),
            "removed" => gtk::Image::from_icon_name("user-trash-symbolic"),
            "updated" => gtk::Image::from_icon_name("emblem-synchronizing-symbolic"),
            _ => gtk::Image::from_icon_name("package-x-generic-symbolic"),
        };
        icon.add_css_class("dim-label");
        row.add_prefix(&icon);

        // Add copy button
        let copy_btn = gtk::Button::from_icon_name("edit-copy-symbolic");
        copy_btn.set_valign(gtk::Align::Center);
        copy_btn.add_css_class("flat");
        copy_btn.add_css_class("circular");
        copy_btn.set_tooltip_text(Some("Copy package name"));

        let pkg_name = item.name.clone();
        copy_btn.connect_clicked(move |_| {
            if let Some(display) = gtk::gdk::Display::default() {
                display.clipboard().set_text(&pkg_name);
            }
        });

        row.add_suffix(&copy_btn);
        card.append(&row);
    }

    section.append(&card);
    section
}

/// Export comparison to a text file
fn export_comparison(parent: &adw::Window, snap1: &str, snap2: &str, diff: &PackageDiff) {
    use std::fs;

    let filename = format!("waypoint_comparison_{}_{}.txt", snap1, snap2).replace('/', "_");

    let mut content = String::new();
    content.push_str(&format!("Package Comparison: {} → {}\n", snap1, snap2));
    content.push_str(&format!(
        "Generated: {}\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
    ));
    content.push_str(&format!("\nSummary:\n"));
    content.push_str(&format!("  Total Changes: {}\n", diff.total_changes()));
    content.push_str(&format!("  Added: {}\n", diff.added.len()));
    content.push_str(&format!("  Removed: {}\n", diff.removed.len()));
    content.push_str(&format!("  Updated: {}\n", diff.updated.len()));

    if !diff.added.is_empty() {
        content.push_str("\n=== Packages Added ===\n");
        for pkg in &diff.added {
            content.push_str(&format!("  + {} ({})\n", pkg.name, pkg.version));
        }
    }

    if !diff.removed.is_empty() {
        content.push_str("\n=== Packages Removed ===\n");
        for pkg in &diff.removed {
            content.push_str(&format!("  - {} ({})\n", pkg.name, pkg.version));
        }
    }

    if !diff.updated.is_empty() {
        content.push_str("\n=== Packages Updated ===\n");
        for upd in &diff.updated {
            content.push_str(&format!(
                "  * {} ({} → {})\n",
                upd.name, upd.old_version, upd.new_version
            ));
        }
    }

    // Try to save to Downloads folder
    let save_path = if let Some(home) = std::env::var_os("HOME") {
        let downloads = std::path::PathBuf::from(home)
            .join("Downloads")
            .join(&filename);
        if downloads.parent().unwrap().exists() {
            downloads
        } else {
            std::path::PathBuf::from("/tmp").join(&filename)
        }
    } else {
        std::path::PathBuf::from("/tmp").join(&filename)
    };

    match fs::write(&save_path, content) {
        Ok(_) => {
            let success_dialog = adw::MessageDialog::new(
                Some(parent),
                Some("Export Successful"),
                Some(&format!("Comparison exported to:\n{}", save_path.display())),
            );
            success_dialog.add_response("ok", "OK");
            success_dialog.set_default_response(Some("ok"));
            success_dialog.present();
        }
        Err(e) => {
            let error_dialog = adw::MessageDialog::new(
                Some(parent),
                Some("Export Failed"),
                Some(&format!("Failed to export comparison: {}", e)),
            );
            error_dialog.add_response("ok", "OK");
            error_dialog.set_default_response(Some("ok"));
            error_dialog.present();
        }
    }
}
