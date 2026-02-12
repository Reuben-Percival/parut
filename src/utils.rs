use crate::logger::{log_error, log_info};
use std::process::Command;

pub fn send_notification(title: &str, body: &str) {
    if is_notify_send_available() {
        let _ = Command::new("notify-send")
            .arg("--app-name=Parut")
            .arg("--icon=system-software-install")
            .arg(title)
            .arg(body)
            .spawn()
            .map_err(|e| log_error(&format!("Failed to send notification: {}", e)));
    } else {
        log_info(&format!(
            "Notification skipped (notify-send not found): {}: {}",
            title, body
        ));
    }
}

fn is_notify_send_available() -> bool {
    std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths)
                .map(|p| p.join("notify-send"))
                .any(|full| full.is_file())
        })
        .unwrap_or(false)
}
