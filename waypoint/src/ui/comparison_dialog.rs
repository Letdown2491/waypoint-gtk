use crate::snapshot::SnapshotManager;
use adw::prelude::*;
use gtk::prelude::*;
use gtk::{Label, Orientation};
use libadwaita as adw;
use std::cell::RefCell;
use std::rc::Rc;

use super::dialogs;
use super::package_diff_dialog;

/// Show dialog to compare two snapshots
pub fn show_compare_dialog(
    window: &adw::ApplicationWindow,
    manager: &Rc<RefCell<SnapshotManager>>,
) {
    let snapshots = match manager.borrow().load_snapshots() {
        Ok(s) => s,
        Err(e) => {
            dialogs::show_error(window, "Error", &format!("Failed to load snapshots: {}", e));
            return;
        }
    };

    if snapshots.len() < 2 {
        dialogs::show_error(
            window,
            "Not Enough Snapshots",
            "You need at least 2 snapshots to compare.\n\nCreate more snapshots first.",
        );
        return;
    }

    // Create full window dialog for better control
    let dialog = adw::Window::new();
    dialog.set_title(Some("Compare Snapshots"));
    dialog.set_default_size(700, 600);
    dialog.set_modal(true);
    dialog.set_transient_for(Some(window));

    let content = gtk::Box::new(Orientation::Vertical, 0);

    // Header
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&adw::WindowTitle::new("Compare Snapshots", "")));
    content.append(&header);

    // Scrollable content
    let scrolled = gtk::ScrolledWindow::new();
    scrolled.set_vexpand(true);

    let main_box = gtk::Box::new(Orientation::Vertical, 24);
    main_box.set_margin_start(24);
    main_box.set_margin_end(24);
    main_box.set_margin_top(24);
    main_box.set_margin_bottom(24);

    // Title and description
    let title_box = gtk::Box::new(Orientation::Vertical, 6);

    let title = Label::new(Some("Select Snapshots to Compare"));
    title.add_css_class("title-2");
    title.set_halign(gtk::Align::Start);
    title_box.append(&title);

    let subtitle = Label::new(Some("Choose two snapshots to see package differences"));
    subtitle.add_css_class("dim-label");
    subtitle.set_halign(gtk::Align::Start);
    title_box.append(&subtitle);

    main_box.append(&title_box);

    // Prepare dropdown data
    let snapshot_names: Vec<String> = snapshots
        .iter()
        .map(|s| format!("{} - {}", s.name, s.format_timestamp()))
        .collect();
    let snapshot_strs: Vec<&str> = snapshot_names.iter().map(|s| s.as_str()).collect();

    // First snapshot selection
    let snap1_box = gtk::Box::new(Orientation::Vertical, 8);

    let snap1_label = Label::new(Some("First Snapshot (Baseline)"));
    snap1_label.add_css_class("caption");
    snap1_label.add_css_class("dim-label");
    snap1_label.set_halign(gtk::Align::Start);
    snap1_box.append(&snap1_label);

    let dropdown1 = gtk::DropDown::from_strings(&snapshot_strs);
    snap1_box.append(&dropdown1);

    main_box.append(&snap1_box);

    // Comparison indicator
    let arrow_box = gtk::Box::new(Orientation::Horizontal, 8);
    arrow_box.set_halign(gtk::Align::Center);
    arrow_box.set_margin_top(6);
    arrow_box.set_margin_bottom(6);

    let arrow_icon = gtk::Image::from_icon_name("go-down-symbolic");
    arrow_icon.set_pixel_size(24);
    arrow_icon.add_css_class("accent");
    arrow_box.append(&arrow_icon);

    let arrow_label = Label::new(Some("Compare"));
    arrow_label.add_css_class("title-4");
    arrow_label.add_css_class("accent");
    arrow_box.append(&arrow_label);

    main_box.append(&arrow_box);

    // Second snapshot selection
    let snap2_box = gtk::Box::new(Orientation::Vertical, 8);

    let snap2_label = Label::new(Some("Second Snapshot (Target)"));
    snap2_label.add_css_class("caption");
    snap2_label.add_css_class("dim-label");
    snap2_label.set_halign(gtk::Align::Start);
    snap2_box.append(&snap2_label);

    let dropdown2 = gtk::DropDown::from_strings(&snapshot_strs);
    // Select last snapshot by default
    if !snapshots.is_empty() {
        dropdown2.set_selected(snapshots.len() as u32 - 1);
    }
    snap2_box.append(&dropdown2);

    main_box.append(&snap2_box);

    // Error message area (hidden by default)
    let error_box = gtk::Box::new(Orientation::Horizontal, 12);
    error_box.add_css_class("error");
    error_box.set_margin_top(12);
    error_box.set_visible(false);

    let error_icon = gtk::Image::from_icon_name("dialog-warning-symbolic");
    error_box.append(&error_icon);

    let error_label = Label::new(Some("Please select two different snapshots"));
    error_label.add_css_class("caption");
    error_box.append(&error_label);

    main_box.append(&error_box);

    // Action buttons
    let button_box = gtk::Box::new(Orientation::Horizontal, 12);
    button_box.set_halign(gtk::Align::End);
    button_box.set_margin_top(12);

    let cancel_btn = gtk::Button::with_label("Cancel");
    button_box.append(&cancel_btn);

    let compare_files_btn = gtk::Button::with_label("Compare Files");
    compare_files_btn.add_css_class("pill");
    button_box.append(&compare_files_btn);

    let compare_packages_btn = gtk::Button::with_label("Compare Packages");
    compare_packages_btn.add_css_class("suggested-action");
    compare_packages_btn.add_css_class("pill");
    button_box.append(&compare_packages_btn);

    main_box.append(&button_box);

    scrolled.set_child(Some(&main_box));
    content.append(&scrolled);

    // Cancel button handler
    let dialog_clone = dialog.clone();
    cancel_btn.connect_clicked(move |_| {
        dialog_clone.close();
    });

    // Compare Packages button handler
    let window_clone = window.clone();
    let snapshots_clone = snapshots.clone();
    let dialog_clone = dialog.clone();
    let error_box_clone = error_box.clone();
    let dropdown1_clone = dropdown1.clone();
    let dropdown2_clone = dropdown2.clone();

    compare_packages_btn.connect_clicked(move |_| {
        let idx1 = dropdown1_clone.selected() as usize;
        let idx2 = dropdown2_clone.selected() as usize;

        if idx1 == idx2 {
            error_box_clone.set_visible(true);
            return;
        }

        error_box_clone.set_visible(false);

        let snap1 = &snapshots_clone[idx1];
        let snap2 = &snapshots_clone[idx2];

        // Show the comparison
        package_diff_dialog::show_package_diff_dialog(
            &window_clone,
            &snap1.name,
            &snap1.packages,
            &snap2.name,
            &snap2.packages,
        );

        dialog_clone.close();
    });

    // Compare Files button handler
    let window_clone2 = window.clone();
    let snapshots_clone2 = snapshots.clone();
    let dialog_clone2 = dialog.clone();
    let error_box_clone2 = error_box.clone();

    compare_files_btn.connect_clicked(move |_| {
        let idx1 = dropdown1.selected() as usize;
        let idx2 = dropdown2.selected() as usize;

        if idx1 == idx2 {
            error_box_clone2.set_visible(true);
            return;
        }

        error_box_clone2.set_visible(false);

        let snap1 = &snapshots_clone2[idx1];
        let snap2 = &snapshots_clone2[idx2];

        // Show the file diff dialog
        super::file_diff_dialog::show_file_diff_dialog(&window_clone2, &snap1.name, &snap2.name);

        dialog_clone2.close();
    });

    dialog.set_content(Some(&content));
    dialog.present();
}
