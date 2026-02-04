use gtk4::prelude::*;
use gtk4::{
    glib, Box, Button, Label, ListBox, Orientation, 
    ScrolledWindow, SearchEntry, Separator,
    Window, ProgressBar, TextView, Spinner, Image,
};
use adw::{HeaderBar, StatusPage, ViewStack, ViewSwitcher};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;
use crate::paru::{Package, ParuBackend};
use crate::task_queue::{TaskQueue, TaskWorker, TaskType, TaskStatus};
use crate::logger::{log_info, log_error};

pub struct ParuGui {
    main_box: Box,
    #[allow(dead_code)]
    search_results: Rc<RefCell<Vec<Package>>>,
    installed_packages: Rc<RefCell<Vec<Package>>>,
    updates: Rc<RefCell<Vec<Package>>>,
    task_queue: Arc<TaskQueue>,
    last_refresh_label: Rc<RefCell<Label>>,
}

impl ParuGui {
    pub fn new() -> Self {
        let main_box = Box::new(Orientation::Vertical, 0);
        
        // Create task queue
        let task_queue = Arc::new(TaskQueue::new());
        
        // Start the worker thread
        let worker = TaskWorker::new(task_queue.clone());
        worker.start();
        
        // Create header bar with modern styling
        let header_bar = HeaderBar::new();
        header_bar.set_show_end_title_buttons(true);
        
        // App title with icon
        let title_box = Box::new(Orientation::Horizontal, 8);
        let app_icon = Image::from_icon_name("system-software-install-symbolic");
        app_icon.set_pixel_size(24);
        app_icon.add_css_class("accent");
        title_box.append(&app_icon);
        
        let title_label = Label::new(Some("Parut"));
        title_label.add_css_class("title");
        title_box.append(&title_label);
        
        header_bar.set_title_widget(Some(&title_box));

        // Last refresh time label in header
        let last_refresh_label = Label::new(Some("Not refreshed"));
        last_refresh_label.add_css_class("caption");
        last_refresh_label.add_css_class("dim-label");
        last_refresh_label.set_margin_start(12);
        last_refresh_label.set_margin_end(12);
        header_bar.pack_start(&last_refresh_label);
        let last_refresh_label_rc = Rc::new(RefCell::new(last_refresh_label));

        // Queue button with badge
        let queue_box = Box::new(Orientation::Horizontal, 4);
        let queue_icon = Image::from_icon_name("view-list-symbolic");
        queue_box.append(&queue_icon);
        let queue_label = Label::new(Some("Queue"));
        queue_box.append(&queue_label);
        
        let queue_btn = Button::new();
        queue_btn.set_child(Some(&queue_box));
        queue_btn.add_css_class("flat");
        queue_btn.set_tooltip_text(Some("View task queue"));
        
        let task_queue_clone = task_queue.clone();
        queue_btn.connect_clicked(move |_| {
            Self::show_queue_window(task_queue_clone.clone());
        });
        header_bar.pack_end(&queue_btn);

        // Refresh button
        let refresh_box = Box::new(Orientation::Horizontal, 6);
        let refresh_icon = Image::from_icon_name("view-refresh-symbolic");
        refresh_box.append(&refresh_icon);
        let refresh_label = Label::new(Some("Refresh"));
        refresh_box.append(&refresh_label);
        
        let refresh_btn = Button::new();
        refresh_btn.set_child(Some(&refresh_box));
        refresh_btn.add_css_class("suggested-action");
        refresh_btn.set_tooltip_text(Some("Refresh all package lists"));
        header_bar.pack_end(&refresh_btn);

        main_box.append(&header_bar);

        // Check if paru is installed
        if !ParuBackend::is_paru_installed() {
            let status_page = StatusPage::builder()
                .icon_name("dialog-error-symbolic")
                .title("Paru Not Found")
                .description("The paru AUR helper is not installed on your system.\n\nPlease install paru first:\n\ngit clone https://aur.archlinux.org/paru.git\ncd paru && makepkg -si")
                .build();
            status_page.add_css_class("error-page");
            main_box.append(&status_page);
            
            return Self {
                main_box,
                search_results: Rc::new(RefCell::new(Vec::new())),
                installed_packages: Rc::new(RefCell::new(Vec::new())),
                updates: Rc::new(RefCell::new(Vec::new())),
                task_queue,
                last_refresh_label: last_refresh_label_rc,
            };
        }

        // Create main content with ViewStack for better navigation
        let content_box = Box::new(Orientation::Vertical, 0);
        
        // Create ViewStack for navigation
        let view_stack = ViewStack::new();
        view_stack.set_vexpand(true);
        view_stack.set_hexpand(true);

        // Create ViewSwitcher for modern tab navigation
        let view_switcher = ViewSwitcher::new();
        view_switcher.set_stack(Some(&view_stack));
        view_switcher.set_policy(adw::ViewSwitcherPolicy::Wide);
        
        // Wrap view switcher in a nice container
        let switcher_box = Box::new(Orientation::Horizontal, 0);
        switcher_box.set_halign(gtk4::Align::Center);
        switcher_box.set_margin_top(8);
        switcher_box.set_margin_bottom(8);
        switcher_box.append(&view_switcher);
        
        content_box.append(&switcher_box);
        content_box.append(&Separator::new(Orientation::Horizontal));

        // Dashboard/Overview view
        let dashboard_view = Self::create_dashboard_view(task_queue.clone());
        let dashboard_page = view_stack.add_titled(&dashboard_view.0, Some("dashboard"), "Overview");
        dashboard_page.set_icon_name(Some("go-home-symbolic"));

        // Search view
        let search_view = Self::create_search_view(task_queue.clone());
        let search_page = view_stack.add_titled(&search_view.0, Some("search"), "Search");
        search_page.set_icon_name(Some("system-search-symbolic"));

        // Installed packages view
        let installed_view = Self::create_installed_view(task_queue.clone());
        let installed_page = view_stack.add_titled(&installed_view.0, Some("installed"), "Installed");
        installed_page.set_icon_name(Some("drive-harddisk-symbolic"));

        // Updates view  
        let updates_view = Self::create_updates_view(task_queue.clone());
        let updates_page = view_stack.add_titled(&updates_view.0, Some("updates"), "Updates");
        updates_page.set_icon_name(Some("software-update-available-symbolic"));

        content_box.append(&view_stack);
        main_box.append(&content_box);

        let gui = Self {
            main_box,
            search_results: search_view.1,
            installed_packages: installed_view.1,
            updates: updates_view.1,
            task_queue,
            last_refresh_label: last_refresh_label_rc.clone(),
        };

        // Connect refresh button
        let installed_list = installed_view.2.clone();
        let updates_list = updates_view.2.clone();
        let installed_packages_clone = gui.installed_packages.clone();
        let updates_clone = gui.updates.clone();
        let task_queue_for_refresh = gui.task_queue.clone();
        let refresh_label_clone = last_refresh_label_rc.clone();
        let installed_search = installed_view.3.clone();
        
        // Clone dashboard labels for use in closure and after
        let dash_label_0 = dashboard_view.1.0.clone();
        let dash_label_1 = dashboard_view.1.1.clone();
        let dash_label_2 = dashboard_view.1.2.clone();
        let dash_label_0_init = dashboard_view.1.0.clone();
        let dash_label_1_init = dashboard_view.1.1.clone();
        let dash_label_2_init = dashboard_view.1.2.clone();
        
        refresh_btn.connect_clicked(move |btn| {
            log_info("Refreshing package lists");
            
            // Visual feedback - disable button and show loading
            btn.set_sensitive(false);
            let btn_clone = btn.clone();
            
            Self::refresh_installed(&installed_list, &installed_packages_clone, task_queue_for_refresh.clone());
            Self::refresh_updates(&updates_list, &updates_clone, task_queue_for_refresh.clone());
            Self::update_refresh_time(&refresh_label_clone);
            
            // Update dashboard stats
            Self::refresh_dashboard_stats(&dash_label_0, &dash_label_1, &dash_label_2);
            
            // Clear search filter after refresh
            installed_search.borrow().set_text("");
            
            // Re-enable button after a short delay
            glib::timeout_add_local_once(Duration::from_millis(1500), move || {
                btn_clone.set_sensitive(true);
            });
        });

        // Initial load
        Self::refresh_installed(&installed_view.2, &gui.installed_packages, gui.task_queue.clone());
        Self::refresh_updates(&updates_view.2, &gui.updates, gui.task_queue.clone());
        Self::refresh_dashboard_stats(&dash_label_0_init, &dash_label_1_init, &dash_label_2_init);
        Self::update_refresh_time(&gui.last_refresh_label);

        gui
    }

