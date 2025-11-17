use adw::prelude::*;
use gtk::prelude::*;
use gtk::{CheckButton, Label};
use libadwaita as adw;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use crate::subvolume::{SubvolumeInfo, detect_mounted_subvolumes, should_allow_snapshot};

// Global state for current subvolume selection (used across dialogs)
thread_local! {
    static CURRENT_SUBVOLUMES: RefCell<Option<Rc<RefCell<Vec<PathBuf>>>>> = RefCell::new(None);
}

/// Get the current subvolume selection
pub fn get_current_subvolume_selection() -> Vec<PathBuf> {
    CURRENT_SUBVOLUMES.with(|cell| {
        cell.borrow()
            .as_ref()
            .map(|rc| rc.borrow().clone())
            .unwrap_or_else(load_config)
    })
}

/// Create the subvolumes preferences page
pub fn create_subvolumes_page(parent: &adw::ApplicationWindow) -> adw::PreferencesPage {
    let current_config = load_config();

    // Store current selection in global state
    let enabled_subvolumes = Rc::new(RefCell::new(current_config.clone()));
    CURRENT_SUBVOLUMES.with(|cell| {
        *cell.borrow_mut() = Some(enabled_subvolumes.clone());
    });

    // Create preferences page
    let page = adw::PreferencesPage::new();
    page.set_title("Manual Snapshots");
    page.set_icon_name(Some("drive-harddisk-symbolic"));

    // Create group for subvolume selection
    let group = adw::PreferencesGroup::new();
    group.set_title("Manual Snapshots");
    group.set_description(Some(
        "Select which Btrfs subvolumes to include when manually creating snapshots. \
         Scheduled snapshots have separate settings configured in each schedule.",
    ));

    // Detect available subvolumes
    let subvolumes = match detect_mounted_subvolumes() {
        Ok(subvols) => subvols,
        Err(e) => {
            log::error!("Failed to detect subvolumes: {}", e);
            Vec::new()
        }
    };

    if subvolumes.is_empty() {
        let empty_label = Label::new(Some("No Btrfs subvolumes detected"));
        empty_label.add_css_class("dim-label");
        group.add(&empty_label);
    } else {
        // Create checkbox for each subvolume
        let checkboxes: Vec<(SubvolumeInfo, CheckButton)> = subvolumes
            .into_iter()
            .filter_map(|subvol| {
                // Filter out subvolumes that should never be snapshotted
                if !should_allow_snapshot(&subvol.subvol_path) {
                    return None;
                }

                let checkbox_row = create_subvolume_row(&subvol, &current_config);
                let checkbox = checkbox_row
                    .activatable_widget()
                    .and_then(|w| w.downcast::<CheckButton>().ok())?;

                group.add(&checkbox_row);
                Some((subvol, checkbox))
            })
            .collect();

        // Update preferences when checkboxes change
        for (subvol, checkbox) in checkboxes {
            let enabled_clone = enabled_subvolumes.clone();
            let mount_point = subvol.mount_point.clone();
            let parent_clone = parent.clone();

            checkbox.connect_toggled(move |cb| {
                let mut enabled = enabled_clone.borrow().clone();

                if cb.is_active() {
                    if !enabled.contains(&mount_point) {
                        enabled.push(mount_point.clone());
                    }
                } else {
                    enabled.retain(|p| p != &mount_point);
                }

                *enabled_clone.borrow_mut() = enabled.clone();

                // Auto-save configuration
                if let Err(e) = save_config(&enabled) {
                    log::error!("Failed to save subvolume preferences: {}", e);
                    super::dialogs::show_error(
                        &parent_clone,
                        "Save Failed",
                        &format!("Failed to save snapshot target preferences: {}", e),
                    );
                } else {
                    log::info!("Saved subvolume preferences: {:?}", enabled);
                    super::dialogs::show_toast(&parent_clone, "Manual snapshot settings updated");
                }
            });
        }
    }

    page.add(&group);
    page
}

/// Create a row for a subvolume checkbox
fn create_subvolume_row(subvol: &SubvolumeInfo, current_config: &[PathBuf]) -> adw::ActionRow {
    let row = adw::ActionRow::new();
    row.set_title(&subvol.display_name);

    // Show subvolume path as subtitle
    let subtitle = format!("Subvolume: {}", subvol.subvol_path);
    row.set_subtitle(&subtitle);

    // Add checkbox
    let checkbox = CheckButton::new();
    checkbox.set_valign(gtk::Align::Center);

    // Set initial state based on current config
    let is_enabled = current_config.contains(&subvol.mount_point);
    checkbox.set_active(is_enabled);

    // Root filesystem should always be enabled and not changeable
    if subvol.mount_point == PathBuf::from("/") {
        checkbox.set_active(true);
        checkbox.set_sensitive(false);
        row.set_subtitle("Subvolume: @ (Required)");
    }

    row.add_suffix(&checkbox);
    row.set_activatable_widget(Some(&checkbox));

    row
}

/// Load subvolume configuration from disk
pub fn load_config() -> Vec<PathBuf> {
    let config_path = dirs::config_local_dir()
        .map(|d| d.join("waypoint").join("subvolumes.json"))
        .unwrap_or_else(|| PathBuf::from("/tmp/waypoint-subvolumes.json"));

    if !config_path.exists() {
        // Default to only root
        return vec![PathBuf::from("/")];
    }

    match std::fs::read_to_string(&config_path) {
        Ok(content) => {
            match serde_json::from_str::<Vec<String>>(&content) {
                Ok(paths) => {
                    let mut result: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();

                    // Ensure root is always included
                    if !result.contains(&PathBuf::from("/")) {
                        result.insert(0, PathBuf::from("/"));
                    }

                    result
                }
                Err(e) => {
                    log::error!("Failed to parse config: {}", e);
                    vec![PathBuf::from("/")]
                }
            }
        }
        Err(e) => {
            log::error!("Failed to read config: {}", e);
            vec![PathBuf::from("/")]
        }
    }
}

/// Save subvolume configuration to disk
pub fn save_config(enabled_subvolumes: &[PathBuf]) -> anyhow::Result<()> {
    let config_dir = dirs::config_local_dir()
        .map(|d| d.join("waypoint"))
        .unwrap_or_else(|| PathBuf::from("/tmp"));

    std::fs::create_dir_all(&config_dir)?;

    let config_path = config_dir.join("subvolumes.json");

    // Convert PathBuf to String for JSON serialization
    let paths: Vec<String> = enabled_subvolumes
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();

    let content = serde_json::to_string_pretty(&paths)?;
    std::fs::write(&config_path, content)?;

    Ok(())
}
