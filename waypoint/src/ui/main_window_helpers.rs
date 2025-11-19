//! Helper functions for MainWindow

use crate::btrfs;
use gtk::prelude::*;
use gtk::{glib, Label};
use libadwaita as adw;
use std::cell::RefCell;
use std::rc::Rc;

/// Update the disk space label with current usage
///
/// Queries the available space for the root filesystem and updates the label and level bar
/// with color-coded visuals based on remaining space percentage.
pub fn update_disk_space_label(label: &Label, level_bar: &gtk::LevelBar) {
    use std::path::PathBuf;

    // Query disk space for root (where snapshots are stored)
    let space_result = btrfs::get_available_space(&PathBuf::from("/"));

    match space_result {
        Ok(available_bytes) => {
            // Get total filesystem size by reading from df
            let total_result = std::process::Command::new("df")
                .arg("-B1")
                .arg("--output=size")
                .arg("/")
                .output();

            let (available_gb, total_gb, percent_free) = match total_result {
                Ok(output) if output.status.success() => {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let lines: Vec<&str> = stdout.lines().collect();

                    if lines.len() >= 2 {
                        if let Ok(total_bytes) = lines[1].trim().parse::<u64>() {
                            let available_gb = available_bytes as f64 / 1_073_741_824.0; // Convert to GB
                            let total_gb = total_bytes as f64 / 1_073_741_824.0;
                            let percent = (available_bytes as f64 / total_bytes as f64) * 100.0;
                            (available_gb, total_gb, percent)
                        } else {
                            // Fallback: just show available
                            let available_gb = available_bytes as f64 / 1_073_741_824.0;
                            (available_gb, 0.0, 0.0)
                        }
                    } else {
                        let available_gb = available_bytes as f64 / 1_073_741_824.0;
                        (available_gb, 0.0, 0.0)
                    }
                }
                _ => {
                    // Fallback: just show available
                    let available_gb = available_bytes as f64 / 1_073_741_824.0;
                    (available_gb, 0.0, 0.0)
                }
            };

            // Format the label text
            let text = if total_gb > 0.0 {
                format!(
                    "{available_gb:.1} GB free of {total_gb:.1} GB ({percent_free:.0}% free)"
                )
            } else {
                format!("{available_gb:.1} GB free")
            };

            label.set_text(&text);

            // Update level bar to show percentage used (inverted from percent_free)
            if total_gb > 0.0 {
                let percent_used = 100.0 - percent_free;
                level_bar.set_value(percent_used / 100.0);
            } else {
                level_bar.set_value(0.0);
            }

            // Remove any existing warning/error classes
            label.remove_css_class("warning");
            label.remove_css_class("error");

            // Color-code based on percentage (if we have total)
            if total_gb > 0.0 {
                if percent_free < 10.0 {
                    // Critical: < 10% free - red
                    label.add_css_class("error");
                    label.set_tooltip_text(Some(
                        "Low disk space! Consider deleting old snapshots.",
                    ));
                } else if percent_free < 20.0 {
                    // Warning: < 20% free - yellow
                    label.add_css_class("warning");
                    label.set_tooltip_text(Some(
                        "Disk space running low. Monitor snapshot usage.",
                    ));
                } else {
                    // OK: >= 20% free - normal
                    label.set_tooltip_text(Some("Available disk space for snapshots"));
                }
            } else {
                label.set_tooltip_text(Some("Available disk space"));
            }
        }
        Err(e) => {
            label.set_text("Space: Unknown");
            label.set_tooltip_text(Some(&format!("Failed to query disk space: {e}")));
        }
    }
}

/// Create the status banner that shows if Btrfs is available
pub fn create_status_banner() -> (adw::Banner, bool) {
    let banner = adw::Banner::new("");

    // Check if running on Btrfs
    let is_btrfs = match btrfs::is_btrfs(&std::path::PathBuf::from("/")) {
        Ok(true) => {
            // Btrfs detected - don't show banner
            banner.set_revealed(false);
            true
        }
        Ok(false) => {
            banner.set_title("Btrfs is required to create system restore points");
            banner.set_button_label(Some("Learn More"));
            banner.set_revealed(true);

            // Connect "Learn More" button to open documentation
            banner.connect_button_clicked(|_| {
                // Open Btrfs wiki page
                let _ = std::process::Command::new("xdg-open")
                    .arg("https://btrfs.readthedocs.io/")
                    .spawn();
            });

            false
        }
        Err(e) => {
            banner.set_title(&format!("Unable to detect filesystem type: {e}"));
            banner.set_revealed(true);
            false
        }
    };

    (banner, is_btrfs)
}

/// Stop a progress pulse animation
pub fn stop_progress_pulse(handle: &Rc<RefCell<Option<glib::SourceId>>>) {
    if let Some(source_id) = handle.borrow_mut().take() {
        source_id.remove();
    }
}