    pub fn main_widget(&self) -> &Box {
        &self.main_box
    }

    fn create_dashboard_view(task_queue: Arc<TaskQueue>) -> (ScrolledWindow, (Rc<RefCell<Label>>, Rc<RefCell<Label>>, Rc<RefCell<Label>>)) {
        let vbox = Box::new(Orientation::Vertical, 24);
        vbox.set_margin_start(24);
        vbox.set_margin_end(24);
        vbox.set_margin_top(20);
        vbox.set_margin_bottom(20);

        // Welcome header
        let header_box = Box::new(Orientation::Vertical, 8);
        header_box.set_halign(gtk4::Align::Start);
        
        let welcome = Label::new(Some("Welcome to Parut"));
        welcome.add_css_class("title-1");
        welcome.set_halign(gtk4::Align::Start);
        header_box.append(&welcome);
        
        let subtitle = Label::new(Some("A beautiful graphical frontend for the Paru AUR helper"));
        subtitle.add_css_class("dim-label");
        subtitle.set_halign(gtk4::Align::Start);
        header_box.append(&subtitle);
        
        vbox.append(&header_box);

        // Stats cards row
        let stats_box = Box::new(Orientation::Horizontal, 16);
        stats_box.set_homogeneous(true);
        stats_box.set_margin_top(12);

        // Installed packages stat
        let installed_stat = Self::create_stat_card("0", "Installed Packages", "drive-harddisk-symbolic");
        let installed_count_label = installed_stat.1;
        stats_box.append(&installed_stat.0);

        // Updates available stat
        let updates_stat = Self::create_stat_card("0", "Updates Available", "software-update-available-symbolic");
        let updates_count_label = updates_stat.1;
        stats_box.append(&updates_stat.0);

        // AUR packages stat
        let aur_stat = Self::create_stat_card("0", "AUR Packages", "emblem-important-symbolic");
        let aur_count_label = aur_stat.1;
        stats_box.append(&aur_stat.0);

        vbox.append(&stats_box);

        // Quick actions section
        let actions_label = Label::new(Some("Quick Actions"));
        actions_label.add_css_class("title-2");
        actions_label.set_halign(gtk4::Align::Start);
        actions_label.set_margin_top(16);
        vbox.append(&actions_label);

        let actions_box = Box::new(Orientation::Horizontal, 12);
        actions_box.set_margin_top(8);

        // Update System button
        let update_btn = Self::create_action_button(
            "Update System",
            "software-update-available-symbolic",
            "Update all packages to their latest versions"
        );
        let tq = task_queue.clone();
        update_btn.connect_clicked(move |_| {
            log_info("Starting system update from dashboard");
            tq.add_task(TaskType::Update, "system".to_string());
        });
        actions_box.append(&update_btn);

        // Orphans cleanup button
        let clean_btn = Self::create_action_button(
            "Clean Cache",
            "user-trash-symbolic",
            "Clean package cache to free disk space"
        );
        clean_btn.set_sensitive(false); // TODO: Implement
        clean_btn.set_tooltip_text(Some("Coming soon: Clean package cache"));
        actions_box.append(&clean_btn);

        vbox.append(&actions_box);

        // Recent activity section (placeholder)
        let activity_label = Label::new(Some("About Parut"));
        activity_label.add_css_class("title-2");
        activity_label.set_halign(gtk4::Align::Start);
        activity_label.set_margin_top(24);
        vbox.append(&activity_label);

        let about_box = Box::new(Orientation::Vertical, 12);
        about_box.add_css_class("card");
        about_box.set_margin_top(8);
        
        let about_text = Label::new(Some(
            "Parut is a modern GTK4/libadwaita frontend for the Paru AUR helper. \
             It provides a beautiful and intuitive interface for managing both official \
             Arch Linux packages and AUR packages.\n\n\
             Features:\n\
             • Search and install packages from repos and AUR\n\
             • Review PKGBUILDs before installing AUR packages\n\
             • View and manage installed packages\n\
             • Check for and apply system updates\n\
             • Task queue for managing package operations"
        ));
        about_text.set_wrap(true);
        about_text.set_xalign(0.0);
        about_text.set_margin_start(16);
        about_text.set_margin_end(16);
        about_text.set_margin_top(16);
        about_text.set_margin_bottom(16);
        about_box.append(&about_text);

        vbox.append(&about_box);

        let scroll = ScrolledWindow::new();
        scroll.set_child(Some(&vbox));
        scroll.set_vexpand(true);

        (scroll, (installed_count_label, updates_count_label, aur_count_label))
    }

