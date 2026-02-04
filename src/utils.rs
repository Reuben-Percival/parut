
use std::process::Command;
use crate::logger::{log_info, log_error};

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
        log_info(&format!("Notification skipped (notify-send not found): {}: {}", title, body));
    }
}

fn is_notify_send_available() -> bool {
    Command::new("which")
        .arg("notify-send")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}
