//! Exclude pattern configuration preferences UI

use adw::prelude::*;
use gtk::Orientation;
use gtk::prelude::*;
use libadwaita as adw;
use waypoint_common::{ExcludeConfig, ExcludePattern, PatternType};

/// Create the exclude patterns preferences page
pub fn create_exclude_page(parent: &adw::ApplicationWindow) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::new();
    page.set_title("Exclusions");
    page.set_icon_name(Some("edit-delete-symbolic"));

    // Load current config
    let config = ExcludeConfig::load().unwrap_or_default();

    // Info group
    let info_group = adw::PreferencesGroup::new();
    info_group.set_title("Exclude Patterns");
    info_group.set_description(Some(
        "Files and directories matching these patterns will be excluded from snapshots. \
         This saves disk space by skipping caches, temporary files, and other non-essential data.",
    ));
    page.add(&info_group);

    // System defaults group
    let defaults_group = adw::PreferencesGroup::new();
    defaults_group.set_title("System Defaults");
    defaults_group.set_description(Some("Built-in patterns (can be disabled but not deleted)"));

    let system_patterns: Vec<_> = config
        .patterns
        .iter()
        .filter(|p| p.system_default)
        .collect();

    for (idx, pattern) in system_patterns.iter().enumerate() {
        let row = create_pattern_row(pattern, idx, true, parent, None);
        defaults_group.add(&row);
    }

    page.add(&defaults_group);

    // Custom patterns group
    let custom_group = adw::PreferencesGroup::new();
    custom_group.set_title("Custom Patterns");
    custom_group.set_description(Some("Your own exclusion patterns"));

    let custom_patterns: Vec<_> = config
        .patterns
        .iter()
        .enumerate()
        .filter(|(_, p)| !p.system_default)
        .collect();

    if custom_patterns.is_empty() {
        let empty_row = adw::ActionRow::new();
        empty_row.set_title("No custom patterns defined");
        empty_row.set_sensitive(false);
        custom_group.add(&empty_row);
    } else {
        for (idx, pattern) in custom_patterns {
            let row = create_pattern_row(pattern, idx, false, parent, Some(&custom_group));
            custom_group.add(&row);
        }
    }

    page.add(&custom_group);

    // Actions group
    let actions_group = adw::PreferencesGroup::new();
    actions_group.set_title("Actions");

    // Add pattern button
    let add_row = adw::ActionRow::new();
    add_row.set_title("Add Custom Pattern");
    add_row.set_subtitle("Create a new exclusion pattern");

    let add_button = gtk::Button::with_label("Add");
    add_button.set_valign(gtk::Align::Center);
    add_button.add_css_class("suggested-action");

    let parent_clone = parent.clone();
    let custom_group_clone = custom_group.clone();
    add_button.connect_clicked(move |_| {
        show_add_pattern_dialog(&parent_clone, &custom_group_clone);
    });

    add_row.add_suffix(&add_button);
    actions_group.add(&add_row);

    page.add(&actions_group);

    page
}

/// Create a row for a pattern
fn create_pattern_row(
    pattern: &ExcludePattern,
    _index: usize,
    is_system: bool,
    parent: &adw::ApplicationWindow,
    custom_group: Option<&adw::PreferencesGroup>,
) -> adw::ActionRow {
    let row = adw::ActionRow::new();
    row.set_title(&pattern.pattern);

    let subtitle = format!(
        "{} - {}",
        match pattern.pattern_type {
            PatternType::Exact => "Exact match",
            PatternType::Prefix => "Prefix match",
            PatternType::Glob => "Glob pattern",
        },
        pattern.description
    );
    row.set_subtitle(&subtitle);

    // Enable/disable switch
    let switch = gtk::Switch::new();
    switch.set_active(pattern.enabled);
    switch.set_valign(gtk::Align::Center);

    let pattern_str = pattern.pattern.clone();
    let parent_clone = parent.clone();
    switch.connect_active_notify(move |sw| {
        let mut config = ExcludeConfig::load().unwrap_or_default();

        // Find and toggle this pattern
        if let Some(p) = config
            .patterns
            .iter_mut()
            .find(|p| p.pattern == pattern_str)
        {
            p.enabled = sw.is_active();
        }

        if let Err(e) = save_exclude_config(&config) {
            log::error!("Failed to save exclude config: {e}");
            super::dialogs::show_error(
                &parent_clone,
                "Save Failed",
                &format!("Failed to save exclusion pattern: {e}"),
            );
        } else {
            super::dialogs::show_toast(&parent_clone, "Exclusion pattern updated");
        }
    });

    row.add_suffix(&switch);

    // Delete button for custom patterns
    if !is_system {
        let delete_button = gtk::Button::from_icon_name("user-trash-symbolic");
        delete_button.set_valign(gtk::Align::Center);
        delete_button.add_css_class("destructive-action");

        let pattern_str = pattern.pattern.clone();
        let parent_clone2 = parent.clone();
        let row_clone = row.clone();
        let custom_group_clone = custom_group.cloned();
        delete_button.connect_clicked(move |_| {
            let mut config = ExcludeConfig::load().unwrap_or_default();

            // Find and remove this pattern
            config.patterns.retain(|p| p.pattern != pattern_str);

            if let Err(e) = save_exclude_config(&config) {
                log::error!("Failed to save exclude config: {e}");
                super::dialogs::show_error(
                    &parent_clone2,
                    "Save Failed",
                    &format!("Failed to delete exclusion pattern: {e}"),
                );
            } else {
                log::info!("Deleted pattern: {pattern_str}");
                super::dialogs::show_toast(&parent_clone2, "Exclusion pattern deleted");

                // Remove this row from the group
                if let Some(ref group) = custom_group_clone {
                    group.remove(&row_clone);

                    // Check if group is now empty and add empty state if needed
                    if !has_custom_rows(group) {
                        let empty_row = adw::ActionRow::new();
                        empty_row.set_title("No custom patterns defined");
                        empty_row.set_sensitive(false);
                        group.add(&empty_row);
                    }
                }
            }
        });

        row.add_suffix(&delete_button);
    }

    row
}

