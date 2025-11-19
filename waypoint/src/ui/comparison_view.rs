use adw::prelude::*;
use crate::dbus_client::WaypointHelperClient;
use crate::packages::{diff_packages, PackageDiff};
use crate::snapshot::Snapshot;
use gtk::prelude::*;
use gtk::{Box, Button, ListBox, Orientation, ScrolledWindow};
use libadwaita as adw;
use serde::Deserialize;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;

/// File change representation (matches waypoint-helper output)
#[derive(Debug, Clone, Deserialize)]
struct FileChange {
    change_type: String, // "Added", "Modified", "Deleted"
    path: String,
}

/// Comparison view with navigation between selection, package diff, and file diff
pub struct ComparisonView {
    /// Main navigation view widget
    pub widget: adw::NavigationView,
}

impl ComparisonView {
    /// Create a new comparison view with loaded snapshots
    pub fn new(snapshots: Vec<Snapshot>) -> Self {
        let nav_view = adw::NavigationView::new();

        // Create selection page
        let selection_page = Self::create_selection_page(snapshots, nav_view.clone());
        nav_view.add(&selection_page);

        Self { widget: nav_view }
    }

    /// Create the selection page
    fn create_selection_page(
        snapshots: Vec<Snapshot>,
        nav_view: adw::NavigationView,
    ) -> adw::NavigationPage {
        let page = adw::NavigationPage::new(&adw::ToolbarView::new(), "Compare Snapshots");

        let toolbar_view = page.child().and_downcast::<adw::ToolbarView>().unwrap();

        // Header bar
        let header = adw::HeaderBar::new();
        toolbar_view.add_top_bar(&header);

        // Content
        let content = Box::new(Orientation::Vertical, 24);
        content.set_margin_top(24);
        content.set_margin_bottom(24);
        content.set_margin_start(24);
        content.set_margin_end(24);
        content.set_valign(gtk::Align::Start);

        // Snapshot selection section
        let selection_box = Box::new(Orientation::Vertical, 12);

        // Base snapshot dropdown
        let base_group = adw::PreferencesGroup::new();
        base_group.set_title("Base Snapshot");

        let base_row = adw::ComboRow::new();
        base_row.set_title("Select base snapshot");

        let base_model = gtk::StringList::new(&[]);
        for snapshot in &snapshots {
            let display = format!(
                "{} ({})",
                snapshot.name,
                snapshot.timestamp.format("%Y-%m-%d %H:%M")
            );
            base_model.append(&display);
        }
        base_row.set_model(Some(&base_model));

        base_group.add(&base_row);
        selection_box.append(&base_group);

        // Compare snapshot dropdown
        let compare_group = adw::PreferencesGroup::new();
        compare_group.set_title("Compare To");

        let compare_row = adw::ComboRow::new();
        compare_row.set_title("Select snapshot to compare");

        let compare_model = gtk::StringList::new(&[]);
        // Initially populate with all snapshots except the first (which is selected in base)
        for (idx, snapshot) in snapshots.iter().enumerate() {
            if idx != 0 {
                let display = format!(
                    "{} ({})",
                    snapshot.name,
                    snapshot.timestamp.format("%Y-%m-%d %H:%M")
                );
                compare_model.append(&display);
            }
        }
        compare_row.set_model(Some(&compare_model));

        compare_group.add(&compare_row);
        selection_box.append(&compare_group);

        content.append(&selection_box);

        // Summary section (initially hidden)
        let summary_group = adw::PreferencesGroup::new();
        summary_group.set_title("Summary");
        summary_group.set_visible(false);

        let summary_list = ListBox::new();
        summary_list.add_css_class("boxed-list");

        let packages_row = adw::ActionRow::new();
        packages_row.set_title("Package Changes");
        packages_row.set_subtitle("Select snapshots to compare");
        summary_list.append(&packages_row);

        let files_row = adw::ActionRow::new();
        files_row.set_title("File Changes");
        files_row.set_subtitle("Not available");
        summary_list.append(&files_row);

        summary_group.add(&summary_list);
        content.append(&summary_group);

        // Action buttons
        let button_box = Box::new(Orientation::Horizontal, 12);
        button_box.set_halign(gtk::Align::Center);
        button_box.set_margin_top(12);

        let view_packages_button = Button::with_label("View Packages");
        view_packages_button.add_css_class("pill");
        view_packages_button.add_css_class("suggested-action");
        view_packages_button.set_visible(false);

        let view_files_button = Button::with_label("View Files");
        view_files_button.add_css_class("pill");
        view_files_button.set_visible(false);

        button_box.append(&view_packages_button);
        button_box.append(&view_files_button);

        content.append(&button_box);

        // Store snapshots for comparison
        let snapshots = Rc::new(snapshots);
        let current_diff: Rc<RefCell<Option<PackageDiff>>> = Rc::new(RefCell::new(None));

        // Mapping from compare dropdown indices to actual snapshot indices
        let compare_mapping: Rc<RefCell<Vec<usize>>> = Rc::new(RefCell::new(
            (1..snapshots.len()).collect()
        ));

        // Handle selection changes
        let snapshots_for_base = snapshots.clone();
        let snapshots_for_compare = snapshots.clone();
        let base_row_clone = base_row.clone();
        let compare_row_clone = compare_row.clone();
        let summary_group_clone = summary_group.clone();
        let packages_row_clone = packages_row.clone();
        let view_packages_clone = view_packages_button.clone();
        let view_files_clone = view_files_button.clone();
        let diff_for_base = current_diff.clone();
        let mapping_for_compare = compare_mapping.clone();

        let update_comparison = move || {
            let base_idx = base_row_clone.selected() as usize;
            let compare_dropdown_idx = compare_row_clone.selected() as usize;

            if base_idx >= snapshots_for_base.len() {
                return;
            }

            // Map compare dropdown index to actual snapshot index
            let mapping = mapping_for_compare.borrow();
            let compare_idx = match mapping.get(compare_dropdown_idx) {
                Some(&idx) => idx,
                None => {
                    summary_group_clone.set_visible(false);
                    view_packages_clone.set_visible(false);
                    view_files_clone.set_visible(false);
                    return;
                }
            };

            if base_idx == compare_idx {
                summary_group_clone.set_visible(false);
                view_packages_clone.set_visible(false);
                view_files_clone.set_visible(false);
                return;
            }

            let snap1 = &snapshots_for_base[base_idx];
            let snap2 = &snapshots_for_compare[compare_idx];

            // Compute package diff
            let diff = diff_packages(&snap1.packages, &snap2.packages);

            packages_row_clone.set_subtitle(&format!(
                "{} added, {} removed, {} changed",
                diff.added.len(),
                diff.removed.len(),
                diff.updated.len()
            ));

            // Store diff for later use
            *diff_for_base.borrow_mut() = Some(diff);

            summary_group_clone.set_visible(true);
            view_packages_clone.set_visible(true);
            view_files_clone.set_visible(true);
        };

        // When base selection changes, rebuild compare dropdown
        let snapshots_for_base_change = snapshots.clone();
        let compare_model_for_base = compare_model.clone();
        let compare_row_for_base = compare_row.clone();
        let mapping_for_base = compare_mapping.clone();
        let update_for_base = update_comparison.clone();

        base_row.connect_selected_notify(move |row| {
            let base_idx = row.selected() as usize;

            // Rebuild compare dropdown excluding the selected base snapshot
            // First, remove all items
            while compare_model_for_base.n_items() > 0 {
                compare_model_for_base.remove(0);
            }

            // Then add all snapshots except the selected base
            let mut new_mapping = Vec::new();
            for (idx, snapshot) in snapshots_for_base_change.iter().enumerate() {
                if idx != base_idx {
                    let display = format!(
                        "{} ({})",
                        snapshot.name,
                        snapshot.timestamp.format("%Y-%m-%d %H:%M")
                    );
                    compare_model_for_base.append(&display);
                    new_mapping.push(idx);
                }
            }

            *mapping_for_base.borrow_mut() = new_mapping;

            // Select first item if available
            if compare_model_for_base.n_items() > 0 {
                compare_row_for_base.set_selected(0);
            }

            update_for_base();
        });

        let update_for_compare = update_comparison.clone();
        compare_row.connect_selected_notify(move |_| {
            update_for_compare();
        });

        // Handle view packages button
        let nav_view_for_packages = nav_view.clone();
        let diff_for_packages = current_diff.clone();
        let snapshots_for_btn = snapshots.clone();
        let base_row_for_btn = base_row.clone();
        let compare_row_for_btn = compare_row.clone();
        let mapping_for_packages = compare_mapping.clone();

        view_packages_button.connect_clicked(move |_| {
            if let Some(diff) = diff_for_packages.borrow().clone() {
                let base_idx = base_row_for_btn.selected() as usize;
                let compare_dropdown_idx = compare_row_for_btn.selected() as usize;

                let mapping = mapping_for_packages.borrow();
                if let Some(&compare_idx) = mapping.get(compare_dropdown_idx) {
                    let snap1 = &snapshots_for_btn[base_idx];
                    let snap2 = &snapshots_for_btn[compare_idx];

                    let package_page =
                        Self::create_package_diff_page(&snap1.name, &snap2.name, &diff);
                    nav_view_for_packages.push(&package_page);
                }
            }
        });

        // Handle view files button
        let nav_view_for_files = nav_view.clone();
        let snapshots_for_files = snapshots.clone();
        let base_row_for_files = base_row.clone();
        let compare_row_for_files = compare_row.clone();
        let mapping_for_files = compare_mapping.clone();

        view_files_button.connect_clicked(move |_| {
            let base_idx = base_row_for_files.selected() as usize;
            let compare_dropdown_idx = compare_row_for_files.selected() as usize;

            let mapping = mapping_for_files.borrow();
            if let Some(&compare_idx) = mapping.get(compare_dropdown_idx) {
                let snap1 = &snapshots_for_files[base_idx];
                let snap2 = &snapshots_for_files[compare_idx];

                let file_page = Self::create_file_diff_page(&snap1.name, &snap2.name, nav_view_for_files.clone());
                nav_view_for_files.push(&file_page);
            }
        });

        let scrolled = ScrolledWindow::new();
        scrolled.set_child(Some(&content));
        toolbar_view.set_content(Some(&scrolled));

        page
    }

