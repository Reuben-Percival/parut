use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::OnceLock;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct AppSettings {
    pub notifications_enabled: bool,
    pub compact_mode: bool,
    pub theme: String, // "default", "light", "dark"
    pub confirm_actions: bool, // legacy global flag
    pub check_updates_on_startup: bool,
    pub auto_refresh_interval: String, // off, 15m, 30m, 1h, 6h
    pub search_result_limit: usize,    // 50, 100, 250, 500
    pub aur_pkgbuild_required: bool,
    pub confirm_remove: bool,
    pub confirm_update_all: bool,
    pub confirm_clean_cache: bool,
    pub terminal_preference: String, // auto, gnome-terminal, konsole, xterm, xfce4-terminal, alacritty
    pub show_only_updates_from: String, // all, repo-only, aur-only
    pub default_sort_installed: u32,
    pub default_sort_search: u32,
    pub show_package_details_on_single_click: bool,
    pub notify_on_task_complete: bool,
    pub notify_on_task_failed: bool,
    pub log_level: String, // error, warn, info, debug
    pub max_log_size_mb: u64,
    pub show_arch_news: bool,
    pub arch_news_items: usize,
    pub show_arch_news_dates: bool,
    pub confirm_remove_orphans: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            notifications_enabled: true,
            compact_mode: false,
            theme: "default".to_string(),
            confirm_actions: true,
            check_updates_on_startup: true,
            auto_refresh_interval: "off".to_string(),
            search_result_limit: 100,
            aur_pkgbuild_required: true,
            confirm_remove: true,
            confirm_update_all: true,
            confirm_clean_cache: true,
            terminal_preference: "auto".to_string(),
            show_only_updates_from: "all".to_string(),
            default_sort_installed: 0,
            default_sort_search: 0,
            show_package_details_on_single_click: false,
            notify_on_task_complete: false,
            notify_on_task_failed: true,
            log_level: "info".to_string(),
            max_log_size_mb: 10,
            show_arch_news: true,
            arch_news_items: 5,
            show_arch_news_dates: true,
            confirm_remove_orphans: true,
        }
    }
}

pub static SETTINGS: OnceLock<Mutex<AppSettings>> = OnceLock::new();

pub fn init() {
    let settings = load_settings().unwrap_or_default();
    let _ = SETTINGS.set(Mutex::new(settings));
}

pub fn get() -> AppSettings {
    SETTINGS
        .get()
        .and_then(|s| s.lock().ok().map(|cfg| cfg.clone()))
        .unwrap_or_default()
}

pub fn update<F>(f: F)
where
    F: FnOnce(&mut AppSettings),
{
    if let Some(lock) = SETTINGS.get() {
        if let Ok(mut settings) = lock.lock() {
            f(&mut settings);
            let _ = save_settings(&settings);
        }
    }
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

#[cfg(test)]
mod tests {
    use super::AppSettings;

    #[test]
    fn settings_roundtrip_json() {
        let settings = AppSettings::default();
        let json = serde_json::to_string(&settings).expect("serialize settings");
        let parsed: AppSettings = serde_json::from_str(&json).expect("deserialize settings");
        assert_eq!(parsed.notifications_enabled, settings.notifications_enabled);
        assert_eq!(parsed.compact_mode, settings.compact_mode);
        assert_eq!(parsed.theme, settings.theme);
        assert_eq!(parsed.confirm_actions, settings.confirm_actions);
        assert_eq!(
            parsed.check_updates_on_startup,
            settings.check_updates_on_startup
        );
        assert_eq!(parsed.auto_refresh_interval, settings.auto_refresh_interval);
        assert_eq!(parsed.search_result_limit, settings.search_result_limit);
        assert_eq!(parsed.aur_pkgbuild_required, settings.aur_pkgbuild_required);
        assert_eq!(parsed.confirm_remove, settings.confirm_remove);
        assert_eq!(parsed.confirm_update_all, settings.confirm_update_all);
        assert_eq!(parsed.confirm_clean_cache, settings.confirm_clean_cache);
        assert_eq!(parsed.terminal_preference, settings.terminal_preference);
        assert_eq!(
            parsed.show_only_updates_from,
            settings.show_only_updates_from
        );
        assert_eq!(parsed.default_sort_installed, settings.default_sort_installed);
        assert_eq!(parsed.default_sort_search, settings.default_sort_search);
        assert_eq!(
            parsed.show_package_details_on_single_click,
            settings.show_package_details_on_single_click
        );
        assert_eq!(parsed.notify_on_task_complete, settings.notify_on_task_complete);
        assert_eq!(parsed.notify_on_task_failed, settings.notify_on_task_failed);
        assert_eq!(parsed.log_level, settings.log_level);
        assert_eq!(parsed.max_log_size_mb, settings.max_log_size_mb);
        assert_eq!(parsed.show_arch_news, settings.show_arch_news);
        assert_eq!(parsed.arch_news_items, settings.arch_news_items);
        assert_eq!(parsed.show_arch_news_dates, settings.show_arch_news_dates);
        assert_eq!(parsed.confirm_remove_orphans, settings.confirm_remove_orphans);
    }
}
