use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::OnceLock;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppSettings {
    pub notifications_enabled: bool,
    pub compact_mode: bool,
    pub theme: String, // "default", "light", "dark"
    pub confirm_actions: bool,
    pub check_updates_on_startup: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            notifications_enabled: true,
            compact_mode: false,
            theme: "default".to_string(),
            confirm_actions: true,
            check_updates_on_startup: true,
        }
    }
}

pub static SETTINGS: OnceLock<Mutex<AppSettings>> = OnceLock::new();

pub fn init() {
    let settings = load_settings().unwrap_or_default();
    let _ = SETTINGS.set(Mutex::new(settings));
}

pub fn get() -> AppSettings {
    SETTINGS.get().unwrap().lock().unwrap().clone()
}

pub fn update<F>(f: F)
where
    F: FnOnce(&mut AppSettings),
{
    let mut settings = SETTINGS.get().unwrap().lock().unwrap();
    f(&mut settings);
    let _ = save_settings(&settings);
}

fn get_config_path() -> PathBuf {
    let mut path = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push("parut");
    std::fs::create_dir_all(&path).ok();
    path.push("settings.json");
    path
}

fn load_settings() -> Option<AppSettings> {
    let path = get_config_path();
    if let Ok(content) = fs::read_to_string(path) {
        serde_json::from_str(&content).ok()
    } else {
        None
    }
}

fn save_settings(settings: &AppSettings) -> anyhow::Result<()> {
    let path = get_config_path();
    let content = serde_json::to_string_pretty(settings)?;
    fs::write(path, content)?;
    Ok(())
}