    fn create_stat_card(value: &str, label: &str, icon_name: &str) -> (Box, Rc<RefCell<Label>>) {
        let card = Box::new(Orientation::Vertical, 8);
        card.add_css_class("card");
        card.set_halign(gtk4::Align::Fill);
        card.set_valign(gtk4::Align::Center);

        let icon = Image::from_icon_name(icon_name);
        icon.set_pixel_size(32);
        icon.add_css_class("dim-label");
        icon.set_margin_top(8);
        card.append(&icon);

        let value_label = Label::new(Some(value));
        value_label.add_css_class("title-1");
        card.append(&value_label);

        let label_widget = Label::new(Some(label));
        label_widget.add_css_class("caption");
        label_widget.set_margin_bottom(8);
        card.append(&label_widget);

        let value_label_rc = Rc::new(RefCell::new(value_label));
        (card, value_label_rc)
    }

    fn create_action_button(label: &str, icon_name: &str, tooltip: &str) -> Button {
        let content_box = Box::new(Orientation::Horizontal, 8);
        
        let icon = Image::from_icon_name(icon_name);
        icon.set_pixel_size(18);
        content_box.append(&icon);

        let label_widget = Label::new(Some(label));
        content_box.append(&label_widget);

        let btn = Button::new();
        btn.set_child(Some(&content_box));
        btn.add_css_class("suggested-action");
        btn.set_tooltip_text(Some(tooltip));
        
        btn
    }

    fn refresh_dashboard_stats(
        installed_label: &Rc<RefCell<Label>>,
        updates_label: &Rc<RefCell<Label>>,
        aur_label: &Rc<RefCell<Label>>
    ) {
        let installed_label = installed_label.clone();
        let updates_label = updates_label.clone();
        let aur_label = aur_label.clone();

        glib::spawn_future_local(async move {
            // Get installed count
            if let Ok(pkgs) = ParuBackend::list_installed() {
                let total = pkgs.len();
                let aur_count = pkgs.iter().filter(|p| p.repository == "aur").count();
                installed_label.borrow().set_text(&format!("{}", total));
                aur_label.borrow().set_text(&format!("{}", aur_count));
            }

            // Get updates count
            if let Ok(updates) = ParuBackend::list_updates() {
                updates_label.borrow().set_text(&format!("{}", updates.len()));
            }
        });
    }

    /// Show PKGBUILD review dialog for AUR packages
    fn show_pkgbuild_dialog(_window: &impl IsA<gtk4::Widget>, package_name: &str, task_queue: Arc<TaskQueue>) {
        // Create dialog window
        let dialog = Window::builder()
            .title(format!("Review PKGBUILD - {}", package_name))
            .default_width(900)
            .default_height(700)
            .modal(true)
            .build();

        let main_box = Box::new(Orientation::Vertical, 16);
        main_box.set_margin_start(20);
        main_box.set_margin_end(20);
        main_box.set_margin_top(20);
        main_box.set_margin_bottom(20);

        // Warning header with icon
        let warning_box = Box::new(Orientation::Horizontal, 12);
        warning_box.add_css_class("warning");
        warning_box.set_margin_bottom(8);
        
        let warning_icon = Image::from_icon_name("dialog-warning-symbolic");
        warning_icon.set_pixel_size(32);
        warning_icon.set_margin_start(12);
        warning_box.append(&warning_icon);
        
        let warning_text_box = Box::new(Orientation::Vertical, 4);
        
        let warning_title = Label::new(Some("Security Warning"));
        warning_title.add_css_class("heading");
        warning_title.set_halign(gtk4::Align::Start);
        warning_text_box.append(&warning_title);
        
        let warning_desc = Label::new(Some(
            "AUR packages are user-produced content. Review this PKGBUILD carefully before proceeding."
        ));
        warning_desc.set_wrap(true);
        warning_desc.set_xalign(0.0);
        warning_desc.add_css_class("dim-label");
        warning_text_box.append(&warning_desc);
        
        warning_box.append(&warning_text_box);
        main_box.append(&warning_box);

        // Package info
        let info_box = Box::new(Orientation::Horizontal, 8);
        let pkg_icon = Image::from_icon_name("package-x-generic-symbolic");
        pkg_icon.set_pixel_size(20);
        info_box.append(&pkg_icon);
        
        let info_label = Label::new(Some(&format!("Package: {} (from AUR)", package_name)));
        info_label.add_css_class("heading");
        info_box.append(&info_label);
        main_box.append(&info_box);

        // Loading spinner
        let loading_box = Box::new(Orientation::Horizontal, 8);
        loading_box.set_halign(gtk4::Align::Center);
        let spinner = Spinner::new();
        spinner.set_spinning(true);
        loading_box.append(&spinner);
        let loading_label = Label::new(Some("Loading PKGBUILD..."));
        loading_label.add_css_class("dim-label");
        loading_box.append(&loading_label);
        
        let loading_box_rc = Rc::new(RefCell::new(loading_box.clone()));
        main_box.append(&loading_box);

        // Text view for PKGBUILD
        let scrolled = ScrolledWindow::new();
        scrolled.set_vexpand(true);
        scrolled.set_hexpand(true);
        scrolled.add_css_class("card");

        let text_view = TextView::new();
        text_view.set_editable(false);
        text_view.set_monospace(true);
        text_view.set_margin_start(12);
        text_view.set_margin_end(12);
        text_view.set_margin_top(12);
        text_view.set_margin_bottom(12);
        text_view.set_wrap_mode(gtk4::WrapMode::Word);
        
        scrolled.set_child(Some(&text_view));
        main_box.append(&scrolled);

        // Button box
        let button_box = Box::new(Orientation::Horizontal, 12);
        button_box.set_halign(gtk4::Align::End);
        button_box.set_margin_top(8);
        
        let cancel_btn = Button::with_label("Cancel");
        cancel_btn.add_css_class("flat");
        
        let install_box = Box::new(Orientation::Horizontal, 8);
        let install_icon = Image::from_icon_name("emblem-ok-symbolic");
        install_box.append(&install_icon);
        let install_label = Label::new(Some("I've Reviewed - Install"));
        install_box.append(&install_label);
        
        let install_btn = Button::new();
        install_btn.set_child(Some(&install_box));
        install_btn.add_css_class("suggested-action");
        install_btn.set_sensitive(false); // Enable after content loads
        
        button_box.append(&cancel_btn);
        button_box.append(&install_btn);
        main_box.append(&button_box);

        dialog.set_child(Some(&main_box));

        // Fetch PKGBUILD content
        let text_buffer = text_view.buffer();
        let package_name_clone = package_name.to_string();
        let install_btn_clone = install_btn.clone();
        let loading_box_clone = loading_box_rc.clone();
        
        glib::spawn_future_local(async move {
            match ParuBackend::get_pkgbuild(&package_name_clone) {
                Ok(content) => {
                    text_buffer.set_text(&content);
                    install_btn_clone.set_sensitive(true);
                    loading_box_clone.borrow().set_visible(false);
                }
                Err(e) => {
                    text_buffer.set_text(&format!("Error loading PKGBUILD:\n\n{}", e));
                    loading_box_clone.borrow().set_visible(false);
                }
            }
        });

        // Connect buttons
        let dialog_weak = dialog.downgrade();
        cancel_btn.connect_clicked(move |_| {
            if let Some(dialog) = dialog_weak.upgrade() {
                dialog.close();
            }
        });

        let dialog_weak2 = dialog.downgrade();
        let pkg_clone = package_name.to_string();
        install_btn.connect_clicked(move |_| {
            log_info(&format!("Adding install task for package: {}", pkg_clone));
            task_queue.add_task(TaskType::Install, pkg_clone.clone());
            if let Some(dialog) = dialog_weak2.upgrade() {
                dialog.close();
            }
        });

        dialog.present();
    }

