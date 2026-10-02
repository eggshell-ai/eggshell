// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let mut options = sentry::ClientOptions::default();
    options.release = sentry::release_name!();
    options.send_default_pii = true;
    options.before_send = Some(std::sync::Arc::new(|event| {
        let is_feedback = event.tags.get("type").map(|v| v.as_str()) == Some("feedback")
            || event.tags.contains_key("feedback_rating");
        if is_feedback || eggshell_lib::telemetry::is_telemetry_enabled() {
            Some(event)
        } else {
            None
        }
    }));

    let _guard = sentry::init((
        "https://e9b79ba89783f877e38040f014505180@o4512181149827072.ingest.us.sentry.io/4512181206122496",
        options,
    ));

    eggshell_lib::run()
}
