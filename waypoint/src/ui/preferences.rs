use gtk::{CheckButton, Label};
use gtk::prelude::*;
use libadwaita as adw;
use adw::prelude::*;
use std::path::PathBuf;
use std::rc::Rc;
use std::cell::RefCell;

use crate::subvolume::{detect_mounted_subvolumes, SubvolumeInfo};

/// Configuration for subvolume snapshots
#[derive(Clone)]
pub struct SubvolumePreferences {
    enabled_subvolumes: Rc<RefCell<Vec<PathBuf>>>,
}

impl SubvolumePreferences {
    pub fn new(enabled: Vec<PathBuf>) -> Self {
        Self {
            enabled_subvolumes: Rc::new(RefCell::new(enabled)),
        }
    }

    pub fn get_enabled(&self) -> Vec<PathBuf> {
        self.enabled_subvolumes.borrow().clone()
    }

    #[allow(dead_code)]
    fn set_enabled(&self, enabled: Vec<PathBuf>) {
        *self.enabled_subvolumes.borrow_mut() = enabled;
    }
}

/// Show preferences dialog for selecting which subvolumes to snapshot
pub fn show_preferences_dialog(parent: &adw::ApplicationWindow, current_config: Vec<PathBuf>) -> SubvolumePreferences {
    let prefs = SubvolumePreferences::new(current_config.clone());
    let prefs_clone = prefs.clone();

    // Create preferences window
    let dialog = adw::PreferencesWindow::new();
    dialog.set_title(Some("Snapshot Preferences"));
    dialog.set_modal(true);
    dialog.set_transient_for(Some(parent));
    dialog.set_default_size(480, 450);

    // Create preferences page
    let page = adw::PreferencesPage::new();
    page.set_title("Subvolumes");
    page.set_icon_name(Some("drive-harddisk-symbolic"));

    // Create group for subvolume selection
    let group = adw::PreferencesGroup::new();
    group.set_title("Subvolumes to Snapshot");
    group.set_description(Some(
        "Select which Btrfs subvolumes should be included when creating restore points. \
         The root filesystem (/) is always required."
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
                let checkbox_row = create_subvolume_row(&subvol, &current_config);
                let checkbox = checkbox_row.activatable_widget()
                    .and_then(|w| w.downcast::<CheckButton>().ok())?;

                group.add(&checkbox_row);
                Some((subvol, checkbox))
            })
            .collect();

        // Update preferences when checkboxes change
        for (subvol, checkbox) in checkboxes {
            let prefs_clone2 = prefs_clone.clone();
            let mount_point = subvol.mount_point.clone();

            checkbox.connect_toggled(move |cb| {
                let mut enabled = prefs_clone2.enabled_subvolumes.borrow().clone();

                if cb.is_active() {
                    if !enabled.contains(&mount_point) {
                        enabled.push(mount_point.clone());
                    }
                } else {
                    enabled.retain(|p| p != &mount_point);
                }

                *prefs_clone2.enabled_subvolumes.borrow_mut() = enabled;
            });
        }
    }

    page.add(&group);
    dialog.add(&page);

    // Show dialog
    dialog.present();

    prefs
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

    // Disable @snapshots and @swap - these should not be snapshotted
    if subvol.subvol_path == "@snapshots" || subvol.subvol_path == "@swap" {
        checkbox.set_active(false);
        checkbox.set_sensitive(false);
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
                    let mut result: Vec<PathBuf> = paths.into_iter()
                        .map(PathBuf::from)
                        .collect();

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