    fn show_queue_window(task_queue: Arc<TaskQueue>) {
        let window = Window::builder()
            .title("Task Queue")
            .default_width(700)
            .default_height(500)
            .build();

        let vbox = Box::new(Orientation::Vertical, 16);
        vbox.set_margin_start(20);
        vbox.set_margin_end(20);
        vbox.set_margin_top(20);
        vbox.set_margin_bottom(20);

        // Header with icon
        let header_box = Box::new(Orientation::Horizontal, 12);
        
        let header_icon = Image::from_icon_name("view-list-symbolic");
        header_icon.set_pixel_size(28);
        header_box.append(&header_icon);
        
        let title = Label::new(Some("Task Queue"));
        title.add_css_class("title-2");
        title.set_halign(gtk4::Align::Start);
        title.set_hexpand(true);
        header_box.append(&title);

        let clear_box = Box::new(Orientation::Horizontal, 6);
        let clear_icon = Image::from_icon_name("user-trash-symbolic");
        clear_box.append(&clear_icon);
        let clear_label = Label::new(Some("Clear Completed"));
        clear_box.append(&clear_label);
        
        let clear_btn = Button::new();
        clear_btn.set_child(Some(&clear_box));
        clear_btn.add_css_class("flat");
        clear_btn.set_tooltip_text(Some("Remove completed and failed tasks from the queue"));
        
        let task_queue_clone = task_queue.clone();
        clear_btn.connect_clicked(move |_| {
            task_queue_clone.clear_completed();
        });
        header_box.append(&clear_btn);
        
        vbox.append(&header_box);

        // Task list
        let scrolled = ScrolledWindow::new();
        scrolled.set_vexpand(true);

        let list_box = ListBox::new();
        list_box.add_css_class("boxed-list");
        scrolled.set_child(Some(&list_box));
        vbox.append(&scrolled);

        window.set_child(Some(&vbox));

        // Update task list
        let list_box_rc = Rc::new(RefCell::new(list_box));
        let list_box_clone = list_box_rc.clone();
        
        let update_list = move || {
            let tasks = task_queue.get_tasks();
            let list_box = list_box_clone.borrow();
            
            // Clear existing
            while let Some(child) = list_box.first_child() {
                list_box.remove(&child);
            }
            
            if tasks.is_empty() {
                let empty_box = Box::new(Orientation::Vertical, 12);
                empty_box.set_margin_top(48);
                empty_box.set_margin_bottom(48);
                empty_box.set_halign(gtk4::Align::Center);
                
                let empty_icon = Image::from_icon_name("view-list-symbolic");
                empty_icon.set_pixel_size(64);
                empty_icon.add_css_class("dim-label");
                empty_box.append(&empty_icon);
                
                let empty = Label::new(Some("No tasks in queue"));
                empty.add_css_class("dim-label");
                empty_box.append(&empty);
                
                list_box.append(&empty_box);
            } else {
                for task in &tasks {
                    let row = Self::create_task_row(task);
                    list_box.append(&row);
                }
            }
        };

        // Initial update
        update_list();

        // Update periodically
        glib::timeout_add_seconds_local(1, move || {
            update_list();
            glib::ControlFlow::Continue
        });

        window.present();
    }

