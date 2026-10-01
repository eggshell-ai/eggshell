use std::sync::atomic::{AtomicBool, Ordering};

static TELEMETRY_ENABLED: AtomicBool = AtomicBool::new(false);

/// Checks whether telemetry is currently active at runtime.
pub fn is_telemetry_enabled() -> bool {
    TELEMETRY_ENABLED.load(Ordering::SeqCst)
}

/// Updates the runtime telemetry enabled state.
pub fn set_telemetry_enabled(enabled: bool) {
    TELEMETRY_ENABLED.store(enabled, Ordering::SeqCst);
}

/// Binds the persistent installation ID to Sentry's user scope.
pub fn configure_scope(installation_id: &str) {
    sentry::configure_scope(|scope| {
        let mut user = sentry::User::default();
        user.id = Some(installation_id.to_string());
        scope.set_user(Some(user));
        scope.set_tag("installation_id", installation_id);
    });
}

/// Clears Sentry user scope when telemetry is turned off.
pub fn clear_scope() {
    sentry::configure_scope(|scope| {
        scope.set_user(None);
        scope.remove_tag("installation_id");
    });
}

/// Emits the application startup breadcrumb and event with the installation ID.
pub fn record_startup_event(installation_id: &str) {
    configure_scope(installation_id);

    sentry::with_scope(
        |scope| {
            let mut user = sentry::User::default();
            user.id = Some(installation_id.to_string());
            scope.set_user(Some(user));
            scope.set_tag("installation_id", installation_id);
        },
        || {
            sentry::add_breadcrumb(sentry::Breadcrumb {
                ty: "info".into(),
                message: Some("Application started".into()),
                ..Default::default()
            });
            sentry::capture_message("Application started", sentry::Level::Info);
        },
    );
}

/// Emits the conversation started breadcrumb and event with the installation ID if collection is enabled.
pub fn record_conversation_started_event(installation_id: &str) {
    if !is_telemetry_enabled() || installation_id.trim().is_empty() {
        return;
    }
    configure_scope(installation_id);

    sentry::with_scope(
        |scope| {
            let mut user = sentry::User::default();
            user.id = Some(installation_id.to_string());
            scope.set_user(Some(user));
            scope.set_tag("installation_id", installation_id);
        },
        || {
            sentry::add_breadcrumb(sentry::Breadcrumb {
                ty: "info".into(),
                message: Some("Conversation started".into()),
                ..Default::default()
            });
            sentry::capture_message("Conversation started", sentry::Level::Info);
        },
    );
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FeedbackMessagePayload {
    pub role: String,
    pub content: String,
}

/// Dispatches a user feedback event to Sentry with associated metadata.
pub fn record_feedback_event(
    installation_id: &str,
    app_version: &str,
    rating: &str,
    reason: Option<&str>,
    notes: Option<&str>,
    provider: Option<&str>,
    model: Option<&str>,
    prompt_history: Option<&[FeedbackMessagePayload]>,
    diagnostic_logs: Option<&[crate::logger::LogEntry]>,
) {
    sentry::with_scope(
        |scope| {
            let mut user = sentry::User::default();
            user.id = Some(installation_id.to_string());
            scope.set_user(Some(user));
            scope.set_tag("installation_id", installation_id);
            scope.set_tag("type", "feedback");
            scope.set_tag("feedback_rating", rating);
            scope.set_tag("app_version", app_version);
            if let Some(p) = provider {
                scope.set_tag("provider", p);
            }
            if let Some(m) = model {
                scope.set_tag("model", m);
            }
            if let Some(r) = reason {
                scope.set_tag("feedback_reason", r);
            }
            if let Some(n) = notes {
                if !n.trim().is_empty() {
                    scope.set_extra("feedback_notes", serde_json::Value::String(n.to_string()));
                }
            }
            if let Some(history) = prompt_history {
                if let Ok(val) = serde_json::to_value(history) {
                    scope.set_extra("prompt_history", val);
                }
            }
            if let Some(logs) = diagnostic_logs {
                if let Ok(val) = serde_json::to_value(logs) {
                    scope.set_extra("diagnostic_logs", val);
                }
            }
        },
        || {
            let msg = match (rating, reason) {
                ("thumbs_up", _) => "Feedback: Thumbs Up".to_string(),
                ("thumbs_down", Some(r)) => format!("Feedback: Thumbs Down - {r}"),
                ("thumbs_down", None) => "Feedback: Thumbs Down".to_string(),
                _ => format!("Feedback: {rating}"),
            };
            sentry::capture_message(&msg, sentry::Level::Info);
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_telemetry_toggle() {
        set_telemetry_enabled(false);
        assert!(!is_telemetry_enabled());
        set_telemetry_enabled(true);
        assert!(is_telemetry_enabled());
        set_telemetry_enabled(false);
    }

    #[test]
    fn test_record_conversation_started_when_disabled() {
        set_telemetry_enabled(false);
        // Should not panic or perform operations when disabled
        record_conversation_started_event("test-install-id");
    }

    #[test]
    fn test_record_conversation_started_empty_id() {
        set_telemetry_enabled(true);
        // Should not panic or set scope with empty ID
        record_conversation_started_event("");
        record_conversation_started_event("   ");
        set_telemetry_enabled(false);
    }

    #[test]
    fn test_record_conversation_started_when_enabled() {
        set_telemetry_enabled(true);
        record_conversation_started_event("test-install-id");
        set_telemetry_enabled(false);
    }
}

