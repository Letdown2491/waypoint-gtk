//! Structured audit logging for security events

use chrono::Utc;

/// Audit log entry for security-relevant events
#[derive(Debug, serde::Serialize)]
struct AuditEvent {
    timestamp: String,
    user_id: String,
    user_name: Option<String>,
    process_id: u32,
    operation: String,
    resource: String,
    result: String,
    details: Option<String>,
}

impl AuditEvent {
    fn new(
        user_id: String,
        process_id: u32,
        operation: &str,
        resource: &str,
        result: &str,
    ) -> Self {
        // Try to get username from UID
        let user_name = get_username_from_uid(&user_id);

        Self {
            timestamp: Utc::now().to_rfc3339(),
            user_id,
            user_name,
            process_id,
            operation: operation.to_string(),
            resource: resource.to_string(),
            result: result.to_string(),
            details: None,
        }
    }

    fn with_details(mut self, details: String) -> Self {
        self.details = Some(details);
        self
    }

    /// Log the audit event as structured JSON
    fn log(&self) {
        // Log as JSON for easy parsing by audit tools
        if let Ok(json) = serde_json::to_string(self) {
            log::info!(target: "audit", "{json}");
        } else {
            // Fallback to unstructured if serialization fails
            log::info!(
                target: "audit",
                "user={} pid={} operation={} resource={} result={}",
                self.user_id,
                self.process_id,
                self.operation,
                self.resource,
                self.result
            );
        }
    }
}

/// Get username from UID (best effort)
fn get_username_from_uid(uid_str: &str) -> Option<String> {
    use std::process::Command;

    let output = Command::new("id")
        .arg("-un")
        .arg(uid_str)
        .output()
        .ok()?;

    if output.status.success() {
        String::from_utf8(output.stdout)
            .ok()
            .map(|s| s.trim().to_string())
    } else {
        None
    }
}

/// Log a snapshot creation event
pub fn log_snapshot_create(
    user_id: String,
    process_id: u32,
    snapshot_name: &str,
    success: bool,
    error: Option<&str>,
) {
    let result = if success { "success" } else { "failure" };
    let mut event = AuditEvent::new(
        user_id,
        process_id,
        "create_snapshot",
        snapshot_name,
        result,
    );

    if let Some(err) = error {
        event = event.with_details(format!("error: {err}"));
    }

    event.log();
}

/// Log a snapshot deletion event
pub fn log_snapshot_delete(
    user_id: String,
    process_id: u32,
    snapshot_name: &str,
    success: bool,
    error: Option<&str>,
) {
    let result = if success { "success" } else { "failure" };
    let mut event = AuditEvent::new(
        user_id,
        process_id,
        "delete_snapshot",
        snapshot_name,
        result,
    );

    if let Some(err) = error {
        event = event.with_details(format!("error: {err}"));
    }

    event.log();
}

/// Log a snapshot restore/rollback event
pub fn log_snapshot_restore(
    user_id: String,
    process_id: u32,
    snapshot_name: &str,
    success: bool,
    error: Option<&str>,
) {
    let result = if success { "success" } else { "failure" };
    let mut event = AuditEvent::new(
        user_id,
        process_id,
        "restore_snapshot",
        snapshot_name,
        result,
    );

    if let Some(err) = error {
        event = event.with_details(format!("error: {err}"));
    }

    event.log();
}

/// Log a configuration change event
pub fn log_config_change(
    user_id: String,
    process_id: u32,
    config_type: &str,
    success: bool,
    error: Option<&str>,
) {
    let result = if success { "success" } else { "failure" };
    let mut event = AuditEvent::new(
        user_id,
        process_id,
        "modify_configuration",
        config_type,
        result,
    );

    if let Some(err) = error {
        event = event.with_details(format!("error: {err}"));
    }

    event.log();
}

/// Log an authorization failure
pub fn log_auth_failure(
    user_id: String,
    process_id: u32,
    operation: &str,
    reason: &str,
) {
    let event = AuditEvent::new(
        user_id,
        process_id,
        operation,
        "authorization",
        "denied",
    ).with_details(format!("reason: {reason}"));

    event.log();
}