    fn create_task_row(task: &crate::task_queue::Task) -> Box {
        let row_box = Box::new(Orientation::Vertical, 8);
        row_box.set_margin_start(16);
        row_box.set_margin_end(16);
        row_box.set_margin_top(12);
        row_box.set_margin_bottom(12);

        // Header with task type icon, name, and status
        let header_box = Box::new(Orientation::Horizontal, 12);
        
        // Task type icon
        let icon_name = match task.task_type {
            TaskType::Install => "list-add-symbolic",
            TaskType::Remove => "list-remove-symbolic",
            TaskType::Update => "software-update-available-symbolic",
        };
        let task_icon = Image::from_icon_name(icon_name);
        task_icon.set_pixel_size(20);
        header_box.append(&task_icon);
        
        let task_type_str = match task.task_type {
            TaskType::Install => "Install",
            TaskType::Remove => "Remove",
            TaskType::Update => "Update",
        };
        
        let title_label = Label::new(Some(&format!("{}: {}", task_type_str, task.package_name)));
        title_label.add_css_class("heading");
        title_label.set_halign(gtk4::Align::Start);
        title_label.set_hexpand(true);
        header_box.append(&title_label);

        // Status indicator
        let status_box = Box::new(Orientation::Horizontal, 6);
        
        match &task.status {
            TaskStatus::Queued => {
                let icon = Image::from_icon_name("content-loading-symbolic");
                icon.set_pixel_size(16);
                status_box.append(&icon);
                let label = Label::new(Some("Queued"));
                label.add_css_class("dim-label");
                status_box.append(&label);
            }
            TaskStatus::Running => {
                let spinner = Spinner::new();
                spinner.set_spinning(true);
                status_box.append(&spinner);
                let label = Label::new(Some("Running..."));
                label.add_css_class("accent");
                status_box.append(&label);
            }
            TaskStatus::Completed => {
                let icon = Image::from_icon_name("emblem-ok-symbolic");
                icon.set_pixel_size(16);
                icon.add_css_class("success");
                status_box.append(&icon);
                let label = Label::new(Some("Completed"));
                label.add_css_class("success");
                status_box.append(&label);
            }
            TaskStatus::Failed(_) => {
                let icon = Image::from_icon_name("dialog-error-symbolic");
                icon.set_pixel_size(16);
                icon.add_css_class("error");
                status_box.append(&icon);
                let label = Label::new(Some("Failed"));
                label.add_css_class("error");
                status_box.append(&label);
            }
        }
        
        header_box.append(&status_box);
        row_box.append(&header_box);

        // Progress bar if task is running
        if task.status == TaskStatus::Running {
            if let Some(progress) = task.progress {
                let progress_bar = ProgressBar::new();
                progress_bar.set_fraction(progress);
                progress_bar.set_show_text(true);
                progress_bar.set_text(Some(&format!("{}%", (progress * 100.0) as i32)));
                row_box.append(&progress_bar);
            } else {
                let progress_bar = ProgressBar::new();
                progress_bar.pulse();
                row_box.append(&progress_bar);
            }
        }

        // Show error message if failed
        if let TaskStatus::Failed(error) = &task.status {
            let error_box = Box::new(Orientation::Horizontal, 8);
            error_box.add_css_class("card");
            error_box.set_margin_top(4);
            
            let error_icon = Image::from_icon_name("dialog-error-symbolic");
            error_icon.add_css_class("error");
            error_icon.set_margin_start(8);
            error_box.append(&error_icon);
            
            let error_label = Label::new(Some(&format!("Error: {}", error)));
            error_label.add_css_class("error");
            error_label.set_halign(gtk4::Align::Start);
            error_label.set_wrap(true);
            error_label.set_margin_end(8);
            error_box.append(&error_label);
            
            row_box.append(&error_box);
        }

        // Show last few output lines if available
        if !task.output.is_empty() && matches!(task.status, TaskStatus::Running | TaskStatus::Failed(_)) {
            let output_box = Box::new(Orientation::Vertical, 2);
            output_box.add_css_class("card");
            output_box.set_margin_top(4);
            
            // Show last 3 lines
            let last_lines: Vec<_> = task.output.iter().rev().take(3).rev().collect();
            for line in last_lines {
                let output_label = Label::new(Some(line));
                output_label.add_css_class("monospace");
                output_label.add_css_class("caption");
                output_label.set_halign(gtk4::Align::Start);
                output_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
                output_label.set_max_width_chars(80);
                output_label.set_margin_start(8);
                output_label.set_margin_end(8);
                output_box.append(&output_label);
            }
            
            row_box.append(&output_box);
        }

        row_box
    }

    fn create_search_view(task_queue: Arc<TaskQueue>) -> (Box, Rc<RefCell<Vec<Package>>>, Rc<RefCell<ListBox>>) {
        let vbox = Box::new(Orientation::Vertical, 16);
        vbox.set_margin_start(20);
        vbox.set_margin_end(20);
        vbox.set_margin_top(16);
        vbox.set_margin_bottom(16);

        // Search header
        let header_box = Box::new(Orientation::Horizontal, 12);
        let search_icon = Image::from_icon_name("system-search-symbolic");
        search_icon.set_pixel_size(24);
        header_box.append(&search_icon);
        
        let title = Label::new(Some("Search Packages"));
        title.add_css_class("title-2");
        title.set_halign(gtk4::Align::Start);
        header_box.append(&title);
        vbox.append(&header_box);

        // Search entry with hint
        let search_box = Box::new(Orientation::Vertical, 6);
        
        let search_entry = SearchEntry::new();
        search_entry.set_placeholder_text(Some("Search for packages in repos and AUR..."));
        search_entry.set_hexpand(true);
        search_box.append(&search_entry);
        
        let hint_label = Label::new(Some("Start typing to search. Results limited to 100 packages."));
        hint_label.add_css_class("caption");
        hint_label.add_css_class("dim-label");
        hint_label.set_halign(gtk4::Align::Start);
        search_box.append(&hint_label);
        
        vbox.append(&search_box);

        // Loading indicator (hidden by default)
        let loading_box = Box::new(Orientation::Horizontal, 8);
        loading_box.set_halign(gtk4::Align::Center);
        loading_box.set_visible(false);
        let spinner = Spinner::new();
        spinner.set_spinning(true);
        loading_box.append(&spinner);
        let loading_label = Label::new(Some("Searching..."));
        loading_label.add_css_class("dim-label");
        loading_box.append(&loading_label);
        let loading_box_rc = Rc::new(RefCell::new(loading_box.clone()));
        vbox.append(&loading_box);

        // Results list
        let scrolled = ScrolledWindow::new();
        scrolled.set_vexpand(true);

        let list_box = ListBox::new();
        list_box.add_css_class("boxed-list");
        scrolled.set_child(Some(&list_box));
        vbox.append(&scrolled);

        let packages = Rc::new(RefCell::new(Vec::new()));
        let list_box_rc = Rc::new(RefCell::new(list_box));

        // Debounced search handler
        let list_box_clone = list_box_rc.clone();
        let packages_clone = packages.clone();
        let task_queue_clone = task_queue.clone();
        let loading_clone = loading_box_rc.clone();
        let hint_label_rc = Rc::new(RefCell::new(hint_label));
        let hint_clone = hint_label_rc.clone();
        
        // Store the timeout ID
        let timeout_id: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
        let timeout_id_clone = timeout_id.clone();
        
        search_entry.connect_search_changed(move |entry| {
            let query = entry.text().to_string();
            
            // Cancel previous timeout if any
            if let Some(id) = timeout_id_clone.borrow_mut().take() {
                id.remove();
            }
            
            if query.is_empty() {
                let list_box = list_box_clone.borrow();
                while let Some(child) = list_box.first_child() {
                    list_box.remove(&child);
                }
                hint_clone.borrow().set_visible(true);
                hint_clone.borrow().set_text("Start typing to search. Results limited to 100 packages.");
                loading_clone.borrow().set_visible(false);
                return;
            }

            if query.len() < 2 {
                hint_clone.borrow().set_text("Type at least 2 characters to search...");
                return;
            }

            // Show loading
            loading_clone.borrow().set_visible(true);
            hint_clone.borrow().set_visible(false);

            let list_box = list_box_clone.clone();
            let packages = packages_clone.clone();
            let task_queue = task_queue_clone.clone();
            let loading = loading_clone.clone();
            let hint = hint_clone.clone();
            
            let timeout_id_inner = timeout_id_clone.clone();
            
            // Debounce: wait 300ms before searching
            let id = glib::timeout_add_local_once(Duration::from_millis(300), move || {
                // Clear the ID as it is executing so we don't try to remove it later
                let _ = timeout_id_inner.borrow_mut().take();
                
                glib::spawn_future_local(async move {
                    match ParuBackend::search_packages(&query, Some(100)) {
                        Ok(results) => {
                            loading.borrow().set_visible(false);
                            if results.is_empty() {
                                hint.borrow().set_visible(true);
                                hint.borrow().set_text("No packages found matching your query.");
                            } else {
                                hint.borrow().set_visible(true);
                                hint.borrow().set_text(&format!("Found {} packages", results.len()));
                            }
                            *packages.borrow_mut() = results.clone();
                            Self::update_package_list(&list_box.borrow(), &results, true, task_queue);
                        }
                        Err(e) => {
                            loading.borrow().set_visible(false);
                            hint.borrow().set_visible(true);
                            hint.borrow().set_text(&format!("Search error: {}", e));
                            log_error(&format!("Search failed: {}", e));
                        }
                    }
                });
            });
            
            *timeout_id.borrow_mut() = Some(id);
        });

        (vbox, packages, list_box_rc)
    }

