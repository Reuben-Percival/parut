// Add these helper functions to the ParuGui impl block in ui.rs

/// Get installed package names (cached version for better performance)
fn get_installed_package_names() -> std::collections::HashSet<String> {
    use std::collections::HashSet;
    use std::process::Command;
    
    // Quick check using pacman -Qq (just names, much faster than -Q)
    if let Ok(output) = Command::new("pacman")
        .arg("-Qq")
        .output()
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            return stdout.lines().map(|s| s.to_string()).collect();
        }
    }
    
    HashSet::new()
}

/// Update search results with install/uninstall buttons
fn update_search_results(
    list_box: &ListBox,
    packages: &[Package],
    installed_names: &std::collections::HashSet<String>,
    task_queue: Arc<TaskQueue>
) {
    // Clear existing items
    while let Some(child) = list_box.first_child() {
        list_box.remove(&child);
    }

    if packages.is_empty() {
        let empty_label = Label::new(Some("No packages found"));
        empty_label.set_margin_top(24);
        empty_label.set_margin_bottom(24);
        list_box.append(&empty_label);
        return;
    }

    // Limit results to 100 for better performance
    let display_count = packages.len().min(100);
    
    for package in &packages[..display_count] {
        let is_installed = installed_names.contains(&package.name);
        let row = Self::create_search_result_row(package, is_installed, task_queue.clone());
        list_box.append(&row);
    }
    
    if packages.len() > 100 {
        let more_label = Label::new(Some(&format!(
            "... and {} more results (refine your search)", 
            packages.len() - 100
        )));
        more_label.add_css_class("dim-label");
        more_label.set_margin_top(12);
        more_label.set_margin_bottom(12);
        list_box.append(&more_label);
    }
}

/// Create a search result row with install or uninstall button
fn create_search_result_row(package: &Package, is_installed: bool, task_queue: Arc<TaskQueue>) -> Box {
    let row_box = Box::new(Orientation::Horizontal, 12);
    row_box.set_margin_start(12);
    row_box.set_margin_end(12);
    row_box.set_margin_top(8);
    row_box.set_margin_bottom(8);

    let info_box = Box::new(Orientation::Vertical, 4);
    info_box.set_hexpand(true);

    // Package name with installed indicator
    let name_box = Box::new(Orientation::Horizontal, 8);
    let name_label = Label::new(Some(&package.name));
    name_label.add_css_class("heading");
    name_label.set_halign(gtk4::Align::Start);
    name_box.append(&name_label);

    // Show installed badge if installed
    if is_installed {
        let installed_badge = Label::new(Some("installed"));
        installed_badge.add_css_class("caption");
        installed_badge.add_css_class("success");
        name_box.append(&installed_badge);
    }

    // Repository badge
    let repo_label = Label::new(Some(&package.repository));
    repo_label.add_css_class("caption");
    repo_label.add_css_class("dim-label");
    
    if package.repository == "aur" {
        repo_label.add_css_class("warning");
    }
    
    name_box.append(&repo_label);
    info_box.append(&name_box);

    // Version
    let version_label = Label::new(Some(&package.version));
    version_label.add_css_class("caption");
    version_label.set_halign(gtk4::Align::Start);
    info_box.append(&version_label);

    // Description (truncated for performance)
    if !package.description.is_empty() {
        let desc_label = Label::new(Some(&package.description));
        desc_label.add_css_class("caption");
        desc_label.add_css_class("dim-label");
        desc_label.set_halign(gtk4::Align::Start);
        desc_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        desc_label.set_max_width_chars(60);
        info_box.append(&desc_label);
    }

    row_box.append(&info_box);

    // Action buttons
    let action_box = Box::new(Orientation::Horizontal, 6);
    
    if is_installed {
        // Show uninstall button
        let uninstall_btn = Button::with_label("Uninstall");
        uninstall_btn.add_css_class("destructive-action");
        
        let pkg_name = package.name.clone();
        uninstall_btn.connect_clicked(move |_| {
            task_queue.add_task(TaskType::Remove, pkg_name.clone());
        });
        
        action_box.append(&uninstall_btn);
    } else {
        // Show install button
        let install_btn = Button::with_label("Install");
        install_btn.add_css_class("suggested-action");
        
        let pkg_name = package.name.clone();
        let pkg_repo = package.repository.clone();
        let row_box_weak = row_box.downgrade();
        
        install_btn.connect_clicked(move |_| {
            let is_aur = pkg_repo == "aur" || ParuBackend::is_aur_package(&pkg_name);
            
            if is_aur {
                if let Some(row_box) = row_box_weak.upgrade() {
                    Self::show_pkgbuild_dialog(&row_box, &pkg_name, task_queue.clone());
                }
            } else {
                task_queue.add_task(TaskType::Install, pkg_name.clone());
            }
        });
        
        action_box.append(&install_btn);
    }
    
    row_box.append(&action_box);
    row_box
}
