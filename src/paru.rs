use std::process::{Command, Stdio};
use std::io::{BufRead, BufReader};
use std::collections::{HashMap, HashSet};
use crate::logger::{log_info, log_error, log_warning, log_debug};

#[derive(Debug, Clone)]
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
}

pub struct ParuBackend;

impl ParuBackend {
    pub fn is_paru_installed() -> bool {
        Command::new("which")
        .arg("paru")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
    }

    // UPDATED: Added limit parameter for performance
    pub fn search_packages(query: &str, limit: Option<usize>) ->
    Result<Vec<Package>, String> {
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
        if let Some(l) = limit {
            if packages.len() > l {
                packages.truncate(l);
            }
        }

        log_info(&format!("Search completed: found {} packages",
                          packages.len()));
        Ok(packages)
    }

    pub fn list_installed() -> Result<Vec<Package>, String> {
        log_debug("Listing installed packages");

        let output = Command::new("pacman")
        .arg("-Q")
        .output()
        .map_err(|e| {
            let err = format!("Failed to execute pacman: {}", e);
            log_error(&err);
            err
        })?;

        if !output.status.success() {
            log_error("Failed to list installed packages");
            return Err("Failed to list installed packages".to_string());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut packages = Self::parse_installed_output(&stdout);

        let foreign_output = Command::new("pacman")
        .arg("-Qm")
        .output();

        let mut foreign_set = HashSet::new();
        if let Ok(output) = foreign_output {
            if output.status.success() {
                let foreign_stdout = String::from_utf8_lossy(&output.stdout);
                for line in foreign_stdout.lines() {
                    if let Some(name) = line.split_whitespace().next() {
                        foreign_set.insert(name.to_string());
                    }
                }
            }
        }

        for package in &mut packages {
            if foreign_set.contains(&package.name) {
                package.repository = "aur".to_string();
            } else {
                package.repository = "repo".to_string();
            }
        }

        log_info(&format!("Listed {} installed packages ({} from AUR)",
                          packages.len(), foreign_set.len()));
        Ok(packages)
    }

    pub fn list_updates() -> Result<Vec<Package>, String> {
        log_debug("Checking for available updates");

        let mut packages = Vec::new();
        let mut seen_packages = HashSet::new();

        // 1. Try to get official repo updates via checkupdates (safely syncs DB)
        // If checkupdates is missing, we rely on paru -Qu (local DB only)
        let use_checkupdates = Command::new("which")
            .arg("checkupdates")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

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
                    let updates = Self::parse_update_lines(&stdout, if use_checkupdates { "aur" } else { "unknown" });
                    
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
             let package_names: Vec<&str> = packages.iter()
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
         output.lines()
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
        let output = Command::new("pacman")
        .arg("-Si")
        .arg(name)
        .output();

        if let Ok(output) = output {
            if output.status.success() {
                return false;
            }
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

        log_info(&format!("Successfully fetched PKGBUILD for package: {}",
                          package_name));
        Ok(pkgbuild)
    }

    pub fn install_package<F>(name: &str, output_callback: F) -> Result<(),
    String>
    where
    F: Fn(String) + Send + 'static,
    {
        log_info(&format!("Starting installation of package: {}", name));

        let is_aur = Self::is_aur_package(name);
        log_debug(&format!("Package {} is from: {}", name, if is_aur { "AUR" }
        else { "official repos" }));

        let result = if is_aur {
            Self::run_paru_without_root(&["-S", "--noconfirm", name],
                                        output_callback)
        } else {
            Self::run_paru_with_output(&["-S", "--noconfirm", name],
                                       output_callback)
        };

        match &result {
            Ok(_) => log_info(&format!("Successfully installed package: {}",
                                       name)),
                                       Err(e) => log_error(&format!("Failed to
                                       install package {}: {}", name, e)),
        }

        result
    }

    pub fn remove_package<F>(name: &str, output_callback: F) -> Result<(),
    String>
    where
    F: Fn(String) + Send + 'static,
    {
        log_info(&format!("Starting removal of package: {}", name));

        let result = Self::run_paru_with_output(&["-Rns", "--noconfirm", name],
                                                output_callback);

        match &result {
            Ok(_) => log_info(&format!("Successfully removed package: {}",
                                       name)),
                                       Err(e) => log_error(&format!("Failed to
                                       remove package {}: {}", name, e)),
        }

        result
    }

    pub fn update_system<F>(output_callback: F) -> Result<(), String>
    where
    F: Fn(String) + Send + 'static,
    {
        log_info("Starting system update");

        let result = Self::run_paru_without_root(&["-Syu", "--noconfirm"],
                                                 output_callback);

        match &result {
            Ok(_) => log_info("System update completed successfully"),
            Err(e) => log_error(&format!("System update failed: {}", e)),
        }

        result
    }

    pub fn clean_cache<F>(output_callback: F) -> Result<(), String>
    where
    F: Fn(String) + Send + 'static,
    {
        log_info("Starting cache cleanup");

        // -Sc removes uninstalled packages from cache
        let result = Self::run_paru_without_root(&["-Sc", "--noconfirm"],
                                                 output_callback);

        match &result {
            Ok(_) => log_info("Cache cleanup completed successfully"),
            Err(e) => log_error(&format!("Cache cleanup failed: {}", e)),
        }

        result
    }



    pub fn remove_orphans<F>(output_callback: F) -> Result<(), String>
    where
    F: Fn(String) + Send + 'static,
    {
        log_info("Starting orphan removal");

        // -c removes orphans (recursive)
        let result = Self::run_paru_without_root(&["-c", "--noconfirm"],
                                                 output_callback);

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
        };

        for line in output.lines() {
            if let Some((key, value)) = line.split_once(':') {
                let key = key.trim();
                let value = value.trim().to_string();

                match key {
                    "Name" => details.name = value,
                    "Version" => details.version = value,
                    "Description" => details.description = value,
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
                    _ => {}
                }
            }
        }

        Ok(details)
    }

    fn run_paru_with_output<F>(args: &[&str], output_callback: F) -> Result<(),
    String>
    where
    F: Fn(String) + Send + 'static,
    {
        let mut cmd = Command::new("pkexec");
        cmd.arg("paru");
        for arg in args {
            cmd.arg(arg);
        }

        cmd.stdout(Stdio::piped())
        .stderr(Stdio::piped());

        let mut child = cmd.spawn()
        .map_err(|e| format!("Failed to spawn paru: {}. Make sure pkexec is
        installed.", e))?;

        if let Some(stdout) = child.stdout.take() {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                if let Ok(line) = line {
                    output_callback(line);
                }
            }
        }

        if let Some(stderr) = child.stderr.take() {
            let reader = BufReader::new(stderr);
            for line in reader.lines() {
                if let Ok(line) = line {
                    output_callback(format!("ERROR: {}", line));
                }
            }
        }

        let status = child.wait()
        .map_err(|e| format!("Failed to wait for paru: {}", e))?;

        if status.success() {
            Ok(())
        } else {
            Err("Operation failed".to_string())
        }
    }

    fn run_paru_without_root<F>(args: &[&str], output_callback: F) ->
    Result<(), String>
    where
    F: Fn(String) + Send + 'static,
    {
        let paru_cmd = format!("paru {}", args.join(" "));

        let terminals = [
            ("gnome-terminal", vec!["--", "bash", "-c"]),
            ("konsole", vec!["-e", "bash", "-c"]),
            ("xterm", vec!["-e", "bash", "-c"]),
            ("xfce4-terminal", vec!["-e", "bash", "-c"]),
            ("alacritty", vec!["-e", "bash", "-c"]),
        ];

        let terminal_found = false;
        let mut last_error = String::new();

        for (terminal, mut args_prefix) in terminals {
            if Command::new("which").arg(terminal).output().map(|o|
                o.status.success()).unwrap_or(false) {
                    output_callback(format!("Running: {} {}", terminal,
                                            paru_cmd));

                    // The terminal will close automatically upon completion
                    let full_cmd = paru_cmd.clone();
                    args_prefix.push(&full_cmd);

                    let mut cmd = Command::new(terminal);
                    for arg in &args_prefix {
                        cmd.arg(arg);
                    }

                    match cmd.spawn() {
                        Ok(mut child) => {
                            output_callback("Terminal opened - waiting for
                            completion...".to_string());

                            let status = child.wait()
                            .map_err(|e| format!("Failed to wait for terminal:
                            {}", e))?;

                            if status.success() {
                                return Ok(());
                            } else {
                                return Err("Operation failed - check terminal
                                output".to_string());
                            }
                        }
                        Err(e) => {
                            last_error = format!("Failed to spawn {}: {}",
                                                 terminal, e);
                            continue;
                        }
                    }
                }
        }

        if !terminal_found {
            Err(format!("No terminal emulator found. Last error: {}",
                        last_error))
        } else {
            Err(last_error)
        }
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

            if let Some((repo_name, version)) = line.split_once(' ') {
                if let Some((repository, name)) = repo_name.split_once('/') {
                    let description = if i + 1 < lines.len() {
                        lines[i + 1].trim().to_string()
                    } else {
                        String::new()
                    };

                    packages.push(Package {
                        name: name.to_string(),
                                  version: version.to_string(),
                                  description,
                                  repository: repository.to_string(),
                                  installed_version: None,
                    });

                    i += 2;
                    continue;
                }
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

    fn parse_updates_output_optimized(output: &str) -> Vec<Package> {
        let updates: Vec<(String, String, String)> = output
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 {
                Some((
                    parts[0].to_string(),
                      parts[1].to_string(),
                      parts[3].to_string(),
                ))
            } else {
                None
            }
        })
        .collect();

        if updates.is_empty() {
            return Vec::new();
        }

        let foreign_set = Self::get_foreign_packages();

        let package_names: Vec<&str> = updates
        .iter()
        .filter(|(name, _, _)| !foreign_set.contains(name))
        .map(|(name, _, _)| name.as_str())
        .collect();

        let repo_map = if !package_names.is_empty() {
            Self::get_repos_batch(&package_names)
        } else {
            HashMap::new()
        };

        updates
        .into_iter()
        .map(|(name, old_version, new_version)| {
            let repository = if foreign_set.contains(&name) {
                "aur".to_string()
            } else {
                repo_map
                .get(&name)
                .cloned()
                .unwrap_or_else(|| "core".to_string())
            };

            Package {
                name,
                version: new_version,
                description: String::new(),
             repository,
             installed_version: Some(old_version),
            }
        })
        .collect()
    }

    fn get_foreign_packages() -> HashSet<String> {
        let mut foreign_set = HashSet::new();

        if let Ok(output) = Command::new("pacman").arg("-Qm").output() {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    if let Some(name) = line.split_whitespace().next() {
                        foreign_set.insert(name.to_string());
                    }
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

        if let Ok(output) = cmd.output() {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);

                let mut current_package = None;
                let mut current_repo = None;

                for line in stdout.lines() {
                    if line.starts_with("Name") {
                        if let (Some(pkg), Some(repo)) =
                            (current_package.take(), current_repo.take()) {
                                repo_map.insert(pkg, repo);
                            }

                            if let Some(name) = line.split(':').nth(1) {
                                current_package = Some(name.trim().to_string());
                            }
                    } else if line.starts_with("Repository") {
                        if let Some(repo) = line.split(':').nth(1) {
                            current_repo = Some(repo.trim().to_string());
                        }
                    }
                }

                if let (Some(pkg), Some(repo)) = (current_package,
                    current_repo) {
                    repo_map.insert(pkg, repo);
                    }
            }
        }

        repo_map
    }

    #[allow(dead_code)]
    fn get_package_repositories(package_names: &[String]) -> HashMap<String,
    String> {
        let mut repo_map = HashMap::new();
        let foreign_packages = Self::get_foreign_packages();

        for package_name in package_names {
            if foreign_packages.contains(package_name) {
                repo_map.insert(package_name.clone(), "aur".to_string());
            } else {
                let output = Command::new("pacman")
                .arg("-Si")
                .arg(package_name)
                .output();

                if let Ok(output) = output {
                    if output.status.success() {
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        for line in stdout.lines() {
                            if line.starts_with("Repository") {
                                if let Some(repo) = line.split(':').nth(1) {
                                    repo_map.insert(package_name.clone(),
                                                    repo.trim().to_string());
                                    break;
                                }
                            }
                        }
                    }
                }

                repo_map.entry(package_name.clone()).or_insert_with(||
                "unknown".to_string());
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
                    let repository = repo_map.get(&package_name)
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