    fn create_installed_view(task_queue: Arc<TaskQueue>) -> (Box, Rc<RefCell<Vec<Package>>>, Rc<RefCell<ListBox>>, Rc<RefCell<SearchEntry>>) {
        let vbox = Box::new(Orientation::Vertical, 16);
        vbox.set_margin_start(20);
        vbox.set_margin_end(20);
        vbox.set_margin_top(16);
        vbox.set_margin_bottom(16);

        // Header with icon
        let header_box = Box::new(Orientation::Horizontal, 12);
        let header_icon = Image::from_icon_name("drive-harddisk-symbolic");
        header_icon.set_pixel_size(24);
        header_box.append(&header_icon);
        
        let title = Label::new(Some("Installed Packages"));
        title.add_css_class("title-2");
        title.set_halign(gtk4::Align::Start);
        header_box.append(&title);
        
        // Package count badge
        let count_label = Label::new(Some("0 packages"));
        count_label.add_css_class("badge");
        count_label.set_halign(gtk4::Align::End);
        count_label.set_hexpand(true);
        header_box.append(&count_label);
        let count_label_rc = Rc::new(RefCell::new(count_label));
        
        vbox.append(&header_box);

        // Search entry for filtering
        let search_entry = SearchEntry::new();
        search_entry.set_placeholder_text(Some("Filter installed packages by name or description..."));
        vbox.append(&search_entry);

        let scrolled = ScrolledWindow::new();
        scrolled.set_vexpand(true);

        let list_box = ListBox::new();
        list_box.add_css_class("boxed-list");
        scrolled.set_child(Some(&list_box));
        vbox.append(&scrolled);

        let packages = Rc::new(RefCell::new(Vec::new()));
        let list_box_rc = Rc::new(RefCell::new(list_box));
        let search_entry_rc = Rc::new(RefCell::new(search_entry.clone()));

        // Filter handler
        let list_box_clone = list_box_rc.clone();
        let packages_clone = packages.clone();
        let task_queue_clone = task_queue.clone();
        let count_clone = count_label_rc.clone();
        
        search_entry.connect_search_changed(move |entry| {
            let query = entry.text().to_string().to_lowercase();
            let all_packages = packages_clone.borrow();
            
            if query.is_empty() {
                // Show all packages
                Self::update_package_list_with_remove(&list_box_clone.borrow(), &all_packages, task_queue_clone.clone());
                count_clone.borrow().set_text(&format!("{} packages", all_packages.len()));
            } else {
                // Filter packages
                let filtered: Vec<Package> = all_packages
                    .iter()
                    .filter(|pkg| {
                        pkg.name.to_lowercase().contains(&query) ||
                        pkg.description.to_lowercase().contains(&query)
                    })
                    .cloned()
                    .collect();
                
                count_clone.borrow().set_text(&format!("{} / {} packages", filtered.len(), all_packages.len()));
                Self::update_package_list_with_remove(&list_box_clone.borrow(), &filtered, task_queue_clone.clone());
            }
        });

        (vbox, packages, list_box_rc, search_entry_rc)
    }