    /// Create package diff page
    fn create_package_diff_page(
        snap1_name: &str,
        snap2_name: &str,
        diff: &PackageDiff,
    ) -> adw::NavigationPage {
        let page = adw::NavigationPage::new(&adw::ToolbarView::new(), "Package Differences");

        let toolbar_view = page.child().and_downcast::<adw::ToolbarView>().unwrap();

        let header = adw::HeaderBar::new();

        // Add export button
        let export_btn = gtk::Button::from_icon_name("document-save-symbolic");
        export_btn.set_tooltip_text(Some("Export comparison to text file"));

        let snap1_for_export = snap1_name.to_string();
        let snap2_for_export = snap2_name.to_string();
        let diff_for_export = diff.clone();

        export_btn.connect_clicked(move |_| {
            Self::export_package_comparison(&snap1_for_export, &snap2_for_export, &diff_for_export);
        });

        header.pack_end(&export_btn);
        toolbar_view.add_top_bar(&header);

        let content = Box::new(Orientation::Vertical, 12);
        content.set_margin_top(12);
        content.set_margin_bottom(12);
        content.set_margin_start(12);
        content.set_margin_end(12);

        // Title showing comparison
        let title_box = Box::new(Orientation::Horizontal, 12);
        title_box.set_halign(gtk::Align::Center);
        title_box.set_margin_bottom(12);

        let snap1_label = gtk::Label::new(Some(snap1_name));
        snap1_label.add_css_class("title-3");
        title_box.append(&snap1_label);

        let arrow = gtk::Image::from_icon_name("go-next-symbolic");
        arrow.add_css_class("dim-label");
        title_box.append(&arrow);

        let snap2_label = gtk::Label::new(Some(snap2_name));
        snap2_label.add_css_class("title-3");
        title_box.append(&snap2_label);

        content.append(&title_box);

        // Check if there are any changes at all
        if diff.added.is_empty() && diff.removed.is_empty() && diff.updated.is_empty() {
            let status_page = adw::StatusPage::new();
            status_page.set_icon_name(Some("emblem-ok-symbolic"));
            status_page.set_title("Identical Package Sets");
            status_page.set_description(Some(
                "Both snapshots have the exact same packages installed.\n\n\
                 This means no packages were added, removed, or updated between these snapshots."
            ));
            content.append(&status_page);
        } else {
            // Added packages
            if !diff.added.is_empty() {
                let added_group = adw::PreferencesGroup::new();
                added_group.set_title(&format!("Added Packages ({})", diff.added.len()));

                let added_list = ListBox::new();
                added_list.add_css_class("boxed-list");

                for pkg in &diff.added {
                    let row = adw::ActionRow::new();
                    row.set_title(&pkg.name);
                    row.set_subtitle(&pkg.version);
                    row.add_prefix(&gtk::Image::from_icon_name("list-add-symbolic"));
                    added_list.append(&row);
                }

                added_group.add(&added_list);
                content.append(&added_group);
            }

            // Removed packages
            if !diff.removed.is_empty() {
                let removed_group = adw::PreferencesGroup::new();
                removed_group.set_title(&format!("Removed Packages ({})", diff.removed.len()));

                let removed_list = ListBox::new();
                removed_list.add_css_class("boxed-list");

                for pkg in &diff.removed {
                    let row = adw::ActionRow::new();
                    row.set_title(&pkg.name);
                    row.set_subtitle(&pkg.version);
                    row.add_prefix(&gtk::Image::from_icon_name("list-remove-symbolic"));
                    removed_list.append(&row);
                }

                removed_group.add(&removed_list);
                content.append(&removed_group);
            }

            // Updated packages
            if !diff.updated.is_empty() {
                let changed_group = adw::PreferencesGroup::new();
                changed_group.set_title(&format!("Updated Packages ({})", diff.updated.len()));

                let changed_list = ListBox::new();
                changed_list.add_css_class("boxed-list");

                for update in &diff.updated {
                    let row = adw::ActionRow::new();
                    row.set_title(&update.name);
                    row.set_subtitle(&format!("{} → {}", update.old_version, update.new_version));
                    row.add_prefix(&gtk::Image::from_icon_name("emblem-synchronizing-symbolic"));
                    changed_list.append(&row);
                }

                changed_group.add(&changed_list);
                content.append(&changed_group);
            }
        }

        let scrolled = ScrolledWindow::new();
        scrolled.set_child(Some(&content));
        toolbar_view.set_content(Some(&scrolled));

        page
    }

