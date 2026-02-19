use crate::logger::{log_debug, log_error, log_info, log_warning};
use crate::settings;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Package {
    pub name: String,
    pub version: String,
    pub description: String,
    pub repository: String,
    pub installed_version: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PackageDetails {
    pub name: String,
    pub version: String,
    pub description: String,
    pub repository: String,
    pub url: String,
    pub licenses: String,
    pub groups: String,
    pub provides: String,
    pub depends_on: String,
    pub optional_deps: String,
    pub required_by: String,
    pub optional_for: String,
    pub conflicts_with: String,
    pub replaces: String,
    pub installed_size: String,
    pub packager: String,
    pub build_date: String,
    pub install_date: String,
    pub install_reason: String,
    pub install_script: String,
    pub validated_by: String,
    pub votes: String,
    pub popularity: String,
}

#[derive(Debug, Clone)]
pub struct NewsItem {
    pub title: String,
    pub link: String,
    pub published: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AurComment {
    pub author: String,
    pub date: String,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct CleanupEstimate {
    pub pacman_cache_bytes: u64,
    pub paru_clone_bytes: u64,
    pub orphan_count: usize,
}

pub struct ParuBackend;

impl ParuBackend {
    pub fn is_paru_installed() -> bool {
        Self::command_exists("paru")
    }

    // UPDATED: Added limit parameter for performance
    pub fn search_packages(query: &str, limit: Option<usize>) -> Result<Vec<Package>, String> {
        log_debug(&format!("Searching packages with query: {}", query));

        let output = Command::new("paru")
            .arg("-Ss")
            .arg(query)
            .output()
            .map_err(|e| {
                let err = format!("Failed to execute paru: {}", e);
                log_error(&err);
                err
            })?;

        if !output.status.success() {
            log_error("Paru search failed");
            return Err("Paru search failed".to_string());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut packages = Self::parse_search_output(&stdout);

        // Truncate results if a limit is specified
        if let Some(l) = limit
            && packages.len() > l
        {
            packages.truncate(l);
        }

        log_info(&format!(
            "Search completed: found {} packages",
            packages.len()
        ));
        Ok(packages)
    }

    pub fn list_installed() -> Result<Vec<Package>, String> {
        log_debug("Listing installed packages with descriptions");

        // 1. Get name and version
        let output = Command::new("pacman")
            .env("LANG", "C")
            .arg("-Q")
            .output()
            .map_err(|e| {
                let err = format!("Failed to execute pacman -Q: {}", e);
                log_error(&err);
                err
            })?;

        if !output.status.success() {
            log_error("Failed to list installed packages via pacman -Q");
            return Err("Failed to list installed packages".to_string());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut packages = Self::parse_installed_output(&stdout);

        // 2. Get all descriptions in one go via pacman -Qi
        // This is much faster than individual calls
        let mut descriptions = HashMap::new();
        if let Ok(details_output) = Command::new("pacman").env("LANG", "C").arg("-Qi").output()
            && details_output.status.success()
        {
            let details_stdout = String::from_utf8_lossy(&details_output.stdout);
            let mut current_name = String::new();
            for line in details_stdout.lines() {
                if line.starts_with("Name") {
                    if let Some(name) = line.split(':').nth(1) {
                        current_name = name.trim().to_string();
                    }
                } else if line.starts_with("Description")
                    && let Some(desc) = line.split(':').nth(1)
                    && !current_name.is_empty()
                {
                    descriptions.insert(current_name.clone(), desc.trim().to_string());
                }
            }
        }

        // 3. Get foreign packages
        let foreign_set = Self::get_foreign_packages();

        // 4. Update package objects
        for package in &mut packages {
            if let Some(desc) = descriptions.get(&package.name) {
                package.description = desc.clone();
            }

            if foreign_set.contains(&package.name) {
                package.repository = "aur".to_string();
            } else {
                package.repository = "repo".to_string();
            }
        }

        log_info(&format!(
            "Listed {} installed packages ({} from AUR)",
            packages.len(),
            foreign_set.len()
        ));
        Ok(packages)
    }

    pub fn list_updates() -> Result<Vec<Package>, String> {
        log_debug("Checking for available updates");

        let mut packages = Vec::new();
        let mut seen_packages = HashSet::new();

        // 1. Try to get official repo updates via checkupdates (safely syncs DB)
        // If checkupdates is missing, we rely on paru -Qu (local DB only)
        let use_checkupdates = Self::command_exists("checkupdates");

        if use_checkupdates {
            log_info("Using checkupdates for repo updates");
            match Command::new("checkupdates").output() {
                Ok(output) => {
                    if output.status.success() {
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        let updates = Self::parse_update_lines(&stdout, "repo");
                        for pkg in updates {
                            seen_packages.insert(pkg.name.clone());
                            packages.push(pkg);
                        }
                    }
                }
                Err(e) => log_error(&format!("checkupdates failed: {}", e)),
            }
        }

        // 2. Get AUR updates (or all if checkupdates failed/missing) via paru
        // -Qu: Upgradeable
        // -a: AUR only (if we used checkupdates), otherwise omit -a to get all
        let mut cmd = Command::new("paru");
        cmd.arg("-Qu").arg("--noconfirm"); // Non-interactive

        // If we successfully used checkupdates, we only need AUR from paru
        if use_checkupdates {
            cmd.arg("-a");
        }

        match cmd.output() {
            Ok(output) => {
                // paru returns 1 if no updates found, which is not an error for us
                if output.status.success() || output.status.code() == Some(1) {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let updates = Self::parse_update_lines(
                        &stdout,
                        if use_checkupdates { "aur" } else { "unknown" },
                    );

                    for pkg in updates {
                        if !seen_packages.contains(&pkg.name) {
                            seen_packages.insert(pkg.name.clone());
                            packages.push(pkg);
                        }
                    }
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    log_error(&format!("paru -Qu failed: {}", stderr));
                }
            }
            Err(e) => log_error(&format!("Failed to execute paru: {}", e)),
        }

        // If we didn't use checkupdates, we need to identify repos
        if !use_checkupdates && !packages.is_empty() {
            let foreign_set = Self::get_foreign_packages();
            let package_names: Vec<&str> = packages
                .iter()
                .filter(|p| !foreign_set.contains(&p.name))
                .map(|p| p.name.as_str())
                .collect();

            let repo_map = if !package_names.is_empty() {
                Self::get_repos_batch(&package_names)
            } else {
                HashMap::new()
            };

            for pkg in &mut packages {
                if foreign_set.contains(&pkg.name) {
                    pkg.repository = "aur".to_string();
                } else if let Some(repo) = repo_map.get(&pkg.name) {
                    pkg.repository = repo.clone();
                } else {
                    pkg.repository = "core".to_string(); // Fallback assumption
                }
            }
        }

        log_info(&format!("Found {} available updates", packages.len()));
        Ok(packages)
    }

    fn parse_update_lines(output: &str, default_repo: &str) -> Vec<Package> {
        output
            .lines()
            .filter_map(|line| {
                // Format: name old -> new
                // Filter out "[ignored]" or other noise
                let clean_line = line.replace("[ignored]", "").trim().to_string();
                let parts: Vec<&str> = clean_line.split_whitespace().collect();

                if parts.len() >= 4 {
                    // parts[0] might be 'repo/name' or just 'name'
                    let raw_name = parts[0];
                    let (repo, name) = if let Some((r, n)) = raw_name.split_once('/') {
                        (r, n)
                    } else {
                        (default_repo, raw_name)
                    };

                    Some(Package {
                        name: name.to_string(),
                        version: parts[3].to_string(),
                        description: String::new(),
                        repository: repo.to_string(),
                        installed_version: Some(parts[1].to_string()),
                    })
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn is_aur_package(name: &str) -> bool {
        let output = Command::new("pacman").arg("-Si").arg(name).output();

        if let Ok(output) = output
            && output.status.success()
        {
            return false;
        }

        true
    }

    pub fn get_pkgbuild(package_name: &str) -> Result<String, String> {
        log_debug(&format!("Fetching PKGBUILD for package: {}", package_name));

        let output = Command::new("paru")
            .arg("-Gp")
            .arg(package_name)
            .output()
            .map_err(|e| {
                let err = format!("Failed to fetch PKGBUILD: {}", e);
                log_error(&err);
                err
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let err = format!("Failed to get PKGBUILD: {}", stderr);
            log_error(&err);
            return Err(err);
        }

        let pkgbuild = String::from_utf8_lossy(&output.stdout).to_string();

        if pkgbuild.trim().is_empty() {
            let err = "PKGBUILD is empty or package not found".to_string();
            log_warning(&format!("{} for package: {}", err, package_name));
            return Err(err);
        }

        log_info(&format!(
            "Successfully fetched PKGBUILD for package: {}",
            package_name
        ));
        Ok(pkgbuild)
    }

    pub fn install_package<F>(
        name: &str,
        output_callback: F,
        cancel_requested: std::sync::Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Result<(), String>
    where
        F: Fn(String) + Send + Sync + 'static,
    {
        log_info(&format!("Starting installation of package: {}", name));
        let result = Self::run_paru_in_terminal(
            &["-S", "--noconfirm", name],
            output_callback,
            cancel_requested,
        );

        match &result {
            Ok(_) => log_info(&format!("Successfully installed package: {}", name)),
            Err(e) => log_error(&format!(
                "Failed to
                                       install package {}: {}",
                name, e
            )),
        }

        result
    }

    pub fn remove_package<F>(
        name: &str,
        output_callback: F,
        cancel_requested: std::sync::Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Result<(), String>
    where
        F: Fn(String) + Send + Sync + 'static,
    {
        log_info(&format!("Starting removal of package: {}", name));

        let result = Self::run_paru_in_terminal(
            &["-Rns", "--noconfirm", name],
            output_callback,
            cancel_requested,
        );

        match &result {
            Ok(_) => log_info(&format!("Successfully removed package: {}", name)),
            Err(e) => log_error(&format!(
                "Failed to
                                       remove package {}: {}",
                name, e
            )),
        }

        result
    }

    pub fn update_system<F>(
        output_callback: F,
        cancel_requested: std::sync::Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Result<(), String>
    where
        F: Fn(String) + Send + Sync + 'static,
    {
        log_info("Starting system update");
        let settings = settings::get();
        let mut args = vec!["-Syu", "--noconfirm"];
        match settings.default_update_scope.as_str() {
            "repo-only" => args.push("--repo"),
            "aur-only" => args.push("--aur"),
            _ => {}
        }

        let ignored: Vec<String> = settings
            .ignored_updates
            .iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let mut owned_args: Vec<String> = args.into_iter().map(ToString::to_string).collect();
        if !ignored.is_empty() {
            owned_args.push("--ignore".to_string());
            owned_args.push(ignored.join(","));
        }
        let arg_refs: Vec<&str> = owned_args.iter().map(String::as_str).collect();

        let result = Self::run_paru_in_terminal(&arg_refs, output_callback, cancel_requested);

        match &result {
            Ok(_) => log_info("System update completed successfully"),
            Err(e) => log_error(&format!("System update failed: {}", e)),
        }

        result
    }

    pub fn update_package<F>(
        name: &str,
        output_callback: F,
        cancel_requested: std::sync::Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Result<(), String>
    where
        F: Fn(String) + Send + Sync + 'static,
    {
        log_info(&format!("Starting update of package: {}", name));

        let result = Self::run_paru_in_terminal(
            &["-S", "--noconfirm", name],
            output_callback,
            cancel_requested,
        );

        match &result {
            Ok(_) => log_info(&format!("Successfully updated package: {}", name)),
            Err(e) => log_error(&format!("Package update failed for {}: {}", name, e)),
        }

        result
    }

    pub fn clean_cache<F>(
        output_callback: F,
        cancel_requested: std::sync::Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Result<(), String>
    where
        F: Fn(String) + Send + Sync + 'static,
    {
        log_info("Starting cache cleanup");

        // -Sc removes uninstalled packages from cache
        let result =
            Self::run_paru_in_terminal(&["-Sc", "--noconfirm"], output_callback, cancel_requested);

        match &result {
            Ok(_) => log_info("Cache cleanup completed successfully"),
            Err(e) => log_error(&format!("Cache cleanup failed: {}", e)),
        }

        result
    }

    pub fn estimate_cleanup() -> CleanupEstimate {
        let home = std::env::var("HOME").unwrap_or_default();
        let paru_clone = if home.is_empty() {
            0
        } else {
            Self::dir_size_bytes(&format!("{}/.cache/paru/clone", home))
        };
        let pacman_cache = Self::dir_size_bytes("/var/cache/pacman/pkg");

        let orphan_count = Command::new("pacman")
            .arg("-Qtdq")
            .output()
            .ok()
            .and_then(|o| {
                if o.status.success() {
                    let s = String::from_utf8_lossy(&o.stdout);
                    Some(s.lines().count())
                } else {
                    None
                }
            })
            .unwrap_or(0);

        CleanupEstimate {
            pacman_cache_bytes: pacman_cache,
            paru_clone_bytes: paru_clone,
            orphan_count,
        }
    }

    pub fn remove_orphans<F>(
        output_callback: F,
        cancel_requested: std::sync::Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Result<(), String>
    where
        F: Fn(String) + Send + Sync + 'static,
    {
        log_info("Starting orphan removal");

        // -c removes orphans (recursive)
        let result =
            Self::run_paru_in_terminal(&["-c", "--noconfirm"], output_callback, cancel_requested);

        match &result {
            Ok(_) => log_info("Orphan removal completed successfully"),
            Err(e) => log_error(&format!("Orphan removal failed: {}", e)),
        }

        result
    }

    pub fn get_package_details(name: &str) -> Result<PackageDetails, String> {
        let is_installed = Self::is_package_installed(name);

        // Use -Qi for installed, -Si for sync/aur
        let flag = if is_installed { "-Qi" } else { "-Si" };

        let output = Command::new("paru")
            .arg(flag)
            .arg(name)
            .output()
            .map_err(|e| format!("Failed to execute paru: {}", e))?;

        if !output.status.success() {
            return Err(format!("Failed to get details for {}", name));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Self::parse_package_details(&stdout, name)
    }

    pub fn fetch_arch_news(limit: usize) -> Result<Vec<NewsItem>, String> {
        let item_limit = limit.max(1);
        let output = Command::new("curl")
            .arg("-fsSL")
            .arg("https://archlinux.org/feeds/news/")
            .output()
            .map_err(|e| format!("Failed to execute curl: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Failed to fetch Arch news feed: {}", stderr.trim()));
        }

        let xml = String::from_utf8_lossy(&output.stdout);
        let mut items = Vec::new();
        for chunk in xml.split("<item>").skip(1) {
            let Some(end) = chunk.find("</item>") else {
                continue;
            };
            let item_xml = &chunk[..end];
            let title = Self::decode_html_entities(
                &Self::extract_xml_tag(item_xml, "title").unwrap_or_default(),
            );
            let link = Self::extract_xml_tag(item_xml, "link").unwrap_or_default();
            let published = Self::decode_html_entities(
                &Self::extract_xml_tag(item_xml, "pubDate").unwrap_or_default(),
            );

            if title.is_empty() || link.is_empty() {
                continue;
            }

            items.push(NewsItem {
                title,
                link,
                published,
            });

            if items.len() >= item_limit {
                break;
            }
        }

        if items.is_empty() {
            return Err("No news items were found in the feed".to_string());
        }
        Ok(items)
    }

    pub fn fetch_aur_comments(package_name: &str) -> Result<Vec<AurComment>, String> {
        log_debug(&format!("Fetching AUR comments for {}", package_name));
        let url = format!("https://aur.archlinux.org/packages/{}/", package_name);

        let output = Command::new("curl")
            .arg("-fsSL")
            .arg(&url)
            .output()
            .map_err(|e| format!("Failed to execute curl: {}", e))?;

        if !output.status.success() {
            return Err(format!("Failed to fetch AUR comments page: HTTP error"));
        }

        let html = String::from_utf8_lossy(&output.stdout);
        let mut comments = Vec::new();

        // Very basic HTML parsing for comments
        // Each comment starts with <h4 class="comment-header"> or <div id="comment-..."
        let parts: Vec<&str> = html.split("<h4 class=\"comment-header\">").collect();

        for part in parts.into_iter().skip(1) {
            // End of header
            let Some(header_end) = part.find("</h4>") else {
                continue;
            };
            let header = &part[..header_end];

            // Extract author: <a href="/users/AUTHOR/">AUTHOR</a>
            let author = if let Some(a_start) = header.find("/users/") {
                let sub = &header[a_start + 7..];
                if let Some(a_end) = sub.find('/') {
                    sub[..a_end].to_string()
                } else {
                    "unknown".to_string()
                }
            } else {
                "unknown".to_string()
            };

            // Extract date: title="Permalink to this comment">DATE</a>
            let date = if let Some(d_start) = header.find("title=\"Permalink to this comment\">") {
                let sub = &header[d_start + 34..];
                if let Some(d_end) = sub.find("</a>") {
                    sub[..d_end].to_string()
                } else {
                    "".to_string()
                }
            } else {
                "".to_string()
            };

            // Extract content: <div class="article-content comment-content">...</div>
            // Find content after header
            let content_part = &part[header_end..];
            let content = if let Some(c_start) =
                content_part.find("<div class=\"article-content comment-content\">")
            {
                let sub = &content_part[c_start + 45..];
                if let Some(c_end) = sub.find("</div>") {
                    let raw_content = &sub[..c_end];
                    // Clean up some basic HTML tags
                    raw_content
                        .replace("<p>", "")
                        .replace("</p>", "\n")
                        .replace("<br>", "\n")
                        .replace("<code>", "`")
                        .replace("</code>", "`")
                        .replace("<pre>", "```\n")
                        .replace("</pre>", "\n```")
                        .trim()
                        .to_string()
                } else {
                    "".to_string()
                }
            } else {
                "".to_string()
            };

            if !author.is_empty() && !content.is_empty() {
                comments.push(AurComment {
                    author,
                    date: Self::decode_html_entities(&date),
                    content: Self::decode_html_entities(&content),
                });
            }
        }

        log_info(&format!(
            "Fetched {} comments for {}",
            comments.len(),
            package_name
        ));
        Ok(comments)
    }

    fn is_package_installed(name: &str) -> bool {
        Command::new("pacman")
            .arg("-Qi")
            .arg(name)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn parse_package_details(output: &str, name: &str) -> Result<PackageDetails, String> {
        let mut details = PackageDetails {
            name: name.to_string(),
            version: String::new(),
            description: String::new(),
            repository: String::new(),
            url: String::new(),
            licenses: String::new(),
            groups: String::new(),
            provides: String::new(),
            depends_on: String::new(),
            optional_deps: String::new(),
            required_by: String::new(),
            optional_for: String::new(),
            conflicts_with: String::new(),
            replaces: String::new(),
            installed_size: String::new(),
            packager: String::new(),
            build_date: String::new(),
            install_date: String::new(),
            install_reason: String::new(),
            install_script: String::new(),
            validated_by: String::new(),
            votes: String::new(),
            popularity: String::new(),
        };

        for line in output.lines() {
            if let Some((key, value)) = line.split_once(':') {
                let key = key.trim();
                let value = value.trim().to_string();

                match key {
                    "Name" => details.name = value,
                    "Version" => details.version = value,
                    "Description" => details.description = value,
                    "Repository" => details.repository = value,
                    "URL" => details.url = value,
                    "Licenses" => details.licenses = value,
                    "Groups" => details.groups = value,
                    "Provides" => details.provides = value,
                    "Depends On" => details.depends_on = value,
                    "Optional Deps" => details.optional_deps = value,
                    "Required By" => details.required_by = value,
                    "Optional For" => details.optional_for = value,
                    "Conflicts With" => details.conflicts_with = value,
                    "Replaces" => details.replaces = value,
                    "Installed Size" => details.installed_size = value,
                    "Packager" => details.packager = value,
                    "Build Date" => details.build_date = value,
                    "Install Date" => details.install_date = value,
                    "Install Reason" => details.install_reason = value,
                    "Install Script" => details.install_script = value,
                    "Validated By" => details.validated_by = value,
                    "Votes" => details.votes = value,
                    "Popularity" => details.popularity = value,
                    _ => {}
                }
            }
        }

        Ok(details)
    }

    fn extract_xml_tag(input: &str, tag: &str) -> Option<String> {
        let open = format!("<{}>", tag);
        let close = format!("</{}>", tag);
        let start = input.find(&open)?;
        let after_open = start + open.len();
        let end_rel = input[after_open..].find(&close)?;
        Some(input[after_open..after_open + end_rel].trim().to_string())
    }

    fn decode_html_entities(input: &str) -> String {
        input
            .replace("&amp;", "&")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&#39;", "'")
    }

    fn run_paru_in_terminal<F>(
        args: &[&str],
        output_callback: F,
        cancel_requested: std::sync::Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Result<(), String>
    where
        F: Fn(String) + Send + Sync + 'static,
    {
        let preferred = settings::get().terminal_preference;
        let mut terminals = vec![
            "gnome-terminal".to_string(),
            "konsole".to_string(),
            "xterm".to_string(),
            "xfce4-terminal".to_string(),
            "alacritty".to_string(),
        ];
        if preferred != "auto" {
            terminals.retain(|t| *t != preferred);
            terminals.insert(0, preferred);
        }
        let mut terminal_found = false;
        let mut last_error = String::new();

        for terminal in terminals {
            if !Self::command_exists(&terminal) {
                continue;
            }
            terminal_found = true;

            let mut cmd = Command::new(&terminal);
            match terminal.as_str() {
                "gnome-terminal" => {
                    cmd.arg("--").arg("paru").args(args);
                }
                "konsole" | "xterm" | "xfce4-terminal" | "alacritty" => {
                    cmd.arg("-e").arg("paru").args(args);
                }
                _ => {}
            }

            output_callback(format!(
                "Running in terminal: {} paru {}",
                terminal,
                args.join(" ")
            ));
            match cmd.spawn() {
                Ok(mut child) => {
                    output_callback("Terminal opened - waiting for completion...".to_string());
                    loop {
                        if cancel_requested() {
                            let _ = child.kill();
                            let _ = child.wait();
                            output_callback("Task canceled by user.".to_string());
                            return Err("Task canceled by user".to_string());
                        }

                        match child.try_wait() {
                            Ok(Some(status)) => {
                                if status.success() {
                                    return Ok(());
                                }
                                return Err("Operation failed - check terminal output".to_string());
                            }
                            Ok(None) => {
                                std::thread::sleep(std::time::Duration::from_millis(200));
                            }
                            Err(e) => {
                                return Err(format!("Failed to wait for terminal: {}", e));
                            }
                        }
                    }
                }
                Err(e) => {
                    last_error = format!("Failed to spawn {}: {}", terminal, e);
                }
            }
        }

        if !terminal_found {
            Err(format!(
                "No terminal emulator found. Last error: {}",
                last_error
            ))
        } else {
            Err(last_error)
        }
    }

    fn command_exists(binary: &str) -> bool {
        std::env::var_os("PATH")
            .map(|paths| {
                std::env::split_paths(&paths)
                    .map(|p| p.join(binary))
                    .any(|full| full.is_file())
            })
            .unwrap_or(false)
    }

    fn dir_size_bytes(path: &str) -> u64 {
        Command::new("du")
            .arg("-sb")
            .arg(path)
            .output()
            .ok()
            .and_then(|o| {
                if !o.status.success() {
                    return None;
                }
                let s = String::from_utf8_lossy(&o.stdout);
                s.split_whitespace()
                    .next()
                    .and_then(|n| n.parse::<u64>().ok())
            })
            .unwrap_or(0)
    }

    fn parse_search_output(output: &str) -> Vec<Package> {
        let mut packages = Vec::new();
        let lines: Vec<&str> = output.lines().collect();

        let mut i = 0;
        while i < lines.len() {
            let line = lines[i].trim();

            if line.is_empty() {
                i += 1;
                continue;
            }

            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2
                && let Some((repository, name)) = parts[0].split_once('/')
            {
                let clean_version = parts[1].to_string();
                let installed_version = if line.contains("[installed:") {
                    let start = line
                        .find("[installed:")
                        .map(|idx| idx + "[installed:".len());
                    let end = line[start.unwrap_or(0)..]
                        .find(']')
                        .map(|idx| idx + start.unwrap_or(0));
                    match (start, end) {
                        (Some(s), Some(e)) if s < e => Some(line[s..e].trim().to_string()),
                        _ => Some(clean_version.clone()),
                    }
                } else if line.contains("[installed]") {
                    Some(clean_version.clone())
                } else {
                    None
                };

                let description = if i + 1 < lines.len() {
                    lines[i + 1].trim().to_string()
                } else {
                    String::new()
                };

                packages.push(Package {
                    name: name.to_string(),
                    version: clean_version,
                    description,
                    repository: repository.to_string(),
                    installed_version,
                });

                i += 2;
                continue;
            }

            i += 1;
        }

        packages
    }

    fn parse_installed_output(output: &str) -> Vec<Package> {
        output
            .lines()
            .filter_map(|line| {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    Some(Package {
                        name: parts[0].to_string(),
                        version: parts[1].to_string(),
                        description: String::new(),
                        repository: "unknown".to_string(),
                        installed_version: Some(parts[1].to_string()),
                    })
                } else {
                    None
                }
            })
            .collect()
    }

    fn get_foreign_packages() -> HashSet<String> {
        let mut foreign_set = HashSet::new();

        if let Ok(output) = Command::new("pacman").env("LANG", "C").arg("-Qm").output()
            && output.status.success()
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if let Some(name) = line.split_whitespace().next() {
                    foreign_set.insert(name.to_string());
                }
            }
        }

        foreign_set
    }

    fn get_repos_batch(package_names: &[&str]) -> HashMap<String, String> {
        let mut repo_map = HashMap::new();

        if package_names.is_empty() {
            return repo_map;
        }

        let mut cmd = Command::new("pacman");
        cmd.arg("-Si");
        for name in package_names {
            cmd.arg(name);
        }

        if let Ok(output) = cmd.output()
            && output.status.success()
        {
            let stdout = String::from_utf8_lossy(&output.stdout);

            let mut current_package = None;
            let mut current_repo = None;

            for line in stdout.lines() {
                if line.starts_with("Name") {
                    if let (Some(pkg), Some(repo)) = (current_package.take(), current_repo.take()) {
                        repo_map.insert(pkg, repo);
                    }

                    if let Some(name) = line.split(':').nth(1) {
                        current_package = Some(name.trim().to_string());
                    }
                } else if line.starts_with("Repository")
                    && let Some(repo) = line.split(':').nth(1)
                {
                    current_repo = Some(repo.trim().to_string());
                }
            }

            if let (Some(pkg), Some(repo)) = (current_package, current_repo) {
                repo_map.insert(pkg, repo);
            }
        }

        repo_map
    }

    #[allow(dead_code)]
    fn get_package_repositories(package_names: &[String]) -> HashMap<String, String> {
        let mut repo_map = HashMap::new();
        let foreign_packages = Self::get_foreign_packages();

        for package_name in package_names {
            if foreign_packages.contains(package_name) {
                repo_map.insert(package_name.clone(), "aur".to_string());
            } else {
                let output = Command::new("pacman").arg("-Si").arg(package_name).output();

                if let Ok(output) = output
                    && output.status.success()
                {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    for line in stdout.lines() {
                        if line.starts_with("Repository")
                            && let Some(repo) = line.split(':').nth(1)
                        {
                            repo_map.insert(package_name.clone(), repo.trim().to_string());
                            break;
                        }
                    }
                }

                repo_map
                    .entry(package_name.clone())
                    .or_insert_with(|| "unknown".to_string());
            }
        }

        repo_map
    }

    #[allow(dead_code)]
    fn parse_updates_output(output: &str) -> Vec<Package> {
        let package_names: Vec<String> = output
            .lines()
            .filter_map(|line| {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 4 {
                    Some(parts[0].to_string())
                } else {
                    None
                }
            })
            .collect();

        let repo_map = Self::get_package_repositories(&package_names);

        output
            .lines()
            .filter_map(|line| {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 4 {
                    let package_name = parts[0].to_string();
                    let repository = repo_map
                        .get(&package_name)
                        .cloned()
                        .unwrap_or_else(|| "unknown".to_string());

                    Some(Package {
                        name: package_name,
                        version: parts[3].to_string(),
                        description: String::new(),
                        repository,
                        installed_version: Some(parts[1].to_string()),
                    })
                } else {
                    None
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::ParuBackend;

    #[test]
    fn parses_update_lines_with_repo_prefix() {
        let input = "core/linux 6.12.1 -> 6.12.2\nparu 2.0.0 -> 2.1.0";
        let updates = ParuBackend::parse_update_lines(input, "unknown");
        assert_eq!(updates.len(), 2);
        assert_eq!(updates[0].name, "linux");
        assert_eq!(updates[0].repository, "core");
        assert_eq!(updates[0].installed_version.as_deref(), Some("6.12.1"));
        assert_eq!(updates[0].version, "6.12.2");
        assert_eq!(updates[1].name, "paru");
        assert_eq!(updates[1].repository, "unknown");
    }

    #[test]
    fn parses_search_output_pairs() {
        let input = "extra/ripgrep 14.1.0-1\n    A fast line-oriented search tool\naur/parut-git 0.2.0-1\n    GUI for paru";
        let pkgs = ParuBackend::parse_search_output(input);
        assert_eq!(pkgs.len(), 2);
        assert_eq!(pkgs[0].name, "ripgrep");
        assert_eq!(pkgs[0].repository, "extra");
        assert_eq!(pkgs[0].description, "A fast line-oriented search tool");
        assert_eq!(pkgs[1].name, "parut-git");
        assert_eq!(pkgs[1].repository, "aur");
    }

    #[test]
    fn parses_installed_output_lines() {
        let input = "gtk4 1:4.16.12-1\nparu 2.1.0-1";
        let installed = ParuBackend::parse_installed_output(input);
        assert_eq!(installed.len(), 2);
        assert_eq!(installed[0].name, "gtk4");
        assert_eq!(
            installed[0].installed_version.as_deref(),
            Some("1:4.16.12-1")
        );
    }

    #[test]
    fn extracts_xml_tag_value() {
        let input = "<item><title>Arch News</title></item>";
        let title = ParuBackend::extract_xml_tag(input, "title");
        assert_eq!(title.as_deref(), Some("Arch News"));
    }
}