    fn create_updates_view(task_queue: Arc<TaskQueue>) -> (Box, Rc<RefCell<Vec<Package>>>, Rc<RefCell<ListBox>>) {
        let vbox = Box::new(Orientation::Vertical, 16);
        vbox.set_margin_start(20);
        vbox.set_margin_end(20);
        vbox.set_margin_top(16);
        vbox.set_margin_bottom(16);

        // Header with icon and update all button
        let header_box = Box::new(Orientation::Horizontal, 12);
        
        let header_icon = Image::from_icon_name("software-update-available-symbolic");
        header_icon.set_pixel_size(24);
        header_box.append(&header_icon);
        
        let title = Label::new(Some("Available Updates"));
        title.add_css_class("title-2");
        title.set_halign(gtk4::Align::Start);
        title.set_hexpand(true);
        header_box.append(&title);

        let update_box = Box::new(Orientation::Horizontal, 8);
        let update_icon = Image::from_icon_name("software-update-available-symbolic");
        update_box.append(&update_icon);
        let update_label = Label::new(Some("Update All"));
        update_box.append(&update_label);
        
        let update_all_btn = Button::new();
        update_all_btn.set_child(Some(&update_box));
        update_all_btn.add_css_class("suggested-action");
        update_all_btn.set_tooltip_text(Some("Update all packages to their latest versions"));
        
        let task_queue_clone = task_queue.clone();
        update_all_btn.connect_clicked(move |_| {
            log_info("Starting system update");
            task_queue_clone.add_task(TaskType::Update, "system".to_string());
        });
        
        header_box.append(&update_all_btn);
        vbox.append(&header_box);

        // Info box
        let info_box = Box::new(Orientation::Horizontal, 8);
        info_box.add_css_class("card");
        info_box.set_margin_top(4);
        
        let info_icon = Image::from_icon_name("dialog-information-symbolic");
        info_icon.set_margin_start(12);
        info_icon.set_margin_top(8);
        info_icon.set_margin_bottom(8);
        info_box.append(&info_icon);
        
        let info_label = Label::new(Some("Click 'Refresh' in the header to check for new updates."));
        info_label.add_css_class("dim-label");
        info_label.set_margin_end(12);
        info_box.append(&info_label);
        
        vbox.append(&info_box);

        let scrolled = ScrolledWindow::new();
        scrolled.set_vexpand(true);

        let list_box = ListBox::new();
        list_box.add_css_class("boxed-list");
        scrolled.set_child(Some(&list_box));
        vbox.append(&scrolled);

        let packages = Rc::new(RefCell::new(Vec::new()));
        let list_box_rc = Rc::new(RefCell::new(list_box));

        (vbox, packages, list_box_rc)
    }

    fn update_package_list(list_box: &ListBox, packages: &[Package], show_actions: bool, task_queue: Arc<TaskQueue>) {
        // Clear existing items
        while let Some(child) = list_box.first_child() {
            list_box.remove(&child);
        }

        if packages.is_empty() {
            let empty_box = Box::new(Orientation::Vertical, 12);
            empty_box.set_margin_top(48);
            empty_box.set_margin_bottom(48);
            empty_box.set_halign(gtk4::Align::Center);
            
            let empty_icon = Image::from_icon_name("package-x-generic-symbolic");
            empty_icon.set_pixel_size(64);
            empty_icon.add_css_class("dim-label");
            empty_box.append(&empty_icon);
            
            let empty_label = Label::new(Some("No packages found"));
            empty_label.add_css_class("dim-label");
            empty_box.append(&empty_label);
            
            list_box.append(&empty_box);
            return;
        }

        for package in packages {
            let row = Self::create_package_row(package, show_actions, task_queue.clone());
            list_box.append(&row);
        }
    }

    fn create_package_row(package: &Package, show_actions: bool, task_queue: Arc<TaskQueue>) -> Box {
        let row_box = Box::new(Orientation::Horizontal, 12);
        row_box.add_css_class("package-row");
        row_box.set_margin_start(16);
        row_box.set_margin_end(16);
        row_box.set_margin_top(12);
        row_box.set_margin_bottom(12);

        // Package icon
        let pkg_icon = Image::from_icon_name("package-x-generic-symbolic");
        pkg_icon.set_pixel_size(32);
        pkg_icon.add_css_class("dim-label");
        row_box.append(&pkg_icon);

        let info_box = Box::new(Orientation::Vertical, 4);
        info_box.set_hexpand(true);

        // Package name and repo tag
        let name_box = Box::new(Orientation::Horizontal, 8);
        
        let name_label = Label::new(Some(&package.name));
        name_label.add_css_class("heading");
        name_label.set_halign(gtk4::Align::Start);
        name_box.append(&name_label);

        // Repository badge with color coding
        let repo_label = Label::new(Some(&package.repository));
        repo_label.add_css_class("repo-tag");
        
        match package.repository.as_str() {
            "aur" => repo_label.add_css_class("repo-tag-aur"),
            "core" | "core-testing" => repo_label.add_css_class("repo-tag-core"),
            "extra" | "extra-testing" => repo_label.add_css_class("repo-tag-extra"),
            "community" | "multilib" => repo_label.add_css_class("repo-tag-community"),
            _ => {}
        }
        
        name_box.append(&repo_label);
        info_box.append(&name_box);

        // Version info with styling
        let version_box = Box::new(Orientation::Horizontal, 8);
        
        let _version_text = if let Some(installed) = &package.installed_version {
            let ver_label = Label::new(Some(installed));
            ver_label.add_css_class("version-badge");
            version_box.append(&ver_label);
            
            let arrow = Label::new(Some("→"));
            arrow.add_css_class("version-update");
            version_box.append(&arrow);
            
            let new_ver = Label::new(Some(&package.version));
            new_ver.add_css_class("version-badge");
            new_ver.add_css_class("version-update");
            version_box.append(&new_ver);
            
            format!("{} → {}", installed, package.version)
        } else {
            let ver_label = Label::new(Some(&package.version));
            ver_label.add_css_class("version-badge");
            version_box.append(&ver_label);
            package.version.clone()
        };
        
        info_box.append(&version_box);

        // Description
        if !package.description.is_empty() {
            let desc_label = Label::new(Some(&package.description));
            desc_label.add_css_class("caption");
            desc_label.add_css_class("dim-label");
            desc_label.set_halign(gtk4::Align::Start);
            desc_label.set_wrap(true);
            desc_label.set_max_width_chars(60);
            desc_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
            desc_label.set_lines(2);
            info_box.append(&desc_label);
        }

        row_box.append(&info_box);

        if show_actions {
            let action_box = Box::new(Orientation::Horizontal, 8);
            
            let install_icon = Image::from_icon_name("list-add-symbolic");
            let install_btn = Button::new();
            install_btn.set_child(Some(&install_icon));
            install_btn.add_css_class("suggested-action");
            install_btn.add_css_class("circular");
            install_btn.set_tooltip_text(Some("Install this package"));
            
            let pkg_name = package.name.clone();
            let pkg_repo = package.repository.clone();
            let row_box_weak = row_box.downgrade();
            
            install_btn.connect_clicked(move |_btn| {
                // Check if it's an AUR package
                let is_aur = pkg_repo == "aur" || ParuBackend::is_aur_package(&pkg_name);
                
                if is_aur {
                    // Show PKGBUILD review dialog for AUR packages
                    if let Some(row_box) = row_box_weak.upgrade() {
                        Self::show_pkgbuild_dialog(&row_box, &pkg_name, task_queue.clone());
                    }
                } else {
                    // Directly install official repo packages
                    log_info(&format!("Adding install task for package: {}", pkg_name));
                    task_queue.add_task(TaskType::Install, pkg_name.clone());
                }
            });
            
            action_box.append(&install_btn);
            row_box.append(&action_box);
        }

        row_box
    }

