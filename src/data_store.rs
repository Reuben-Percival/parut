use crate::paru::Package;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct AppData {
    pub favorites: Vec<String>,
    pub recent_searches: Vec<String>,
    pub search_counts: HashMap<String, u64>,
    pub cached_installed: Vec<Package>,
    pub cached_updates: Vec<Package>,
    pub cached_installed_at: Option<i64>,
    pub cached_updates_at: Option<i64>,
}

pub static DATA: OnceLock<Mutex<AppData>> = OnceLock::new();

pub fn init() {
    let data = load_data().unwrap_or_default();
    let _ = DATA.set(Mutex::new(data));
}

fn get_data_path() -> PathBuf {
    let mut path = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push("parut");
    let _ = fs::create_dir_all(&path);
    path.push("data.json");
    path
}

fn load_data() -> Option<AppData> {
    let path = get_data_path();
    fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
}

fn save_data(data: &AppData) {
    let path = get_data_path();
    if let Ok(raw) = serde_json::to_string_pretty(data) {
        let _ = fs::write(path, raw);
    }
}

fn with_data_mut<F, T>(f: F) -> Option<T>
where
    F: FnOnce(&mut AppData) -> T,
{
    let lock = DATA.get()?;
    let mut data = lock.lock().ok()?;
    let out = f(&mut data);
    save_data(&data);
    Some(out)
}

fn with_data<F, T>(f: F) -> Option<T>
where
    F: FnOnce(&AppData) -> T,
{
    let lock = DATA.get()?;
    let data = lock.lock().ok()?;
    Some(f(&data))
}

pub fn toggle_favorite(name: &str) -> bool {
    with_data_mut(|data| {
        if data.favorites.iter().any(|p| p == name) {
            data.favorites.retain(|p| p != name);
            false
        } else {
            data.favorites.push(name.to_string());
            data.favorites.sort();
            data.favorites.dedup();
            true
        }
    })
    .unwrap_or(false)
}

pub fn is_favorite(name: &str) -> bool {
    with_data(|data| data.favorites.iter().any(|p| p == name)).unwrap_or(false)
}

pub fn favorites() -> Vec<String> {
    with_data(|data| data.favorites.clone()).unwrap_or_default()
}

pub fn record_search(query: &str) {
    let q = query.trim().to_lowercase();
    if q.len() < 2 {
        return;
    }

    let _ = with_data_mut(|data| {
        data.recent_searches.retain(|s| s != &q);
        data.recent_searches.insert(0, q.clone());
        data.recent_searches.truncate(12);
        *data.search_counts.entry(q).or_insert(0) += 1;
    });
}

pub fn recent_searches(limit: usize) -> Vec<String> {
    with_data(|data| data.recent_searches.iter().take(limit).cloned().collect()).unwrap_or_default()
}

pub fn trending_searches(limit: usize) -> Vec<String> {
    with_data(|data| {
        let mut items: Vec<(String, u64)> = data
            .search_counts
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        items.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        items.into_iter().take(limit).map(|(k, _)| k).collect()
    })
    .unwrap_or_default()
}

pub fn set_cached_installed(packages: &[Package]) {
    let _ = with_data_mut(|data| {
        data.cached_installed = packages.to_vec();
        data.cached_installed_at = Some(chrono::Local::now().timestamp());
    });
}

pub fn set_cached_updates(packages: &[Package]) {
    let _ = with_data_mut(|data| {
        data.cached_updates = packages.to_vec();
        data.cached_updates_at = Some(chrono::Local::now().timestamp());
    });
}

pub fn cached_installed() -> Vec<Package> {
    with_data(|data| data.cached_installed.clone()).unwrap_or_default()
}

pub fn cached_updates() -> Vec<Package> {
    with_data(|data| data.cached_updates.clone()).unwrap_or_default()
}

pub fn cached_installed_at() -> Option<i64> {
    with_data(|data| data.cached_installed_at).unwrap_or(None)
}

pub fn cached_updates_at() -> Option<i64> {
    with_data(|data| data.cached_updates_at).unwrap_or(None)
}