/// Check if a preferences group has any custom pattern rows (non-empty-state rows)
fn has_custom_rows(group: &adw::PreferencesGroup) -> bool {
    let mut child = group.first_child();
    while let Some(widget) = child {
        let next = widget.next_sibling();
        if let Ok(row) = widget.downcast::<adw::ActionRow>() {
            // If the row is sensitive, it's a real pattern row (empty state is insensitive)
            if row.is_sensitive() {
                return true;
            }
        }
        child = next;
    }
    false
}

/// Show dialog to add a new pattern
fn show_add_pattern_dialog(parent: &adw::ApplicationWindow, custom_group: &adw::PreferencesGroup) {
    let dialog = adw::MessageDialog::new(Some(parent), Some("Add Exclusion Pattern"), None);
    dialog.set_modal(true);

    // Create form in a box
    let content = gtk::Box::new(Orientation::Vertical, 12);
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_margin_start(12);
    content.set_margin_end(12);

    // Pattern entry
    let pattern_row = adw::EntryRow::new();
    pattern_row.set_title("Pattern");
    content.append(&pattern_row);

    // Pattern type dropdown
    let type_row = adw::ComboRow::new();
    type_row.set_title("Pattern Type");
    let type_model = gtk::StringList::new(&["Prefix Match", "Exact Match", "Glob Pattern"]);
    type_row.set_model(Some(&type_model));
    type_row.set_selected(0); // Default to prefix
    content.append(&type_row);

    // Description entry
    let desc_row = adw::EntryRow::new();
    desc_row.set_title("Description");
    content.append(&desc_row);

    // Examples
    let examples_label = gtk::Label::new(Some(
        "Examples:\n\
         • Prefix: /var/cache (excludes /var/cache/*)\n\
         • Exact: /swapfile (excludes only /swapfile)\n\
         • Glob: /home/*/.cache (excludes all user caches)",
    ));
    examples_label.set_xalign(0.0);
    examples_label.add_css_class("dim-label");
    examples_label.add_css_class("caption");
    content.append(&examples_label);

    dialog.set_extra_child(Some(&content));

    dialog.add_response("cancel", "Cancel");
    dialog.add_response("add", "Add");
    dialog.set_response_appearance("add", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("add"));

    let parent_clone = parent.clone();
    let custom_group_clone = custom_group.clone();
    dialog.connect_response(None, move |dialog, response| {
        if response == "add" {
            let pattern_text = pattern_row.text().to_string();
            let description = desc_row.text().to_string();

            if pattern_text.is_empty() {
                return;
            }

            let pattern_type = match type_row.selected() {
                0 => PatternType::Prefix,
                1 => PatternType::Exact,
                _ => PatternType::Glob,
            };

            let new_pattern = ExcludePattern::new(
                pattern_text.clone(),
                pattern_type,
                if description.is_empty() {
                    "Custom pattern".to_string()
                } else {
                    description
                },
            );

            let mut config = ExcludeConfig::load().unwrap_or_default();
            config.add_pattern(new_pattern.clone());

            if let Err(e) = save_exclude_config(&config) {
                log::error!("Failed to save exclude config: {e}");
                super::dialogs::show_error(
                    &parent_clone,
                    "Save Failed",
                    &format!("Failed to add exclusion pattern: {e}"),
                );
            } else {
                log::info!("Added new exclude pattern: {pattern_text}");
                super::dialogs::show_toast(&parent_clone, "Exclusion pattern added");

                // Remove empty state if present
                let mut child = custom_group_clone.first_child();
                while let Some(widget) = child {
                    let next = widget.next_sibling();
                    if let Ok(row) = widget.downcast::<adw::ActionRow>() {
                        if !row.is_sensitive() {
                            // This is the empty state row, remove it
                            custom_group_clone.remove(&row);
                            break;
                        }
                    }
                    child = next;
                }

                // Add the new pattern row
                let new_row = create_pattern_row(&new_pattern, 0, false, &parent_clone, Some(&custom_group_clone));
                custom_group_clone.add(&new_row);
            }
        }

        dialog.close();
    });

    dialog.present();
}

/// Save exclude config via D-Bus (requires root permissions)
fn save_exclude_config(config: &ExcludeConfig) -> anyhow::Result<()> {
    use crate::dbus_client::WaypointHelperClient;

    // Serialize config to TOML
    let toml_content = toml::to_string_pretty(config)?;

    // Call D-Bus to save (this will prompt for password)
    let client = WaypointHelperClient::new()?;
    client.save_exclude_config(toml_content)?;

    Ok(())
}
