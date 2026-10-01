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