    /// Create file diff page with loading state and async data fetching
    fn create_file_diff_page(
        snap1_name: &str,
        snap2_name: &str,
        _nav_view: adw::NavigationView,
    ) -> adw::NavigationPage {
        let page = adw::NavigationPage::new(&adw::ToolbarView::new(), "File Differences");

        let toolbar_view = page.child().and_downcast::<adw::ToolbarView>().unwrap();

        let header = adw::HeaderBar::new();

        // Add export button (will be enabled after loading completes)
        let export_btn = gtk::Button::from_icon_name("document-save-symbolic");
        export_btn.set_tooltip_text(Some("Export file changes to text file"));
        export_btn.set_sensitive(false); // Disabled during loading
        header.pack_end(&export_btn);

        toolbar_view.add_top_bar(&header);

        // Loading state
        let content = Box::new(Orientation::Vertical, 24);
        content.set_margin_top(48);
        content.set_margin_bottom(24);
        content.set_margin_start(24);
        content.set_margin_end(24);
        content.set_valign(gtk::Align::Center);

        let spinner = gtk::Spinner::new();
        spinner.set_spinning(true);
        spinner.set_halign(gtk::Align::Center);
        spinner.set_size_request(48, 48);
        content.append(&spinner);

        let loading_label = gtk::Label::new(Some("Comparing file changes..."));
        loading_label.add_css_class("title-3");
        loading_label.set_halign(gtk::Align::Center);
        loading_label.set_margin_top(12);
        content.append(&loading_label);

        let hint_label = gtk::Label::new(Some("This may take a moment for large snapshots"));
        hint_label.add_css_class("dim-label");
        hint_label.set_halign(gtk::Align::Center);
        hint_label.set_margin_top(6);
        content.append(&hint_label);

        let scrolled = ScrolledWindow::new();
        scrolled.set_child(Some(&content));
        toolbar_view.set_content(Some(&scrolled));

        // Start comparison in background
        let (tx, rx) = mpsc::channel();
        let old_snapshot = snap1_name.to_string();
        let new_snapshot = snap2_name.to_string();
        let snap1_display = snap1_name.to_string();
        let snap2_display = snap2_name.to_string();

        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<Vec<FileChange>> {
                let client = WaypointHelperClient::new()?;
                let json = client.compare_snapshots(old_snapshot, new_snapshot)?;
                let changes: Vec<FileChange> = serde_json::from_str(&json)?;
                Ok(changes)
            })();
            let _ = tx.send(result);
        });

        // Poll for results
        let page_clone = page.clone();
        let export_btn_clone = export_btn.clone();
        gtk::glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
            match rx.try_recv() {
                Ok(result) => {
                    match result {
                        Ok(changes) => {
                            // Replace loading content with results
                            let new_toolbar_view = page_clone
                                .child()
                                .and_downcast::<adw::ToolbarView>()
                                .unwrap();

                            let results_content = Self::create_file_diff_results(
                                &snap1_display,
                                &snap2_display,
                                changes.clone(),
                            );

                            let scrolled = ScrolledWindow::new();
                            scrolled.set_child(Some(&results_content));
                            new_toolbar_view.set_content(Some(&scrolled));

                            // Enable export button and wire it up
                            export_btn_clone.set_sensitive(true);
                            let snap1_for_export = snap1_display.clone();
                            let snap2_for_export = snap2_display.clone();
                            export_btn_clone.connect_clicked(move |_| {
                                Self::export_file_comparison(&snap1_for_export, &snap2_for_export, &changes);
                            });
                        }
                        Err(e) => {
                            // Show error
                            let new_toolbar_view = page_clone
                                .child()
                                .and_downcast::<adw::ToolbarView>()
                                .unwrap();

                            let error_page = adw::StatusPage::new();
                            error_page.set_icon_name(Some("dialog-error-symbolic"));
                            error_page.set_title("Comparison Failed");

                            let error_msg = if e.to_string().contains("timeout") {
                                "The file comparison took too long (>25 seconds).\n\nThis happens with very large snapshots.\nTry using package comparison instead."
                            } else {
                                "Failed to compare file changes between snapshots."
                            };
                            error_page.set_description(Some(error_msg));

                            new_toolbar_view.set_content(Some(&error_page));
                        }
                    }
                    gtk::glib::ControlFlow::Break
                }
                Err(mpsc::TryRecvError::Empty) => gtk::glib::ControlFlow::Continue,
                Err(_) => gtk::glib::ControlFlow::Break,
            }
        });

        page
    }

    /// Create the results display for file diff
    fn create_file_diff_results(
        snap1_name: &str,
        snap2_name: &str,
        changes: Vec<FileChange>,
    ) -> Box {
        let content = Box::new(Orientation::Vertical, 12);
        content.set_margin_top(12);
        content.set_margin_bottom(12);
        content.set_margin_start(12);
        content.set_margin_end(12);

        // Title showing comparison
        let title_box = Box::new(Orientation::Horizontal, 12);
        title_box.set_halign(gtk::Align::Center);
        title_box.set_margin_bottom(12);

        let snap1_label = gtk::Label::new(Some(snap1_name));
        snap1_label.add_css_class("title-3");
        title_box.append(&snap1_label);

        let arrow = gtk::Image::from_icon_name("go-next-symbolic");
        arrow.add_css_class("dim-label");
        title_box.append(&arrow);

        let snap2_label = gtk::Label::new(Some(snap2_name));
        snap2_label.add_css_class("title-3");
        title_box.append(&snap2_label);

        content.append(&title_box);

        if changes.is_empty() {
            let status_page = adw::StatusPage::new();
            status_page.set_icon_name(Some("emblem-ok-symbolic"));
            status_page.set_title("No File Changes");
            status_page.set_description(Some("The snapshots have identical files"));
            content.append(&status_page);
            return content;
        }

        // Group changes by type
        let mut added: Vec<&FileChange> = Vec::new();
        let mut modified: Vec<&FileChange> = Vec::new();
        let mut deleted: Vec<&FileChange> = Vec::new();

        for change in &changes {
            match change.change_type.as_str() {
                "Added" => added.push(change),
                "Modified" => modified.push(change),
                "Deleted" => deleted.push(change),
                _ => {}
            }
        }

        // Display each category
        if !added.is_empty() {
            let added_group = adw::PreferencesGroup::new();
            added_group.set_title(&format!("Added Files ({})", added.len()));

            let added_list = ListBox::new();
            added_list.add_css_class("boxed-list");

            for change in added.iter().take(100) {
                let row = adw::ActionRow::new();
                row.set_title(&change.path);
                row.add_prefix(&gtk::Image::from_icon_name("list-add-symbolic"));
                added_list.append(&row);
            }

            if added.len() > 100 {
                let more_row = adw::ActionRow::new();
                more_row.set_title(&format!("... and {} more", added.len() - 100));
                more_row.add_css_class("dim-label");
                added_list.append(&more_row);
            }

            added_group.add(&added_list);
            content.append(&added_group);
        }

        if !modified.is_empty() {
            let modified_group = adw::PreferencesGroup::new();
            modified_group.set_title(&format!("Modified Files ({})", modified.len()));

            let modified_list = ListBox::new();
            modified_list.add_css_class("boxed-list");

            for change in modified.iter().take(100) {
                let row = adw::ActionRow::new();
                row.set_title(&change.path);
                row.add_prefix(&gtk::Image::from_icon_name("document-edit-symbolic"));
                modified_list.append(&row);
            }

            if modified.len() > 100 {
                let more_row = adw::ActionRow::new();
                more_row.set_title(&format!("... and {} more", modified.len() - 100));
                more_row.add_css_class("dim-label");
                modified_list.append(&more_row);
            }

            modified_group.add(&modified_list);
            content.append(&modified_group);
        }

        if !deleted.is_empty() {
            let deleted_group = adw::PreferencesGroup::new();
            deleted_group.set_title(&format!("Deleted Files ({})", deleted.len()));

            let deleted_list = ListBox::new();
            deleted_list.add_css_class("boxed-list");

            for change in deleted.iter().take(100) {
                let row = adw::ActionRow::new();
                row.set_title(&change.path);
                row.add_prefix(&gtk::Image::from_icon_name("list-remove-symbolic"));
                deleted_list.append(&row);
            }

            if deleted.len() > 100 {
                let more_row = adw::ActionRow::new();
                more_row.set_title(&format!("... and {} more", deleted.len() - 100));
                more_row.add_css_class("dim-label");
                deleted_list.append(&more_row);
            }

            deleted_group.add(&deleted_list);
            content.append(&deleted_group);
        }

        content
    }

    /// Export file comparison to a text file
    fn export_file_comparison(snap1_name: &str, snap2_name: &str, changes: &[FileChange]) {
        use gtk::gio;

        // Create file chooser dialog
        let dialog = gtk::FileDialog::new();
        dialog.set_title("Export File Comparison");
        dialog.set_initial_name(Some(&format!("file_changes_{snap1_name}_{snap2_name}.txt")));

        // Set default filter for text files
        let filter = gtk::FileFilter::new();
        filter.set_name(Some("Text files"));
        filter.add_pattern("*.txt");
        let filters = gio::ListStore::new::<gtk::FileFilter>();
        filters.append(&filter);
        dialog.set_filters(Some(&filters));

        let snap1 = snap1_name.to_string();
        let snap2 = snap2_name.to_string();
        let changes = changes.to_vec();

        dialog.save(None::<&gtk::Window>, None::<&gio::Cancellable>, move |result| {
            if let Ok(file) = result {
                if let Some(path) = file.path() {
                    match Self::write_file_comparison_file(&path, &snap1, &snap2, &changes) {
                        Ok(()) => {
                            log::info!("Exported file comparison to {}", path.display());
                        }
                        Err(e) => {
                            log::error!("Failed to export file comparison: {e}");
                        }
                    }
                }
            }
        });
    }

    /// Write file comparison data to a text file
    fn write_file_comparison_file(
        path: &std::path::Path,
        snap1_name: &str,
        snap2_name: &str,
        changes: &[FileChange],
    ) -> anyhow::Result<()> {
        use std::io::Write;

        let mut file = std::fs::File::create(path)?;

        // Group changes by type
        let mut added: Vec<&FileChange> = Vec::new();
        let mut modified: Vec<&FileChange> = Vec::new();
        let mut deleted: Vec<&FileChange> = Vec::new();

        for change in changes {
            match change.change_type.as_str() {
                "Added" => added.push(change),
                "Modified" => modified.push(change),
                "Deleted" => deleted.push(change),
                _ => {}
            }
        }

        writeln!(file, "File Changes Report")?;
        writeln!(file, "===================")?;
        writeln!(file)?;
        writeln!(file, "Base Snapshot:    {snap1_name}")?;
        writeln!(file, "Compare Snapshot: {snap2_name}")?;
        writeln!(file)?;
        writeln!(file, "Summary:")?;
        writeln!(file, "  {} files added", added.len())?;
        writeln!(file, "  {} files modified", modified.len())?;
        writeln!(file, "  {} files deleted", deleted.len())?;
        writeln!(file)?;

        if !added.is_empty() {
            writeln!(file, "Added Files ({}):", added.len())?;
            writeln!(file, "{}", "-".repeat(60))?;
            for change in &added {
                writeln!(file, "  + {}", change.path)?;
            }
            writeln!(file)?;
        }

        if !modified.is_empty() {
            writeln!(file, "Modified Files ({}):", modified.len())?;
            writeln!(file, "{}", "-".repeat(60))?;
            for change in &modified {
                writeln!(file, "  ~ {}", change.path)?;
            }
            writeln!(file)?;
        }

        if !deleted.is_empty() {
            writeln!(file, "Deleted Files ({}):", deleted.len())?;
            writeln!(file, "{}", "-".repeat(60))?;
            for change in &deleted {
                writeln!(file, "  - {}", change.path)?;
            }
            writeln!(file)?;
        }

        if changes.is_empty() {
            writeln!(file, "No file changes detected.")?;
            writeln!(file, "Both snapshots have identical files.")?;
        }

        Ok(())
    }

    /// Export package comparison to a text file
    fn export_package_comparison(snap1_name: &str, snap2_name: &str, diff: &PackageDiff) {
        use gtk::gio;

        // Create file chooser dialog
        let dialog = gtk::FileDialog::new();
        dialog.set_title("Export Package Comparison");
        dialog.set_initial_name(Some(&format!("comparison_{snap1_name}_{snap2_name}.txt")));

        // Set default filter for text files
        let filter = gtk::FileFilter::new();
        filter.set_name(Some("Text files"));
        filter.add_pattern("*.txt");
        let filters = gio::ListStore::new::<gtk::FileFilter>();
        filters.append(&filter);
        dialog.set_filters(Some(&filters));

        let snap1 = snap1_name.to_string();
        let snap2 = snap2_name.to_string();
        let diff = diff.clone();

        dialog.save(None::<&gtk::Window>, None::<&gio::Cancellable>, move |result| {
            if let Ok(file) = result {
                if let Some(path) = file.path() {
                    match Self::write_comparison_file(&path, &snap1, &snap2, &diff) {
                        Ok(()) => {
                            log::info!("Exported comparison to {}", path.display());
                        }
                        Err(e) => {
                            log::error!("Failed to export comparison: {e}");
                        }
                    }
                }
            }
        });
    }

    /// Write comparison data to a text file
    fn write_comparison_file(
        path: &std::path::Path,
        snap1_name: &str,
        snap2_name: &str,
        diff: &PackageDiff,
    ) -> anyhow::Result<()> {
        use std::io::Write;

        let mut file = std::fs::File::create(path)?;

        writeln!(file, "Package Comparison Report")?;
        writeln!(file, "=========================")?;
        writeln!(file)?;
        writeln!(file, "Base Snapshot:    {snap1_name}")?;
        writeln!(file, "Compare Snapshot: {snap2_name}")?;
        writeln!(file)?;
        writeln!(file, "Summary:")?;
        writeln!(file, "  {} packages added", diff.added.len())?;
        writeln!(file, "  {} packages removed", diff.removed.len())?;
        writeln!(file, "  {} packages updated", diff.updated.len())?;
        writeln!(file)?;

        if !diff.added.is_empty() {
            writeln!(file, "Added Packages ({}):", diff.added.len())?;
            writeln!(file, "{}", "-".repeat(60))?;
            for pkg in &diff.added {
                writeln!(file, "  + {} ({})", pkg.name, pkg.version)?;
            }
            writeln!(file)?;
        }

        if !diff.removed.is_empty() {
            writeln!(file, "Removed Packages ({}):", diff.removed.len())?;
            writeln!(file, "{}", "-".repeat(60))?;
            for pkg in &diff.removed {
                writeln!(file, "  - {} ({})", pkg.name, pkg.version)?;
            }
            writeln!(file)?;
        }

        if !diff.updated.is_empty() {
            writeln!(file, "Updated Packages ({}):", diff.updated.len())?;
            writeln!(file, "{}", "-".repeat(60))?;
            for update in &diff.updated {
                writeln!(file, "  ~ {} ({} → {})",
                    update.name, update.old_version, update.new_version)?;
            }
            writeln!(file)?;
        }

        if diff.added.is_empty() && diff.removed.is_empty() && diff.updated.is_empty() {
            writeln!(file, "No package changes detected.")?;
            writeln!(file, "Both snapshots have identical package sets.")?;
        }

        Ok(())
    }

    /// Get the main widget
    pub fn widget(&self) -> &adw::NavigationView {
        &self.widget
    }
}
