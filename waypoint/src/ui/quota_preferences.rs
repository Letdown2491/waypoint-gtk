//! Quota configuration preferences UI

use crate::dbus_client::WaypointHelperClient;
use adw::prelude::*;
use gtk::prelude::*;
use gtk::{Orientation, SpinButton};
use libadwaita as adw;
use waypoint_common::{QuotaConfig, QuotaType};

use super::dialogs;

/// Create the quota preferences page
pub fn create_quota_page(parent: &adw::ApplicationWindow) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::new();
    page.set_title("Quota");
    page.set_icon_name(Some("drive-harddisk-symbolic"));

    // Load current config
    let config = QuotaConfig::load().unwrap_or_default();

    // Basic quota settings group
    let basic_group = adw::PreferencesGroup::new();
    basic_group.set_title("Quota Settings");
    basic_group.set_description(Some(
        "Control snapshot disk space usage with btrfs quotas.",
    ));

    // Enable quotas switch
    let enable_row = adw::SwitchRow::new();
    enable_row.set_title("Enable Quotas");
    enable_row.set_subtitle("Track and limit snapshot disk usage");
    enable_row.set_active(config.enabled);
    basic_group.add(&enable_row);

    // Quota type selection
    let type_row = adw::ComboRow::new();
    type_row.set_title("Quota Type");
    type_row.set_subtitle("Simple: faster, less overhead. Traditional: complete tracking");

    let type_model = gtk::StringList::new(&["Simple", "Traditional"]);
    type_row.set_model(Some(&type_model));
    type_row.set_selected(match config.quota_type {
        QuotaType::Simple => 0,
        QuotaType::Traditional => 1,
    });
    type_row.set_sensitive(config.enabled);
    basic_group.add(&type_row);

    // Quota status row (between type and cleanup)
    let status_row = adw::ActionRow::new();
    status_row.set_title("Quota Status");

    // Try to get current usage
    let usage_text = if config.enabled {
        match WaypointHelperClient::new() {
            Ok(client) => match client.get_quota_usage() {
                Ok(usage) => {
                    let used = QuotaConfig::format_size(usage.referenced);
                    if let Some(limit) = usage.limit {
                        let limit_str = QuotaConfig::format_size(limit);
                        let pct = usage.usage_percent().unwrap_or(0.0) * 100.0;
                        format!("{} / {} ({:.1}%)", used, limit_str, pct)
                    } else {
                        format!("{} (no limit set)", used)
                    }
                }
                Err(e) => format!("Error: {}", e),
            },
            Err(_) => "Cannot connect to helper service".to_string(),
        }
    } else {
        "Quotas not enabled".to_string()
    };

    status_row.set_subtitle(&usage_text);
    status_row.set_visible(config.enabled);
    basic_group.add(&status_row);

    // Auto-cleanup switch
    let cleanup_row = adw::SwitchRow::new();
    cleanup_row.set_title("Automatic Cleanup");
    cleanup_row.set_subtitle("Automatically delete old snapshots when quota limit is reached");
    cleanup_row.set_active(config.auto_cleanup);
    cleanup_row.set_sensitive(config.enabled);
    basic_group.add(&cleanup_row);

    page.add(&basic_group);

    // Limits group
    let limits_group = adw::PreferencesGroup::new();
    limits_group.set_title("Limits");
    limits_group.set_description(Some("Set maximum disk space for snapshots"));
    limits_group.set_margin_top(24);

    // Total limit row
    let limit_row = adw::ActionRow::new();
    limit_row.set_title("Total Snapshot Limit");
    limit_row.set_subtitle("Maximum space for all snapshots (0 = no limit)");

    // Create spin button for limit in GB
    let limit_spin = SpinButton::with_range(0.0, 10000.0, 1.0);
    let current_limit_gb = config
        .total_limit_bytes
        .map(|bytes| bytes as f64 / (1024.0 * 1024.0 * 1024.0))
        .unwrap_or(0.0);
    limit_spin.set_value(current_limit_gb);
    limit_spin.set_digits(0);
    limit_spin.set_valign(gtk::Align::Center);
    limit_spin.set_sensitive(config.enabled);

    let limit_label = gtk::Label::new(Some("GB"));
    limit_label.set_valign(gtk::Align::Center);
    limit_label.add_css_class("dim-label");

    let limit_box = gtk::Box::new(Orientation::Horizontal, 6);
    limit_box.append(&limit_spin);
    limit_box.append(&limit_label);

    limit_row.add_suffix(&limit_box);
    limits_group.add(&limit_row);

    // Cleanup threshold row
    let threshold_row = adw::ActionRow::new();
    threshold_row.set_title("Cleanup Threshold");
    threshold_row.set_subtitle("Trigger cleanup when usage reaches this percentage");

    let threshold_spin = SpinButton::with_range(50.0, 99.0, 1.0);
    threshold_spin.set_value(config.cleanup_threshold * 100.0);
    threshold_spin.set_digits(0);
    threshold_spin.set_valign(gtk::Align::Center);
    threshold_spin.set_sensitive(config.enabled && config.auto_cleanup);

    let threshold_label = gtk::Label::new(Some("%"));
    threshold_label.set_valign(gtk::Align::Center);
    threshold_label.add_css_class("dim-label");

    let threshold_box = gtk::Box::new(Orientation::Horizontal, 6);
    threshold_box.append(&threshold_spin);
    threshold_box.append(&threshold_label);

    threshold_row.add_suffix(&threshold_box);
    limits_group.add(&threshold_row);

    page.add(&limits_group);

    // Apply button (standalone at bottom, right-aligned like in scheduler dialog)
    let button_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    button_box.set_margin_top(24);
    button_box.set_margin_bottom(12);
    button_box.set_margin_start(12);
    button_box.set_margin_end(12);
    button_box.set_halign(gtk::Align::End);

    let apply_button = gtk::Button::with_label("Apply");
    apply_button.add_css_class("suggested-action");
    button_box.append(&apply_button);

    // Create a group to hold the button
    let button_group = adw::PreferencesGroup::new();
    button_group.add(&button_box);
    page.add(&button_group);

    // Wire up sensitivity changes
    let type_row_clone = type_row.clone();
    let cleanup_row_clone = cleanup_row.clone();
    let limit_spin_clone = limit_spin.clone();
    let status_row_clone = status_row.clone();
    let enable_row_clone2 = enable_row.clone();

    enable_row.connect_active_notify(move |switch| {
        let enabled = switch.is_active();
        type_row_clone.set_sensitive(enabled);
        cleanup_row_clone.set_sensitive(enabled);
        limit_spin_clone.set_sensitive(enabled);
        status_row_clone.set_visible(enabled);
    });

    let threshold_spin_clone2 = threshold_spin.clone();
    cleanup_row.connect_active_notify(move |switch| {
        if enable_row_clone2.is_active() {
            threshold_spin_clone2.set_sensitive(switch.is_active());
        }
    });

    // Apply button handler
    let parent_clone = parent.clone();
    let enable_row_clone = enable_row.clone();
    let type_row_clone = type_row.clone();
    let cleanup_row_clone = cleanup_row.clone();
    let limit_spin_clone = limit_spin.clone();
    let threshold_spin_clone = threshold_spin.clone();

    apply_button.connect_clicked(move |_| {
        // Build new config
        let enabled = enable_row_clone.is_active();
        let quota_type = match type_row_clone.selected() {
            0 => QuotaType::Simple,
            _ => QuotaType::Traditional,
        };
        let limit_gb = limit_spin_clone.value();
        let total_limit_bytes = if limit_gb > 0.0 {
            Some((limit_gb * 1024.0 * 1024.0 * 1024.0) as u64)
        } else {
            None
        };
        let cleanup_threshold = threshold_spin_clone.value() / 100.0;
        let auto_cleanup = cleanup_row_clone.is_active();

        let new_config = QuotaConfig {
            enabled,
            quota_type,
            total_limit_bytes,
            per_snapshot_limit_bytes: None, // Not configurable in UI yet
            cleanup_threshold,
            auto_cleanup,
        };

        // Apply quota settings via D-Bus (includes saving config)
        if let Err(e) = apply_quota_settings(&parent_clone, &new_config) {
            dialogs::show_error(
                &parent_clone,
                "Apply Failed",
                &format!("Failed to apply quota settings: {}", e),
            );
            return;
        }

        dialogs::show_toast(&parent_clone, "Quota settings applied successfully");
    });

    page
}

/// Apply quota settings via D-Bus
fn apply_quota_settings(
    _parent: &adw::ApplicationWindow,
    config: &QuotaConfig,
) -> anyhow::Result<()> {
    let client = WaypointHelperClient::new()?;

    // First, save the configuration via D-Bus
    let config_toml = toml::to_string_pretty(config)?;
    let msg = client.save_quota_config(config_toml)?;
    log::info!("{}", msg);

    if config.enabled {
        // Enable quotas
        let use_simple = matches!(config.quota_type, QuotaType::Simple);
        let msg = client.enable_quotas(use_simple)?;
        log::info!("{}", msg);

        // Set limit if specified
        if let Some(limit_bytes) = config.total_limit_bytes {
            let msg = client.set_quota_limit(limit_bytes)?;
            log::info!("{}", msg);
        }
    } else {
        // Disable quotas
        let msg = client.disable_quotas()?;
        log::info!("{}", msg);
    }

    Ok(())
}
