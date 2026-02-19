use chrono::Local;
use std::fs::{OpenOptions, create_dir_all};
use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;

use crate::settings;

pub struct Logger {
    log_path: PathBuf,
}

impl Logger {
    pub fn new() -> Self {
        let log_dir = Self::get_log_dir();
        let log_path = log_dir.join("parut.log");

        // Create log directory if it doesn't exist
        if let Err(e) = create_dir_all(&log_dir) {
            eprintln!("Failed to create log directory: {}", e);
        }

        Self { log_path }
    }

    fn get_log_dir() -> PathBuf {
        // Use XDG data directory: ~/.local/share/parut on most systems.
        if let Some(data_dir) = dirs::data_local_dir() {
            data_dir.join("parut")
        } else {
            PathBuf::from("/tmp/parut")
        }
    }

    pub fn log(&self, level: LogLevel, message: &str) {
        if !Self::should_log(level) {
            return;
        }

        self.rotate_if_needed();

        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S");
        let log_entry = format!("[{}] {}: {}\n", timestamp, level.as_str(), message);

        // Try to write to file
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_path)
        {
            let _ = file.write_all(log_entry.as_bytes());
        }

        // Also print to stderr for important messages
        if matches!(level, LogLevel::Error | LogLevel::Warning) {
            eprint!("{}", log_entry);
        }
    }

    pub fn info(&self, message: &str) {
        self.log(LogLevel::Info, message);
    }

    pub fn warning(&self, message: &str) {
        self.log(LogLevel::Warning, message);
    }

    pub fn error(&self, message: &str) {
        self.log(LogLevel::Error, message);
    }

    pub fn debug(&self, message: &str) {
        self.log(LogLevel::Debug, message);
    }

    #[allow(dead_code)]
    pub fn get_log_path(&self) -> &PathBuf {
        &self.log_path
    }

    fn should_log(level: LogLevel) -> bool {
        let configured = settings::get().log_level;
        let threshold = match configured.as_str() {
            "error" => 0,
            "warn" => 1,
            "info" => 2,
            "debug" => 3,
            _ => 2,
        };
        level.rank() <= threshold
    }

    fn rotate_if_needed(&self) {
        let max_mb = settings::get().max_log_size_mb.max(1);
        let max_bytes = max_mb * 1024 * 1024;

        if let Ok(meta) = std::fs::metadata(&self.log_path) {
            if meta.len() <= max_bytes {
                return;
            }

            let rotated = self.log_path.with_extension("log.1");
            let _ = std::fs::remove_file(&rotated);
            let _ = std::fs::rename(&self.log_path, rotated);
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum LogLevel {
    Info,
    Warning,
    Error,
    Debug,
}

impl LogLevel {
    fn as_str(&self) -> &str {
        match self {
            LogLevel::Info => "INFO",
            LogLevel::Warning => "WARN",
            LogLevel::Error => "ERROR",
            LogLevel::Debug => "DEBUG",
        }
    }

    fn rank(&self) -> i32 {
        match self {
            LogLevel::Error => 0,
            LogLevel::Warning => 1,
            LogLevel::Info => 2,
            LogLevel::Debug => 3,
        }
    }
}

// Global logger instance
static LOGGER: OnceLock<Logger> = OnceLock::new();

pub fn get_logger() -> &'static Logger {
    LOGGER.get_or_init(Logger::new)
}

pub fn log_info(message: &str) {
    get_logger().info(message);
}

pub fn log_warning(message: &str) {
    get_logger().warning(message);
}

pub fn log_error(message: &str) {
    get_logger().error(message);
}

pub fn log_debug(message: &str) {
    get_logger().debug(message);
}