    fn update_package_list_with_remove(list_box: &ListBox, packages: &[Package], task_queue: Arc<TaskQueue>) {
        // Clear existing items
        while let Some(child) = list_box.first_child() {
            list_box.remove(&child);
        }

        if packages.is_empty() {
            let empty_box = Box::new(Orientation::Vertical, 12);
            empty_box.set_margin_top(48);
            empty_box.set_margin_bottom(48);
            empty_box.set_halign(gtk4::Align::Center);
            
            let empty_icon = Image::from_icon_name("package-x-generic-symbolic");
            empty_icon.set_pixel_size(64);
            empty_icon.add_css_class("dim-label");
            empty_box.append(&empty_icon);
            
            let empty_label = Label::new(Some("No packages found"));
            empty_label.add_css_class("dim-label");
            empty_box.append(&empty_label);
            
            list_box.append(&empty_box);
            return;
        }

        for package in packages {
            let row = Self::create_package_row_with_remove(package, task_queue.clone());
            list_box.append(&row);
        }
    }

    fn create_package_row_with_remove(package: &Package, task_queue: Arc<TaskQueue>) -> Box {
        let row_box = Box::new(Orientation::Horizontal, 12);
        row_box.add_css_class("package-row");
        row_box.set_margin_start(16);
        row_box.set_margin_end(16);
        row_box.set_margin_top(12);
        row_box.set_margin_bottom(12);

        // Package icon
        let pkg_icon = Image::from_icon_name("package-x-generic-symbolic");
        pkg_icon.set_pixel_size(32);
        pkg_icon.add_css_class("dim-label");
        row_box.append(&pkg_icon);

        let info_box = Box::new(Orientation::Vertical, 4);
        info_box.set_hexpand(true);

        // Package name and repo tag
        let name_box = Box::new(Orientation::Horizontal, 8);
        
        let name_label = Label::new(Some(&package.name));
        name_label.add_css_class("heading");
        name_label.set_halign(gtk4::Align::Start);
        name_box.append(&name_label);

        // Repository badge
        let repo_label = Label::new(Some(&package.repository));
        repo_label.add_css_class("repo-tag");
        
        match package.repository.as_str() {
            "aur" => repo_label.add_css_class("repo-tag-aur"),
            "core" | "core-testing" => repo_label.add_css_class("repo-tag-core"),
            "extra" | "extra-testing" => repo_label.add_css_class("repo-tag-extra"),
            "community" | "multilib" => repo_label.add_css_class("repo-tag-community"),
            _ => {}
        }
        
        name_box.append(&repo_label);
        info_box.append(&name_box);

        // Version
        let version_label = Label::new(Some(&package.version));
        version_label.add_css_class("version-badge");
        version_label.set_halign(gtk4::Align::Start);
        info_box.append(&version_label);

        row_box.append(&info_box);

        // Remove button
        let action_box = Box::new(Orientation::Horizontal, 8);
        
        let remove_icon = Image::from_icon_name("user-trash-symbolic");
        let remove_btn = Button::new();
        remove_btn.set_child(Some(&remove_icon));
        remove_btn.add_css_class("destructive-action");
        remove_btn.add_css_class("circular");
        remove_btn.set_tooltip_text(Some("Remove this package"));
        
        let pkg_name = package.name.clone();
        remove_btn.connect_clicked(move |_btn| {
            log_info(&format!("Adding remove task for package: {}", pkg_name));
            task_queue.add_task(TaskType::Remove, pkg_name.clone());
        });
        
        action_box.append(&remove_btn);
        row_box.append(&action_box);

        row_box
    }

    fn refresh_installed(list_box: &Rc<RefCell<ListBox>>, packages: &Rc<RefCell<Vec<Package>>>, task_queue: Arc<TaskQueue>) {
        let list_box = list_box.clone();
        let packages = packages.clone();
        
        glib::spawn_future_local(async move {
            match ParuBackend::list_installed() {
                Ok(pkgs) => {
                    *packages.borrow_mut() = pkgs.clone();
                    Self::update_package_list_with_remove(&list_box.borrow(), &pkgs, task_queue);
                }
                Err(e) => {
                    log_error(&format!("Error loading installed packages: {}", e));
                }
            }
        });
    }

    fn refresh_updates(list_box: &Rc<RefCell<ListBox>>, packages: &Rc<RefCell<Vec<Package>>>, task_queue: Arc<TaskQueue>) {
        let list_box = list_box.clone();
        let packages = packages.clone();
        
        glib::spawn_future_local(async move {
            match ParuBackend::list_updates() {
                Ok(pkgs) => {
                    *packages.borrow_mut() = pkgs.clone();
                    Self::update_package_list(&list_box.borrow(), &pkgs, false, task_queue);
                }
                Err(e) => {
                    log_error(&format!("Error loading updates: {}", e));
                }
            }
        });
    }

    fn update_refresh_time(label: &Rc<RefCell<Label>>) {
        use std::time::{SystemTime, UNIX_EPOCH};
        
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        let label_clone = label.clone();
        
        // Update immediately
        label.borrow().set_text("Refreshed just now");
        
        // Update every 30 seconds to show elapsed time
        glib::timeout_add_seconds_local(30, move || {
            let elapsed = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs() - now;
            
            let time_str = if elapsed < 60 {
                "Refreshed just now".to_string()
            } else if elapsed < 3600 {
                format!("Refreshed {} min ago", elapsed / 60)
            } else if elapsed < 86400 {
                format!("Refreshed {} hr ago", elapsed / 3600)
            } else {
                format!("Refreshed {} days ago", elapsed / 86400)
            };
            
            label_clone.borrow().set_text(&time_str);
            glib::ControlFlow::Continue
        });
    }
}
