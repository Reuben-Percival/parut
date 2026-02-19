use crate::logger::{log_error, log_info};
use crate::paru::{NewsItem, Package, ParuBackend};
use crate::task_queue::{TaskQueue, TaskStatus, TaskType, TaskWorker};
use adw::prelude::*;
use adw::{
    ActionRow, ComboRow, HeaderBar, PreferencesGroup, PreferencesPage, StatusPage, StyleManager,
    ViewStack, ViewSwitcher,
};
use gtk4::{
    Box, Button, CheckButton, DropDown, Entry, Image, Label, ListBox, Orientation, ProgressBar,
    ScrolledWindow, SearchEntry, Separator, Spinner, StringList, TextView, Window, gio, glib,
};
use std::cell::RefCell;
use std::collections::HashSet;
use std::process::Command;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

pub struct ParuGui {
    main_box: Box,
    #[allow(dead_code)]
    search_results: Rc<RefCell<Vec<Package>>>,
    installed_packages: Rc<RefCell<Vec<Package>>>,
    updates: Rc<RefCell<Vec<Package>>>,
    task_queue: Arc<TaskQueue>,
    last_refresh_label: Rc<RefCell<Label>>,
    refresh_timer_id: Rc<RefCell<Option<glib::SourceId>>>,
    auto_refresh_timer_id: Rc<RefCell<Option<glib::SourceId>>>,
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
        let refresh_timer_id = Rc::new(RefCell::new(None));
        let auto_refresh_timer_id = Rc::new(RefCell::new(None));

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

        // Settings button
        let settings_icon = Image::from_icon_name("emblem-system-symbolic");
        let settings_btn = Button::new();
        settings_btn.set_child(Some(&settings_icon));
        settings_btn.add_css_class("flat");
        settings_btn.set_tooltip_text(Some("Preferences"));

        settings_btn.set_tooltip_text(Some("Preferences"));

        let settings_btn_weak = settings_btn.downgrade();
        settings_btn.connect_clicked(move |_| {
            if let Some(btn) = settings_btn_weak.upgrade() {
                if let Some(window) = btn.root().and_then(|w| w.downcast::<Window>().ok()) {
                    Self::show_settings_dialog(&window);
                }
            }
        });
        header_bar.pack_end(&settings_btn);

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
                refresh_timer_id,
                auto_refresh_timer_id,
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
        let dashboard_page =
            view_stack.add_titled(&dashboard_view.0, Some("dashboard"), "Overview");
        dashboard_page.set_icon_name(Some("go-home-symbolic"));

        // Search view
        let search_view = Self::create_search_view(task_queue.clone());
        let search_page = view_stack.add_titled(&search_view.0, Some("search"), "Search");
        search_page.set_icon_name(Some("system-search-symbolic"));

        // Installed packages view
        let installed_view = Self::create_installed_view(task_queue.clone());
        let installed_page =
            view_stack.add_titled(&installed_view.0, Some("installed"), "Installed");
        installed_page.set_icon_name(Some("drive-harddisk-symbolic"));

        // Updates view
        let updates_view = Self::create_updates_view(task_queue.clone());
        let updates_page = view_stack.add_titled(&updates_view.0, Some("updates"), "Updates");
        updates_page.set_icon_name(Some("software-update-available-symbolic"));

        // Watchlist view
        let watchlist_view = Self::create_watchlist_view(
            task_queue.clone(),
            installed_view.1.clone(),
            updates_view.1.clone(),
        );
        let watchlist_page =
            view_stack.add_titled(&watchlist_view.0, Some("watchlist"), "Watchlist");
        watchlist_page.set_icon_name(Some("starred-symbolic"));
        view_stack.set_visible_child_name(&crate::settings::get().startup_tab);

        content_box.append(&view_stack);
        main_box.append(&content_box);

        let gui = Self {
            main_box,
            search_results: search_view.1,
            installed_packages: installed_view.1,
            updates: updates_view.1,
            task_queue,
            last_refresh_label: last_refresh_label_rc.clone(),
            refresh_timer_id: refresh_timer_id.clone(),
            auto_refresh_timer_id: auto_refresh_timer_id.clone(),
        };

        // Connect refresh button
        let installed_list = installed_view.2.clone();
        let updates_list = updates_view.2.clone();
        let updates_renderer = updates_view.3.clone();
        let installed_renderer = installed_view.4.clone();
        let installed_renderer_for_refresh = installed_renderer.clone();
        let installed_packages_clone = gui.installed_packages.clone();
        let updates_clone = gui.updates.clone();
        let task_queue_for_refresh = gui.task_queue.clone();
        let refresh_label_clone = last_refresh_label_rc.clone();
        let refresh_timer_clone = refresh_timer_id.clone();
        let installed_search = installed_view.3.clone();

        // Clone dashboard labels for use in closure and after
        let dash_label_0 = dashboard_view.1.0.clone();
        let dash_label_1 = dashboard_view.1.1.clone();
        let dash_label_2 = dashboard_view.1.2.clone();
        let dash_news_list = dashboard_view.1.3.clone();
        let dash_news_status = dashboard_view.1.4.clone();
        let dash_label_0_init = dashboard_view.1.0.clone();
        let dash_label_1_init = dashboard_view.1.1.clone();
        let dash_label_2_init = dashboard_view.1.2.clone();
        let dash_news_list_init = dashboard_view.1.3.clone();
        let dash_news_status_init = dashboard_view.1.4.clone();

        refresh_btn.connect_clicked(move |btn| {
            log_info("Refreshing package lists");

            // Visual feedback - disable button and show loading
            btn.set_sensitive(false);
            let btn_clone = btn.clone();

            Self::refresh_installed(
                &installed_list,
                &installed_packages_clone,
                task_queue_for_refresh.clone(),
                Some(installed_renderer_for_refresh.clone()),
                Some(refresh_label_clone.clone()),
                Some(refresh_timer_clone.clone()),
            );
            Self::refresh_updates(
                &updates_list,
                &updates_clone,
                task_queue_for_refresh.clone(),
                Some(updates_renderer.clone()),
                Some(refresh_label_clone.clone()),
                Some(refresh_timer_clone.clone()),
            );
            refresh_label_clone
                .borrow()
                .set_text("Refreshing package data...");

            // Update dashboard stats
            Self::refresh_dashboard_stats(&dash_label_0, &dash_label_1, &dash_label_2);
            Self::refresh_arch_news(&dash_news_list, &dash_news_status);

            // Clear search filter after refresh
            installed_search.borrow().set_text("");

            // Re-enable button after a short delay
            glib::timeout_add_local_once(Duration::from_millis(1500), move || {
                btn_clone.set_sensitive(true);
            });
        });

        // Initial load
        let cached_installed = crate::data_store::cached_installed();
        let installed_cache_fresh =
            crate::data_store::cached_installed_at().is_some_and(Self::is_cache_within_ttl);
        if !cached_installed.is_empty() && installed_cache_fresh {
            *gui.installed_packages.borrow_mut() = cached_installed.clone();
            (installed_view.4)();
        }

        let cached_updates = crate::data_store::cached_updates();
        let updates_cache_fresh =
            crate::data_store::cached_updates_at().is_some_and(Self::is_cache_within_ttl);
        if !cached_updates.is_empty() && updates_cache_fresh {
            *gui.updates.borrow_mut() = cached_updates;
            (updates_view.3)();
        }

        Self::refresh_installed(
            &installed_view.2,
            &gui.installed_packages,
            gui.task_queue.clone(),
            Some(installed_renderer.clone()),
            Some(gui.last_refresh_label.clone()),
            Some(gui.refresh_timer_id.clone()),
        );

        if crate::settings::get().check_updates_on_startup {
            Self::refresh_updates(
                &updates_view.2,
                &gui.updates,
                gui.task_queue.clone(),
                Some(updates_view.3.clone()),
                Some(gui.last_refresh_label.clone()),
                Some(gui.refresh_timer_id.clone()),
            );
        }

        Self::refresh_dashboard_stats(&dash_label_0_init, &dash_label_1_init, &dash_label_2_init);
        Self::refresh_arch_news(&dash_news_list_init, &dash_news_status_init);
        Self::update_refresh_time_from_cache(&gui.last_refresh_label, &gui.refresh_timer_id);
        Self::setup_auto_refresh(
            &installed_view.2,
            &updates_view.2,
            &gui.installed_packages,
            &gui.updates,
            gui.task_queue.clone(),
            Some(installed_renderer.clone()),
            Some(updates_view.3.clone()),
            &dash_label_0_init,
            &dash_label_1_init,
            &dash_label_2_init,
            &dash_news_list_init,
            &dash_news_status_init,
            &gui.last_refresh_label,
            &gui.refresh_timer_id,
            &gui.auto_refresh_timer_id,
        );
        Self::setup_network_reconnect_refresh(
            &installed_view.2,
            &updates_view.2,
            &gui.installed_packages,
            &gui.updates,
            gui.task_queue.clone(),
            Some(installed_renderer.clone()),
            Some(updates_view.3.clone()),
            &dash_label_0_init,
            &dash_label_1_init,
            &dash_label_2_init,
            &dash_news_list_init,
            &dash_news_status_init,
            &gui.last_refresh_label,
            &gui.refresh_timer_id,
        );

        gui
    }

    pub fn main_widget(&self) -> &Box {
        &self.main_box
    }

    fn run_blocking<T, F, C>(work: F, on_complete: C)
    where
        T: Send + 'static,
        F: FnOnce() -> T + Send + 'static,
        C: FnOnce(T) + 'static,
    {
        let (tx, rx) = mpsc::channel::<T>();
        thread::spawn(move || {
            let _ = tx.send(work());
        });

        let mut on_complete = Some(on_complete);
        glib::timeout_add_local(Duration::from_millis(25), move || match rx.try_recv() {
            Ok(value) => {
                if let Some(cb) = on_complete.take() {
                    cb(value);
                }
                glib::ControlFlow::Break
            }
            Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(mpsc::TryRecvError::Disconnected) => glib::ControlFlow::Break,
        });
    }

    fn auto_refresh_interval_seconds() -> Option<u32> {
        match crate::settings::get().auto_refresh_interval.as_str() {
            "15m" => Some(15 * 60),
            "30m" => Some(30 * 60),
            "1h" => Some(60 * 60),
            "6h" => Some(6 * 60 * 60),
            _ => None,
        }
    }

    fn filter_updates_by_source(packages: Vec<Package>) -> Vec<Package> {
        let settings = crate::settings::get();
        let ignored: std::collections::HashSet<String> = settings
            .ignored_updates
            .into_iter()
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect();
        let scoped: Vec<Package> = match settings.show_only_updates_from.as_str() {
            "repo-only" => packages
                .into_iter()
                .filter(|p| p.repository != "aur")
                .collect(),
            "aur-only" => packages
                .into_iter()
                .filter(|p| p.repository == "aur")
                .collect(),
            _ => packages,
        };
        scoped
            .into_iter()
            .filter(|p| !ignored.contains(&p.name.to_lowercase()))
            .collect()
    }

    fn setup_auto_refresh(
        installed_list: &Rc<RefCell<ListBox>>,
        updates_list: &Rc<RefCell<ListBox>>,
        installed_packages: &Rc<RefCell<Vec<Package>>>,
        updates: &Rc<RefCell<Vec<Package>>>,
        task_queue: Arc<TaskQueue>,
        installed_renderer: Option<Rc<dyn Fn()>>,
        updates_renderer: Option<Rc<dyn Fn()>>,
        dash_installed: &Rc<RefCell<Label>>,
        dash_updates: &Rc<RefCell<Label>>,
        dash_aur: &Rc<RefCell<Label>>,
        dash_news_list: &Rc<RefCell<Box>>,
        dash_news_status: &Rc<RefCell<Label>>,
        refresh_label: &Rc<RefCell<Label>>,
        refresh_timer: &Rc<RefCell<Option<glib::SourceId>>>,
        auto_refresh_timer: &Rc<RefCell<Option<glib::SourceId>>>,
    ) {
        if let Some(existing) = auto_refresh_timer.borrow_mut().take() {
            existing.remove();
        }

        let Some(interval_secs) = Self::auto_refresh_interval_seconds() else {
            return;
        };

        let installed_list = installed_list.clone();
        let updates_list = updates_list.clone();
        let installed_packages = installed_packages.clone();
        let updates = updates.clone();
        let task_queue_cloned = task_queue.clone();
        let installed_renderer_cloned = installed_renderer.clone();
        let updates_renderer_cloned = updates_renderer.clone();
        let dash_installed = dash_installed.clone();
        let dash_updates = dash_updates.clone();
        let dash_aur = dash_aur.clone();
        let dash_news_list = dash_news_list.clone();
        let dash_news_status = dash_news_status.clone();
        let refresh_label = refresh_label.clone();
        let refresh_timer = refresh_timer.clone();

        let id = glib::timeout_add_seconds_local(interval_secs, move || {
            Self::refresh_installed(
                &installed_list,
                &installed_packages,
                task_queue_cloned.clone(),
                installed_renderer_cloned.clone(),
                Some(refresh_label.clone()),
                Some(refresh_timer.clone()),
            );
            Self::refresh_updates(
                &updates_list,
                &updates,
                task_queue_cloned.clone(),
                updates_renderer_cloned.clone(),
                Some(refresh_label.clone()),
                Some(refresh_timer.clone()),
            );
            Self::refresh_dashboard_stats(&dash_installed, &dash_updates, &dash_aur);
            Self::refresh_arch_news(&dash_news_list, &dash_news_status);
            refresh_label
                .borrow()
                .set_text("Refreshing package data...");
            glib::ControlFlow::Continue
        });
        *auto_refresh_timer.borrow_mut() = Some(id);
    }

    fn setup_network_reconnect_refresh(
        installed_list: &Rc<RefCell<ListBox>>,
        updates_list: &Rc<RefCell<ListBox>>,
        installed_packages: &Rc<RefCell<Vec<Package>>>,
        updates: &Rc<RefCell<Vec<Package>>>,
        task_queue: Arc<TaskQueue>,
        installed_renderer: Option<Rc<dyn Fn()>>,
        updates_renderer: Option<Rc<dyn Fn()>>,
        dash_installed: &Rc<RefCell<Label>>,
        dash_updates: &Rc<RefCell<Label>>,
        dash_aur: &Rc<RefCell<Label>>,
        dash_news_list: &Rc<RefCell<Box>>,
        dash_news_status: &Rc<RefCell<Label>>,
        refresh_label: &Rc<RefCell<Label>>,
        refresh_timer: &Rc<RefCell<Option<glib::SourceId>>>,
    ) {
        if !crate::settings::get().refresh_on_network_reconnect {
            return;
        }

        let monitor = gio::NetworkMonitor::default();
        let installed_list = installed_list.clone();
        let updates_list = updates_list.clone();
        let installed_packages = installed_packages.clone();
        let updates = updates.clone();
        let task_queue = task_queue.clone();
        let installed_renderer = installed_renderer.clone();
        let updates_renderer = updates_renderer.clone();
        let dash_installed = dash_installed.clone();
        let dash_updates = dash_updates.clone();
        let dash_aur = dash_aur.clone();
        let dash_news_list = dash_news_list.clone();
        let dash_news_status = dash_news_status.clone();
        let refresh_label = refresh_label.clone();
        let refresh_timer = refresh_timer.clone();

        monitor.connect_network_changed(move |_, available| {
            if !available {
                return;
            }
            Self::refresh_installed(
                &installed_list,
                &installed_packages,
                task_queue.clone(),
                installed_renderer.clone(),
                Some(refresh_label.clone()),
                Some(refresh_timer.clone()),
            );
            Self::refresh_updates(
                &updates_list,
                &updates,
                task_queue.clone(),
                updates_renderer.clone(),
                Some(refresh_label.clone()),
                Some(refresh_timer.clone()),
            );
            Self::refresh_dashboard_stats(&dash_installed, &dash_updates, &dash_aur);
            Self::refresh_arch_news(&dash_news_list, &dash_news_status);
            refresh_label
                .borrow()
                .set_text("Network reconnected, refreshing...");
        });
    }

    fn show_confirmation_dialog<F>(parent: &gtk4::Window, title: &str, body: &str, on_confirm: F)
    where
        F: Fn() + 'static,
    {
        let dialog = Window::builder()
            .title(title)
            .default_width(420)
            .default_height(170)
            .modal(true)
            .transient_for(parent)
            .build();

        let vbox = Box::new(Orientation::Vertical, 12);
        vbox.set_margin_start(16);
        vbox.set_margin_end(16);
        vbox.set_margin_top(16);
        vbox.set_margin_bottom(16);

        let label = Label::new(Some(body));
        label.set_wrap(true);
        label.set_xalign(0.0);
        vbox.append(&label);

        let buttons = Box::new(Orientation::Horizontal, 8);
        buttons.set_halign(gtk4::Align::End);
        let cancel = Button::with_label("Cancel");
        let confirm = Button::with_label("Confirm");
        confirm.add_css_class("destructive-action");
        buttons.append(&cancel);
        buttons.append(&confirm);
        vbox.append(&buttons);
        dialog.set_child(Some(&vbox));

        let dialog_weak = dialog.downgrade();
        cancel.connect_clicked(move |_| {
            if let Some(d) = dialog_weak.upgrade() {
                d.close();
            }
        });

        let dialog_weak2 = dialog.downgrade();
        confirm.connect_clicked(move |_| {
            on_confirm();
            if let Some(d) = dialog_weak2.upgrade() {
                d.close();
            }
        });

        dialog.present();
    }

    fn format_bytes(bytes: u64) -> String {
        const KB: f64 = 1024.0;
        const MB: f64 = KB * 1024.0;
        const GB: f64 = MB * 1024.0;
        let b = bytes as f64;
        if b >= GB {
            format!("{:.1} GB", b / GB)
        } else if b >= MB {
            format!("{:.1} MB", b / MB)
        } else if b >= KB {
            format!("{:.1} KB", b / KB)
        } else {
            format!("{} B", bytes)
        }
    }

    fn show_cleanup_wizard(parent: &gtk4::Window, task_queue: Arc<TaskQueue>) {
        let dialog = Window::builder()
            .title("Cleanup Wizard")
            .default_width(520)
            .default_height(320)
            .modal(true)
            .transient_for(parent)
            .build();

        let root = Box::new(Orientation::Vertical, 12);
        root.set_margin_start(16);
        root.set_margin_end(16);
        root.set_margin_top(16);
        root.set_margin_bottom(16);

        let description = Label::new(Some(
            "Choose cleanup actions. Estimated reclaim is shown below.",
        ));
        description.set_halign(gtk4::Align::Start);
        description.add_css_class("dim-label");
        root.append(&description);

        let check_cache = CheckButton::with_label("Clean package cache");
        check_cache.set_active(true);
        root.append(&check_cache);

        let check_orphans = CheckButton::with_label("Remove orphaned dependencies");
        check_orphans.set_active(true);
        root.append(&check_orphans);

        let estimate_label = Label::new(Some("Calculating estimates..."));
        estimate_label.set_halign(gtk4::Align::Start);
        estimate_label.set_wrap(true);
        root.append(&estimate_label);

        let buttons = Box::new(Orientation::Horizontal, 8);
        buttons.set_halign(gtk4::Align::End);
        let cancel_btn = Button::with_label("Cancel");
        let run_btn = Button::with_label("Run Cleanup");
        run_btn.add_css_class("suggested-action");
        buttons.append(&cancel_btn);
        buttons.append(&run_btn);
        root.append(&buttons);

        dialog.set_child(Some(&root));

        let estimate_label_clone = estimate_label.clone();
        Self::run_blocking(ParuBackend::estimate_cleanup, move |estimate| {
            let cache = Self::format_bytes(estimate.pacman_cache_bytes);
            let paru = Self::format_bytes(estimate.paru_clone_bytes);
            estimate_label_clone.set_text(&format!(
                "Estimated reclaim:\n • Pacman cache: {}\n • Paru build cache: {}\n • Orphans: {} package(s)",
                cache, paru, estimate.orphan_count
            ));
        });

        let dialog_weak = dialog.downgrade();
        cancel_btn.connect_clicked(move |_| {
            if let Some(d) = dialog_weak.upgrade() {
                d.close();
            }
        });

        let dialog_weak2 = dialog.downgrade();
        run_btn.connect_clicked(move |_| {
            if check_cache.is_active() {
                task_queue.add_task(TaskType::CleanCache, "system".to_string());
            }
            if check_orphans.is_active() {
                task_queue.add_task(TaskType::RemoveOrphans, "system".to_string());
            }
            if let Some(d) = dialog_weak2.upgrade() {
                d.close();
            }
        });

        dialog.present();
    }

    fn create_dashboard_view(
        task_queue: Arc<TaskQueue>,
    ) -> (
        ScrolledWindow,
        (
            Rc<RefCell<Label>>,
            Rc<RefCell<Label>>,
            Rc<RefCell<Label>>,
            Rc<RefCell<Box>>,
            Rc<RefCell<Label>>,
        ),
    ) {
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

        let subtitle = Label::new(Some(
            "A beautiful graphical frontend for the Paru AUR helper",
        ));
        subtitle.add_css_class("dim-label");
        subtitle.set_halign(gtk4::Align::Start);
        header_box.append(&subtitle);

        vbox.append(&header_box);

        // Stats cards row
        let stats_box = Box::new(Orientation::Horizontal, 16);
        stats_box.set_homogeneous(true);
        stats_box.set_margin_top(12);

        // Installed packages stat
        let installed_stat =
            Self::create_stat_card("0", "Installed Packages", "drive-harddisk-symbolic");
        let installed_count_label = installed_stat.1;
        stats_box.append(&installed_stat.0);

        // Updates available stat
        let updates_stat = Self::create_stat_card(
            "0",
            "Updates Available",
            "software-update-available-symbolic",
        );
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
            "Update all packages to their latest versions",
        );
        let tq = task_queue.clone();
        update_btn.connect_clicked(move |btn| {
            let needs_confirm =
                crate::settings::get().confirm_update_all || crate::settings::get().confirm_actions;
            if needs_confirm {
                if let Some(window) = btn.root().and_then(|w| w.downcast::<gtk4::Window>().ok()) {
                    let tq_confirm = tq.clone();
                    Self::show_confirmation_dialog(
                        &window,
                        "Confirm System Update",
                        "Update all packages now?",
                        move || {
                            log_info("Starting system update from dashboard");
                            tq_confirm.add_task(TaskType::Update, "system".to_string());
                        },
                    );
                }
            } else {
                log_info("Starting system update from dashboard");
                tq.add_task(TaskType::Update, "system".to_string());
            }
        });
        actions_box.append(&update_btn);

        // Orphans cleanup button
        let clean_btn = Self::create_action_button(
            "Cleanup Wizard",
            "user-trash-symbolic",
            "Estimate reclaimable space and clean cache/orphans",
        );
        let tq_clean = task_queue.clone();
        clean_btn.connect_clicked(move |btn| {
            if let Some(window) = btn.root().and_then(|w| w.downcast::<gtk4::Window>().ok()) {
                Self::show_cleanup_wizard(&window, tq_clean.clone());
            }
        });
        actions_box.append(&clean_btn);

        // Orphan cleanup button
        let orphan_btn = Self::create_action_button(
            "Remove Orphans",
            "edit-clear-all-symbolic",
            "Remove unused dependencies (orphans)",
        );
        let tq_orphan = task_queue.clone();
        orphan_btn.connect_clicked(move |btn| {
            if crate::settings::get().confirm_remove_orphans {
                if let Some(window) = btn.root().and_then(|w| w.downcast::<gtk4::Window>().ok()) {
                    let tq_confirm = tq_orphan.clone();
                    Self::show_confirmation_dialog(
                        &window,
                        "Confirm Orphan Removal",
                        "Remove unused dependency packages now?",
                        move || {
                            log_info("Starting orphan removal from dashboard");
                            tq_confirm.add_task(TaskType::RemoveOrphans, "system".to_string());
                        },
                    );
                }
            } else {
                log_info("Starting orphan removal from dashboard");
                tq_orphan.add_task(TaskType::RemoveOrphans, "system".to_string());
            }
        });
        actions_box.append(&orphan_btn);

        vbox.append(&actions_box);

        // Arch news section
        let arch_news_label = Label::new(Some("Arch Linux News"));
        arch_news_label.add_css_class("title-2");
        arch_news_label.set_halign(gtk4::Align::Start);
        arch_news_label.set_margin_top(24);
        vbox.append(&arch_news_label);

        let news_card = Box::new(Orientation::Vertical, 8);
        news_card.add_css_class("card");
        news_card.set_margin_top(8);

        let news_header = Box::new(Orientation::Horizontal, 8);
        news_header.set_margin_start(16);
        news_header.set_margin_end(16);
        news_header.set_margin_top(12);

        let news_icon = Image::from_icon_name("applications-internet-symbolic");
        news_icon.set_pixel_size(18);
        news_header.append(&news_icon);

        let news_hint = Label::new(Some("Latest announcements from archlinux.org"));
        news_hint.add_css_class("dim-label");
        news_hint.set_halign(gtk4::Align::Start);
        news_hint.set_hexpand(true);
        news_header.append(&news_hint);
        news_card.append(&news_header);

        let news_status = Label::new(Some("Loading Arch news..."));
        news_status.add_css_class("caption");
        news_status.add_css_class("dim-label");
        news_status.set_halign(gtk4::Align::Start);
        news_status.set_margin_start(16);
        news_status.set_margin_end(16);
        news_card.append(&news_status);
        let news_status_rc = Rc::new(RefCell::new(news_status));

        let news_list = Box::new(Orientation::Vertical, 6);
        news_list.set_margin_start(16);
        news_list.set_margin_end(16);
        news_list.set_margin_bottom(16);
        news_card.append(&news_list);
        let news_list_rc = Rc::new(RefCell::new(news_list));

        news_card.set_visible(crate::settings::get().show_arch_news);
        vbox.append(&news_card);

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
             • Task queue for managing package operations",
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

        (
            scroll,
            (
                installed_count_label,
                updates_count_label,
                aur_count_label,
                news_list_rc,
                news_status_rc,
            ),
        )
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
        aur_label: &Rc<RefCell<Label>>,
    ) {
        let installed_label = installed_label.clone();
        let updates_label = updates_label.clone();
        let aur_label = aur_label.clone();

        Self::run_blocking(
            move || {
                let installed = ParuBackend::list_installed().ok();
                let updates = ParuBackend::list_updates()
                    .ok()
                    .map(Self::filter_updates_by_source);
                (installed, updates)
            },
            move |(installed, updates)| {
                if let Some(pkgs) = installed {
                    let total = pkgs.len();
                    let aur_count = pkgs.iter().filter(|p| p.repository == "aur").count();
                    installed_label.borrow().set_text(&total.to_string());
                    aur_label.borrow().set_text(&aur_count.to_string());
                }

                if let Some(update_pkgs) = updates {
                    updates_label
                        .borrow()
                        .set_text(&update_pkgs.len().to_string());
                }
            },
        );
    }

    fn render_news_items(list_box: &Rc<RefCell<Box>>, items: &[NewsItem]) {
        while let Some(child) = list_box.borrow().first_child() {
            list_box.borrow().remove(&child);
        }

        for item in items {
            let row = Box::new(Orientation::Vertical, 2);
            row.set_margin_bottom(8);

            let link_btn = Button::with_label(&item.title);
            link_btn.set_halign(gtk4::Align::Start);
            link_btn.add_css_class("flat");
            let uri = item.link.clone();
            link_btn.connect_clicked(move |_| {
                if crate::settings::get().open_links_in_external_browser {
                    let _ =
                        gio::AppInfo::launch_default_for_uri(&uri, None::<&gio::AppLaunchContext>);
                } else if let Some(display) = gtk4::gdk::Display::default() {
                    display.clipboard().set_text(&uri);
                    crate::utils::send_notification("Parut", "Link copied to clipboard");
                }
            });
            row.append(&link_btn);

            if crate::settings::get().show_arch_news_dates && !item.published.is_empty() {
                let date = Label::new(Some(&item.published));
                date.add_css_class("caption");
                date.add_css_class("dim-label");
                date.set_halign(gtk4::Align::Start);
                row.append(&date);
            }

            list_box.borrow().append(&row);
        }
    }

    fn refresh_arch_news(news_list: &Rc<RefCell<Box>>, news_status: &Rc<RefCell<Label>>) {
        if !crate::settings::get().show_arch_news {
            while let Some(child) = news_list.borrow().first_child() {
                news_list.borrow().remove(&child);
            }
            news_status
                .borrow()
                .set_text("Arch News is disabled in settings.");
            return;
        }

        news_status.borrow().set_text("Refreshing Arch news...");
        let news_list = news_list.clone();
        let news_status = news_status.clone();

        Self::run_blocking(
            move || ParuBackend::fetch_arch_news(crate::settings::get().arch_news_items),
            move |result| match result {
                Ok(items) => {
                    let item_count = items.len();
                    Self::render_news_items(&news_list, &items);
                    news_status
                        .borrow()
                        .set_text(&format!("Showing {} latest posts", item_count));
                }
                Err(e) => {
                    while let Some(child) = news_list.borrow().first_child() {
                        news_list.borrow().remove(&child);
                    }
                    let error = Label::new(Some("Unable to load Arch news right now."));
                    error.add_css_class("dim-label");
                    error.set_halign(gtk4::Align::Start);
                    news_list.borrow().append(&error);
                    news_status.borrow().set_text(&e);
                    log_error(&format!("Arch news fetch failed: {}", e));
                }
            },
        );
    }

    /// Show PKGBUILD review dialog for AUR packages
    fn show_pkgbuild_dialog(
        _window: &impl IsA<gtk4::Widget>,
        package_name: &str,
        task_queue: Arc<TaskQueue>,
    ) {
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
            "AUR packages are user-produced content. Review this PKGBUILD carefully before proceeding.",
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

        Self::run_blocking(
            move || ParuBackend::get_pkgbuild(&package_name_clone),
            move |result| match result {
                Ok(content) => {
                    text_buffer.set_text(&content);
                    install_btn_clone.set_sensitive(true);
                    loading_box_clone.borrow().set_visible(false);
                }
                Err(e) => {
                    text_buffer.set_text(&format!("Error loading PKGBUILD:\n\n{}", e));
                    loading_box_clone.borrow().set_visible(false);
                }
            },
        );

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

    fn show_package_details_dialog(window: &impl IsA<gtk4::Window>, package_name: &str) {
        let dialog = Window::builder()
            .title(format!("Package Details - {}", package_name))
            .default_width(600)
            .default_height(700)
            .modal(true)
            .transient_for(window)
            .build();

        let vbox = Box::new(Orientation::Vertical, 0);

        let header_bar = HeaderBar::new();
        header_bar.set_show_end_title_buttons(true);
        vbox.append(&header_bar);

        let scrolled = ScrolledWindow::new();
        scrolled.set_vexpand(true);
        scrolled.set_hexpand(true);

        let content_box = Box::new(Orientation::Vertical, 16);
        content_box.set_margin_start(24);
        content_box.set_margin_end(24);
        content_box.set_margin_top(24);
        content_box.set_margin_bottom(24);

        scrolled.set_child(Some(&content_box));
        vbox.append(&scrolled);

        // Header with icon and name
        let title_box = Box::new(Orientation::Horizontal, 16);
        let icon = Image::from_icon_name("package-x-generic-symbolic");
        icon.set_pixel_size(64);
        title_box.append(&icon);

        let title_info = Box::new(Orientation::Vertical, 4);
        let name_label = Label::new(Some(package_name));
        name_label.add_css_class("title-1");
        name_label.set_halign(gtk4::Align::Start);
        title_info.append(&name_label);

        let loading_label = Label::new(Some("Loading details..."));
        loading_label.add_css_class("dim-label");
        loading_label.set_halign(gtk4::Align::Start);
        title_info.append(&loading_label);

        title_box.append(&title_info);
        content_box.append(&title_box);

        content_box.append(&Separator::new(Orientation::Horizontal));

        // Grid for details
        let grid = gtk4::Grid::new();
        grid.set_column_spacing(16);
        grid.set_row_spacing(12);
        content_box.append(&grid);

        dialog.set_child(Some(&vbox));
        dialog.present();

        let loading_label_clone = loading_label.clone();
        let name = package_name.to_string();

        Self::run_blocking(
            move || ParuBackend::get_package_details(&name),
            move |result| match result {
                Ok(details) => {
                    loading_label_clone.set_text(&details.version);

                    let fields = [
                        ("Description", &details.description),
                        ("Repository", &details.repository),
                        ("URL", &details.url),
                        ("Licenses", &details.licenses),
                        ("Groups", &details.groups),
                        ("Provides", &details.provides),
                        ("Size", &details.installed_size),
                        ("Packager", &details.packager),
                        ("Build Date", &details.build_date),
                        ("Install Date", &details.install_date),
                        ("Install Reason", &details.install_reason),
                        ("Install Script", &details.install_script),
                        ("Validated By", &details.validated_by),
                    ];

                    let mut row = 0;
                    for (label_text, value) in fields {
                        if !value.is_empty() {
                            let label = Label::new(Some(label_text));
                            label.add_css_class("dim-label");
                            label.set_halign(gtk4::Align::End);
                            label.set_valign(gtk4::Align::Start);
                            grid.attach(&label, 0, row, 1, 1);

                            let value_label = Label::new(Some(value));
                            value_label.set_halign(gtk4::Align::Start);
                            value_label.set_wrap(true);
                            value_label.set_max_width_chars(50);
                            value_label.set_selectable(true);
                            grid.attach(&value_label, 1, row, 1, 1);

                            row += 1;
                        }
                    }

                    // Relationship section
                    if !details.depends_on.is_empty()
                        || !details.optional_deps.is_empty()
                        || !details.required_by.is_empty()
                        || !details.optional_for.is_empty()
                        || !details.conflicts_with.is_empty()
                        || !details.replaces.is_empty()
                    {
                        grid.attach(&Separator::new(Orientation::Horizontal), 0, row, 2, 1);
                        row += 1;
                    }

                    if !details.depends_on.is_empty() {
                        let label = Label::new(Some("Depends On"));
                        label.add_css_class("heading");
                        label.set_halign(gtk4::Align::Start);
                        label.set_margin_top(12);
                        grid.attach(&label, 0, row, 2, 1);
                        row += 1;

                        let val = Label::new(Some(&details.depends_on));
                        val.set_wrap(true);
                        val.set_max_width_chars(60);
                        val.set_halign(gtk4::Align::Start);
                        grid.attach(&val, 0, row, 2, 1);
                        row += 1;
                    }

                    if !details.optional_deps.is_empty() {
                        let label = Label::new(Some("Optional Deps"));
                        label.add_css_class("heading");
                        label.set_halign(gtk4::Align::Start);
                        label.set_margin_top(12);
                        grid.attach(&label, 0, row, 2, 1);
                        row += 1;

                        let val = Label::new(Some(&details.optional_deps));
                        val.set_wrap(true);
                        val.set_max_width_chars(60);
                        val.set_halign(gtk4::Align::Start);
                        grid.attach(&val, 0, row, 2, 1);
                        row += 1;
                    }

                    if !details.required_by.is_empty() {
                        let label = Label::new(Some("Required By"));
                        label.add_css_class("heading");
                        label.set_halign(gtk4::Align::Start);
                        label.set_margin_top(12);
                        grid.attach(&label, 0, row, 2, 1);
                        row += 1;

                        let val = Label::new(Some(&details.required_by));
                        val.set_wrap(true);
                        val.set_max_width_chars(60);
                        val.set_halign(gtk4::Align::Start);
                        grid.attach(&val, 0, row, 2, 1);
                        row += 1;
                    }

                    if !details.optional_for.is_empty() {
                        let label = Label::new(Some("Optional For"));
                        label.add_css_class("heading");
                        label.set_halign(gtk4::Align::Start);
                        label.set_margin_top(12);
                        grid.attach(&label, 0, row, 2, 1);
                        row += 1;

                        let val = Label::new(Some(&details.optional_for));
                        val.set_wrap(true);
                        val.set_max_width_chars(60);
                        val.set_halign(gtk4::Align::Start);
                        grid.attach(&val, 0, row, 2, 1);
                        row += 1;
                    }

                    if !details.conflicts_with.is_empty() {
                        let label = Label::new(Some("Conflicts With"));
                        label.add_css_class("heading");
                        label.set_halign(gtk4::Align::Start);
                        label.set_margin_top(12);
                        grid.attach(&label, 0, row, 2, 1);
                        row += 1;

                        let val = Label::new(Some(&details.conflicts_with));
                        val.set_wrap(true);
                        val.set_max_width_chars(60);
                        val.set_halign(gtk4::Align::Start);
                        grid.attach(&val, 0, row, 2, 1);
                        row += 1;
                    }

                    if !details.replaces.is_empty() {
                        let label = Label::new(Some("Replaces"));
                        label.add_css_class("heading");
                        label.set_halign(gtk4::Align::Start);
                        label.set_margin_top(12);
                        grid.attach(&label, 0, row, 2, 1);
                        row += 1;

                        let val = Label::new(Some(&details.replaces));
                        val.set_wrap(true);
                        val.set_max_width_chars(60);
                        val.set_halign(gtk4::Align::Start);
                        grid.attach(&val, 0, row, 2, 1);
                    }
                }
                Err(e) => {
                    loading_label_clone.set_text("Error loading details");
                    let err_label =
                        Label::new(Some(&format!("Failed to load package details: {}", e)));
                    err_label.add_css_class("error");
                    content_box.append(&err_label);
                }
            },
        );
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
        clear_btn.set_tooltip_text(Some(
            "Remove completed, canceled, and failed tasks from the queue",
        ));

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
                    let row = Self::create_task_row(task, task_queue.clone());
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

    fn create_task_row(task: &crate::task_queue::Task, task_queue: Arc<TaskQueue>) -> Box {
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
            TaskType::UpdatePackage => "software-update-urgent-symbolic",
            TaskType::CleanCache => "user-trash-symbolic",
            TaskType::RemoveOrphans => "edit-clear-all-symbolic",
        };
        let task_icon = Image::from_icon_name(icon_name);
        task_icon.set_pixel_size(20);
        header_box.append(&task_icon);

        let task_type_str = match task.task_type {
            TaskType::Install => "Install",
            TaskType::Remove => "Remove",
            TaskType::Update => "Update",
            TaskType::UpdatePackage => "Update Package",
            TaskType::CleanCache => "Clean Cache",
            TaskType::RemoveOrphans => "Remove Orphans",
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
            TaskStatus::Canceled => {
                let icon = Image::from_icon_name("process-stop-symbolic");
                icon.set_pixel_size(16);
                icon.add_css_class("dim-label");
                status_box.append(&icon);
                let label = Label::new(Some("Canceled"));
                label.add_css_class("dim-label");
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

                let phase_eta = Box::new(Orientation::Horizontal, 8);
                phase_eta.set_halign(gtk4::Align::Start);

                if let Some(phase) = &task.phase {
                    let phase_label = Label::new(Some(&format!("Phase: {}", phase)));
                    phase_label.add_css_class("caption");
                    phase_label.add_css_class("dim-label");
                    phase_eta.append(&phase_label);
                }

                if let Some(started_at) = task.started_at_unix {
                    if progress > 0.01 {
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .ok()
                            .map(|d| d.as_secs())
                            .unwrap_or(started_at);
                        let elapsed = now.saturating_sub(started_at) as f64;
                        let eta_secs = ((elapsed / progress) * (1.0 - progress)).max(0.0) as u64;
                        let eta_label =
                            Label::new(Some(&format!("ETA: {}", Self::format_duration(eta_secs))));
                        eta_label.add_css_class("caption");
                        eta_label.add_css_class("dim-label");
                        phase_eta.append(&eta_label);
                    }
                }

                row_box.append(&phase_eta);
            } else {
                let progress_bar = ProgressBar::new();
                progress_bar.pulse();
                row_box.append(&progress_bar);

                if let Some(phase) = &task.phase {
                    let phase_label = Label::new(Some(&format!("Phase: {}", phase)));
                    phase_label.add_css_class("caption");
                    phase_label.add_css_class("dim-label");
                    phase_label.set_halign(gtk4::Align::Start);
                    row_box.append(&phase_label);
                }
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
        if !task.output.is_empty()
            && matches!(task.status, TaskStatus::Running | TaskStatus::Failed(_))
        {
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

        // Task controls
        let controls = Box::new(Orientation::Horizontal, 8);
        controls.set_halign(gtk4::Align::End);

        match task.status {
            TaskStatus::Queued => {
                let cancel_btn = Button::with_label("Cancel");
                cancel_btn.add_css_class("flat");
                let task_id = task.id;
                let tq = task_queue.clone();
                cancel_btn.connect_clicked(move |_| {
                    tq.cancel_queued_task(task_id);
                });
                controls.append(&cancel_btn);

                let run_now_btn = Button::with_label("Run Now");
                run_now_btn.add_css_class("flat");
                let task_id_run = task.id;
                let tq_run = task_queue.clone();
                run_now_btn.connect_clicked(move |_| {
                    tq_run.run_queued_task_now(task_id_run);
                });
                controls.append(&run_now_btn);

                let up_btn = Button::with_label("Up");
                up_btn.add_css_class("flat");
                let task_id_up = task.id;
                let tq_up = task_queue.clone();
                up_btn.connect_clicked(move |_| {
                    tq_up.move_queued_task_up(task_id_up);
                });
                controls.append(&up_btn);

                let down_btn = Button::with_label("Down");
                down_btn.add_css_class("flat");
                let task_id_down = task.id;
                let tq_down = task_queue.clone();
                down_btn.connect_clicked(move |_| {
                    tq_down.move_queued_task_down(task_id_down);
                });
                controls.append(&down_btn);
            }
            TaskStatus::Running => {
                let cancel_btn = Button::with_label("Cancel");
                cancel_btn.add_css_class("destructive-action");
                let task_id = task.id;
                let tq = task_queue.clone();
                cancel_btn.connect_clicked(move |_| {
                    tq.request_cancel(task_id);
                });
                controls.append(&cancel_btn);
            }
            TaskStatus::Failed(_) => {
                let retry_btn = Button::with_label("Retry");
                retry_btn.add_css_class("suggested-action");
                let task_id = task.id;
                let tq = task_queue.clone();
                retry_btn.connect_clicked(move |_| {
                    let _ = tq.retry_failed_task(task_id);
                });
                controls.append(&retry_btn);
            }
            TaskStatus::Completed | TaskStatus::Canceled => {}
        }

        row_box.append(&controls);

        row_box
    }

    fn create_search_view(
        task_queue: Arc<TaskQueue>,
    ) -> (Box, Rc<RefCell<Vec<Package>>>, Rc<RefCell<ListBox>>) {
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

        // Search controls
        let search_box = Box::new(Orientation::Vertical, 6);
        let controls_box = Box::new(Orientation::Horizontal, 8);
        let search_entry = SearchEntry::new();
        search_entry.set_placeholder_text(Some("Search for packages in repos and AUR..."));
        search_entry.set_hexpand(true);
        controls_box.append(&search_entry);

        let sort_model = StringList::new(&["Name (A-Z)", "Name (Z-A)", "Repository"]);
        let sort_dropdown = DropDown::new(Some(sort_model), None::<gtk4::Expression>);
        sort_dropdown.set_width_request(140);
        sort_dropdown.set_selected(crate::settings::get().default_sort_search.min(2));
        controls_box.append(&sort_dropdown);
        search_box.append(&controls_box);

        let search_limit = crate::settings::get().search_result_limit;
        let hint_label = Label::new(Some(&format!(
            "Start typing to search. Results limited to {} packages.",
            search_limit
        )));
        hint_label.add_css_class("caption");
        hint_label.add_css_class("dim-label");
        hint_label.set_halign(gtk4::Align::Start);
        search_box.append(&hint_label);

        let recent_box = Box::new(Orientation::Horizontal, 6);
        recent_box.set_halign(gtk4::Align::Start);
        search_box.append(&recent_box);

        let trending_box = Box::new(Orientation::Horizontal, 6);
        trending_box.set_halign(gtk4::Align::Start);
        search_box.append(&trending_box);

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

        let packages = Rc::new(RefCell::new(Vec::<Package>::new()));
        let list_box_rc = Rc::new(RefCell::new(list_box));
        let selected_search: Rc<RefCell<HashSet<String>>> = Rc::new(RefCell::new(HashSet::new()));

        let selection_bar = Box::new(Orientation::Horizontal, 8);
        let selected_label = Label::new(Some("0 selected"));
        selected_label.add_css_class("caption");
        selected_label.add_css_class("dim-label");
        selection_bar.append(&selected_label);

        let clear_selected_btn = Button::with_label("Clear Selection");
        clear_selected_btn.add_css_class("flat");
        selection_bar.append(&clear_selected_btn);

        let install_selected_btn = Button::with_label("Install Selected");
        install_selected_btn.add_css_class("suggested-action");
        install_selected_btn.set_sensitive(false);
        selection_bar.append(&install_selected_btn);
        search_box.append(&selection_bar);

        // Debounced search handler
        let list_box_clone = list_box_rc.clone();
        let packages_clone = packages.clone();
        let loading_clone = loading_box_rc.clone();
        let hint_label_rc = Rc::new(RefCell::new(hint_label));
        let hint_clone = hint_label_rc.clone();
        let sort_dropdown_clone = sort_dropdown.clone();
        let recent_box_clone = recent_box.clone();
        let trending_box_clone = trending_box.clone();
        let search_entry_for_suggestions = search_entry.clone();
        let selected_search_clone = selected_search.clone();

        // Store the timeout ID
        let timeout_id: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
        let timeout_id_clone = timeout_id.clone();
        let selected_label_rc = Rc::new(RefCell::new(selected_label));

        let render_search_results: Rc<dyn Fn(&[Package])> = {
            let list_box = list_box_rc.clone();
            let task_queue = task_queue.clone();
            let selected = selected_search.clone();
            let selected_label = selected_label_rc.clone();
            let install_btn = install_selected_btn.clone();
            Rc::new(move |pkgs: &[Package]| {
                while let Some(child) = list_box.borrow().first_child() {
                    list_box.borrow().remove(&child);
                }

                for package in pkgs {
                    let row = Self::create_search_row(
                        package,
                        task_queue.clone(),
                        selected.clone(),
                        selected_label.clone(),
                        install_btn.clone(),
                    );
                    list_box.borrow().append(&row);
                }

                let selected_count = selected.borrow().len();
                selected_label
                    .borrow()
                    .set_text(&format!("{} selected", selected_count));
                install_btn.set_sensitive(selected_count > 0);
            })
        };
        let render_search_results_clone = render_search_results.clone();

        {
            let selected = selected_search.clone();
            let selected_label = selected_label_rc.clone();
            let install_btn = install_selected_btn.clone();
            let render = render_search_results.clone();
            let packages_for_clear = packages.clone();
            clear_selected_btn.connect_clicked(move |_| {
                selected.borrow_mut().clear();
                selected_label.borrow().set_text("0 selected");
                install_btn.set_sensitive(false);
                render(&packages_for_clear.borrow());
            });
        }

        {
            let selected = selected_search.clone();
            let all_packages = packages.clone();
            let tq = task_queue.clone();
            let render = render_search_results.clone();
            install_selected_btn.connect_clicked(move |btn| {
                let chosen = selected.borrow().clone();
                let selected_pkgs: Vec<Package> = all_packages
                    .borrow()
                    .iter()
                    .filter(|p| chosen.contains(&p.name))
                    .cloned()
                    .collect();
                if selected_pkgs.is_empty() {
                    return;
                }
                let has_aur = selected_pkgs.iter().any(|p| p.repository == "aur");
                let to_queue = selected_pkgs.clone();

                let queue_install = {
                    let tq = tq.clone();
                    let selected = selected.clone();
                    let render = render.clone();
                    let all_packages = all_packages.clone();
                    move || {
                        for pkg in &to_queue {
                            tq.add_task(TaskType::Install, pkg.name.clone());
                        }
                        selected.borrow_mut().clear();
                        render(&all_packages.borrow());
                    }
                };

                let force_confirm = crate::settings::get().confirm_batch_install;
                let strict_aur = crate::settings::get().always_show_pkgbuild_for_aur;
                let needs_confirm =
                    force_confirm || strict_aur || (has_aur && crate::settings::get().aur_pkgbuild_required);
                if needs_confirm {
                    if let Some(window) = btn.root().and_then(|w| w.downcast::<gtk4::Window>().ok())
                    {
                        let msg = if has_aur {
                            "Some selected packages are from AUR. Batch install will skip PKGBUILD review dialogs. Continue?"
                        } else {
                            "Install all selected packages?"
                        };
                        Self::show_confirmation_dialog(
                            &window,
                            "Batch Install Confirmation",
                            msg,
                            queue_install,
                        );
                    }
                } else {
                    queue_install();
                }
            });
        }

        search_entry.connect_search_changed(move |entry| {
            let query = entry.text().to_string();

            // Cancel previous timeout if any
            if let Some(id) = timeout_id_clone.borrow_mut().take() {
                id.remove();
            }

            if query.is_empty() {
                selected_search_clone.borrow_mut().clear();
                let list_box = list_box_clone.borrow();
                while let Some(child) = list_box.first_child() {
                    list_box.remove(&child);
                }
                hint_clone.borrow().set_visible(true);
                hint_clone.borrow().set_text(&format!(
                    "Start typing to search. Results limited to {} packages.",
                    crate::settings::get().search_result_limit
                ));
                loading_clone.borrow().set_visible(false);
                return;
            }

            if query.len() < 2 {
                hint_clone
                    .borrow()
                    .set_text("Type at least 2 characters to search...");
                return;
            }

            // Show loading
            loading_clone.borrow().set_visible(true);
            hint_clone.borrow().set_visible(false);

            let packages = packages_clone.clone();
            let loading = loading_clone.clone();
            let hint = hint_clone.clone();
            let sort_dropdown = sort_dropdown_clone.clone();
            let recent_box_for_timeout = recent_box_clone.clone();
            let trending_box_for_timeout = trending_box_clone.clone();
            let search_entry_for_timeout = search_entry_for_suggestions.clone();
            let query_for_search = query.clone();
            let query_for_record = query.clone();
            let selected_for_timeout = selected_search_clone.clone();
            let render_for_timeout = render_search_results_clone.clone();

            let timeout_id_inner = timeout_id_clone.clone();

            // Debounce: wait 300ms before searching
            let id = glib::timeout_add_local_once(Duration::from_millis(300), move || {
                // Clear the ID as it is executing so we don't try to remove it later
                let _ = timeout_id_inner.borrow_mut().take();

                Self::run_blocking(
                    move || {
                        let limit = crate::settings::get().search_result_limit;
                        Self::smart_search_packages(&query_for_search, limit)
                    },
                    move |result| match result {
                        Ok(results) => {
                            crate::data_store::record_search(&query_for_record);
                            Self::populate_search_suggestions(
                                &recent_box_for_timeout,
                                &trending_box_for_timeout,
                                &search_entry_for_timeout,
                            );
                            let sort_idx = sort_dropdown.selected();
                            let sorted = Self::filter_and_sort_packages(&results, "", sort_idx);
                            loading.borrow().set_visible(false);
                            if sorted.is_empty() {
                                hint.borrow().set_visible(true);
                                hint.borrow()
                                    .set_text("No packages found matching your query.");
                            } else {
                                hint.borrow().set_visible(true);
                                hint.borrow()
                                    .set_text(&format!("Found {} packages", sorted.len()));
                            }
                            *packages.borrow_mut() = sorted.clone();
                            selected_for_timeout.borrow_mut().clear();
                            render_for_timeout(&sorted);
                        }
                        Err(e) => {
                            loading.borrow().set_visible(false);
                            hint.borrow().set_visible(true);
                            hint.borrow().set_text(&format!("Search error: {}", e));
                            log_error(&format!("Search failed: {}", e));
                        }
                    },
                );
            });

            *timeout_id.borrow_mut() = Some(id);
        });

        Self::populate_search_suggestions(&recent_box, &trending_box, &search_entry);

        let list_box_clone2 = list_box_rc.clone();
        let packages_clone2 = packages.clone();
        let selected_for_sort = selected_search.clone();
        let render_for_sort = render_search_results.clone();
        sort_dropdown.connect_selected_notify(move |dd| {
            crate::settings::update(|s| s.default_sort_search = dd.selected().min(2));
            let sorted =
                Self::filter_and_sort_packages(&packages_clone2.borrow(), "", dd.selected());
            selected_for_sort.borrow_mut().clear();
            while let Some(child) = list_box_clone2.borrow().first_child() {
                list_box_clone2.borrow().remove(&child);
            }
            render_for_sort(&sorted);
        });

        (vbox, packages, list_box_rc)
    }

    fn create_installed_view(
        task_queue: Arc<TaskQueue>,
    ) -> (
        Box,
        Rc<RefCell<Vec<Package>>>,
        Rc<RefCell<ListBox>>,
        Rc<RefCell<SearchEntry>>,
        Rc<dyn Fn()>,
    ) {
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

        // Controls Box (Search + Sort)
        let controls_box = Box::new(Orientation::Horizontal, 8);

        // Search entry for filtering
        let search_entry = SearchEntry::new();
        search_entry.set_placeholder_text(Some("Filter..."));
        search_entry.set_hexpand(true);
        controls_box.append(&search_entry);

        // Sort DropDown
        let sort_model = StringList::new(&["Name (A-Z)", "Name (Z-A)", "Repository"]);
        let sort_dropdown = DropDown::new(Some(sort_model), None::<gtk4::Expression>);
        sort_dropdown.set_width_request(140);
        sort_dropdown.set_selected(crate::settings::get().default_sort_installed.min(2));
        controls_box.append(&sort_dropdown);

        let selected_label = Label::new(Some("0 selected"));
        selected_label.add_css_class("caption");
        selected_label.add_css_class("dim-label");
        controls_box.append(&selected_label);

        let clear_selected_btn = Button::with_label("Clear Selection");
        clear_selected_btn.add_css_class("flat");
        controls_box.append(&clear_selected_btn);

        let remove_selected_btn = Button::with_label("Remove Selected");
        remove_selected_btn.add_css_class("destructive-action");
        remove_selected_btn.set_sensitive(false);
        controls_box.append(&remove_selected_btn);

        vbox.append(&controls_box);

        let scrolled = ScrolledWindow::new();
        scrolled.set_vexpand(true);

        let list_box = ListBox::new();
        list_box.add_css_class("boxed-list");
        scrolled.set_child(Some(&list_box));

        let packages = Rc::new(RefCell::new(Vec::<Package>::new()));
        let list_box_rc = Rc::new(RefCell::new(list_box));
        let search_entry_rc = Rc::new(RefCell::new(search_entry.clone()));
        let selected_installed: Rc<RefCell<HashSet<String>>> =
            Rc::new(RefCell::new(HashSet::new()));
        let selected_label_rc = Rc::new(RefCell::new(selected_label));

        // Handlers
        let list_box_clone = list_box_rc.clone();
        let packages_clone = packages.clone();
        let task_queue_clone = task_queue.clone();
        let count_clone = count_label_rc.clone();
        let search_entry_clone = search_entry.clone();
        let sort_dropdown_clone = sort_dropdown.clone();
        let selected_for_render = selected_installed.clone();
        let selected_label_for_render = selected_label_rc.clone();
        let remove_btn_for_render = remove_selected_btn.clone();

        // Common update function logic
        let update_view = Rc::new(move || {
            let all_packages = packages_clone.borrow();
            let query = search_entry_clone.text().to_string();
            let sort_idx = sort_dropdown_clone.selected();

            let filtered = Self::filter_and_sort_packages(&all_packages, &query, sort_idx);

            count_clone.borrow().set_text(&format!(
                "{} / {} packages",
                filtered.len(),
                all_packages.len()
            ));
            Self::update_package_list_with_remove_selectable(
                &list_box_clone.borrow(),
                &filtered,
                task_queue_clone.clone(),
                selected_for_render.clone(),
                selected_label_for_render.clone(),
                remove_btn_for_render.clone(),
            );
        });

        // Connect Search
        let update_1 = update_view.clone();
        search_entry.connect_search_changed(move |_| {
            update_1();
        });

        // Connect Sort
        let update_2 = update_view.clone();
        sort_dropdown.connect_selected_notify(move |dd| {
            crate::settings::update(|s| s.default_sort_installed = dd.selected().min(2));
            update_2();
        });

        {
            let selected = selected_installed.clone();
            let label = selected_label_rc.clone();
            let remove_btn = remove_selected_btn.clone();
            let refresh = update_view.clone();
            clear_selected_btn.connect_clicked(move |_| {
                selected.borrow_mut().clear();
                label.borrow().set_text("0 selected");
                remove_btn.set_sensitive(false);
                refresh();
            });
        }

        {
            let selected = selected_installed.clone();
            let all_packages = packages.clone();
            let tq = task_queue.clone();
            let refresh = update_view.clone();
            remove_selected_btn.connect_clicked(move |btn| {
                let chosen = selected.borrow().clone();
                let selected_names: Vec<String> = all_packages
                    .borrow()
                    .iter()
                    .filter(|p| chosen.contains(&p.name))
                    .map(|p| p.name.clone())
                    .collect();
                if selected_names.is_empty() {
                    return;
                }
                let selected_count = selected_names.len();
                let to_remove = selected_names.clone();

                let queue_remove = {
                    let selected = selected.clone();
                    let refresh = refresh.clone();
                    let tq = tq.clone();
                    move || {
                        for name in &to_remove {
                            tq.add_task(TaskType::Remove, name.clone());
                        }
                        selected.borrow_mut().clear();
                        refresh();
                    }
                };

                let needs_confirm = crate::settings::get().confirm_batch_remove
                    || crate::settings::get().confirm_remove
                    || crate::settings::get().confirm_actions;
                if needs_confirm {
                    if let Some(window) = btn.root().and_then(|w| w.downcast::<gtk4::Window>().ok())
                    {
                        Self::show_confirmation_dialog(
                            &window,
                            "Confirm Batch Removal",
                            &format!("Remove {} selected packages?", selected_count),
                            queue_remove,
                        );
                    }
                } else {
                    queue_remove();
                }
            });
        }

        let update_view_export: Rc<dyn Fn()> = {
            let update_view = update_view.clone();
            Rc::new(move || update_view())
        };

        (
            vbox,
            packages,
            list_box_rc,
            search_entry_rc,
            update_view_export,
        )
    }

    fn create_updates_view(
        task_queue: Arc<TaskQueue>,
    ) -> (
        Box,
        Rc<RefCell<Vec<Package>>>,
        Rc<RefCell<ListBox>>,
        Rc<dyn Fn()>,
    ) {
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
        title.set_hexpand(false);
        header_box.append(&title);

        let count_label = Label::new(Some("0 updates"));
        count_label.add_css_class("badge");
        count_label.set_hexpand(true);
        count_label.set_halign(gtk4::Align::Start);
        header_box.append(&count_label);

        let selected_label = Label::new(Some("0 selected"));
        selected_label.add_css_class("caption");
        selected_label.add_css_class("dim-label");
        header_box.append(&selected_label);

        let update_box = Box::new(Orientation::Horizontal, 8);
        let update_icon = Image::from_icon_name("software-update-available-symbolic");
        update_box.append(&update_icon);
        let update_label = Label::new(Some("Update All"));
        update_box.append(&update_label);

        let update_all_btn = Button::new();
        update_all_btn.set_child(Some(&update_box));
        update_all_btn.add_css_class("suggested-action");
        update_all_btn.set_tooltip_text(Some("Update all packages to their latest versions"));

        let update_selected_btn = Button::with_label("Update Selected");
        update_selected_btn.add_css_class("suggested-action");
        update_selected_btn.set_sensitive(false);

        let clear_selection_btn = Button::with_label("Clear Selection");
        clear_selection_btn.add_css_class("flat");

        let task_queue_clone = task_queue.clone();
        update_all_btn.connect_clicked(move |btn| {
            let needs_confirm =
                crate::settings::get().confirm_update_all || crate::settings::get().confirm_actions;
            if needs_confirm {
                if let Some(window) = btn.root().and_then(|w| w.downcast::<gtk4::Window>().ok()) {
                    let tq_confirm = task_queue_clone.clone();
                    Self::show_confirmation_dialog(
                        &window,
                        "Confirm System Update",
                        "Update all packages now?",
                        move || {
                            log_info("Starting system update");
                            tq_confirm.add_task(TaskType::Update, "system".to_string());
                        },
                    );
                }
            } else {
                log_info("Starting system update");
                task_queue_clone.add_task(TaskType::Update, "system".to_string());
            }
        });

        header_box.append(&clear_selection_btn);
        header_box.append(&update_selected_btn);
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

        let info_label = Label::new(Some(
            "Click 'Refresh' in the header to check for new updates.",
        ));
        info_label.add_css_class("dim-label");
        info_label.set_margin_end(12);
        info_box.append(&info_label);

        vbox.append(&info_box);

        let scrolled = ScrolledWindow::new();
        scrolled.set_vexpand(true);

        let list_box = ListBox::new();
        list_box.add_css_class("boxed-list");
        scrolled.set_child(Some(&list_box));

        let packages = Rc::new(RefCell::new(Vec::<Package>::new()));
        let list_box_rc = Rc::new(RefCell::new(list_box));
        let selected_updates: Rc<RefCell<HashSet<String>>> = Rc::new(RefCell::new(HashSet::new()));

        let controls_box = Box::new(Orientation::Horizontal, 8);
        controls_box.set_margin_top(8);

        let filter_entry = SearchEntry::new();
        filter_entry.set_placeholder_text(Some("Filter updates..."));
        filter_entry.set_hexpand(true);
        controls_box.append(&filter_entry);

        let source_model = StringList::new(&["All", "Repo Only", "AUR Only"]);
        let source_dropdown = DropDown::new(Some(source_model), None::<gtk4::Expression>);
        source_dropdown.set_selected(
            match crate::settings::get().show_only_updates_from.as_str() {
                "repo-only" => 1,
                "aur-only" => 2,
                _ => 0,
            },
        );
        controls_box.append(&source_dropdown);

        let sort_model = StringList::new(&["Name (A-Z)", "Name (Z-A)", "Repository"]);
        let sort_dropdown = DropDown::new(Some(sort_model), None::<gtk4::Expression>);
        sort_dropdown.set_selected(0);
        controls_box.append(&sort_dropdown);
        vbox.append(&controls_box);
        vbox.append(&scrolled);

        let selected_label_rc = Rc::new(RefCell::new(selected_label));
        let count_label_rc = Rc::new(RefCell::new(count_label));

        let render_list: Rc<dyn Fn()> = {
            let list_box = list_box_rc.clone();
            let packages = packages.clone();
            let selected_updates = selected_updates.clone();
            let filter_entry = filter_entry.clone();
            let source_dropdown = source_dropdown.clone();
            let sort_dropdown = sort_dropdown.clone();
            let task_queue = task_queue.clone();
            let selected_label = selected_label_rc.clone();
            let update_selected_btn = update_selected_btn.clone();
            let count_label = count_label_rc.clone();

            Rc::new(move || {
                let query = filter_entry.text().to_string().to_lowercase();

                let mut filtered: Vec<Package> = packages
                    .borrow()
                    .iter()
                    .filter(|pkg| {
                        if query.is_empty() {
                            true
                        } else {
                            pkg.name.to_lowercase().contains(&query)
                                || pkg.description.to_lowercase().contains(&query)
                        }
                    })
                    .cloned()
                    .collect();

                filtered = match source_dropdown.selected() {
                    1 => filtered
                        .into_iter()
                        .filter(|p| p.repository != "aur")
                        .collect::<Vec<_>>(),
                    2 => filtered
                        .into_iter()
                        .filter(|p| p.repository == "aur")
                        .collect::<Vec<_>>(),
                    _ => filtered,
                };

                match sort_dropdown.selected() {
                    1 => filtered.sort_by(|a, b| b.name.to_lowercase().cmp(&a.name.to_lowercase())),
                    2 => filtered.sort_by(|a, b| {
                        a.repository
                            .to_lowercase()
                            .cmp(&b.repository.to_lowercase())
                            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                    }),
                    _ => filtered.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase())),
                }

                let list_box = list_box.borrow();
                while let Some(child) = list_box.first_child() {
                    list_box.remove(&child);
                }

                if filtered.is_empty() {
                    let empty_box = Box::new(Orientation::Vertical, 12);
                    empty_box.set_margin_top(48);
                    empty_box.set_margin_bottom(48);
                    empty_box.set_halign(gtk4::Align::Center);
                    let empty_icon = Image::from_icon_name("software-update-available-symbolic");
                    empty_icon.set_pixel_size(64);
                    empty_icon.add_css_class("dim-label");
                    empty_box.append(&empty_icon);
                    let empty = Label::new(Some("No updates match current filters"));
                    empty.add_css_class("dim-label");
                    empty_box.append(&empty);
                    list_box.append(&empty_box);
                } else {
                    for pkg in &filtered {
                        let row = Self::create_update_row(
                            pkg,
                            task_queue.clone(),
                            selected_updates.clone(),
                            selected_label.clone(),
                            update_selected_btn.clone(),
                        );
                        list_box.append(&row);
                    }
                }

                // Cleanup stale selections
                {
                    let visible: HashSet<String> = filtered
                        .iter()
                        .map(|p| p.name.clone())
                        .collect::<HashSet<_>>();
                    selected_updates
                        .borrow_mut()
                        .retain(|name| visible.contains(name));
                }

                let selected_count = selected_updates.borrow().len();
                selected_label
                    .borrow()
                    .set_text(&format!("{} selected", selected_count));
                update_selected_btn.set_sensitive(selected_count > 0);
                count_label
                    .borrow()
                    .set_text(&format!("{} updates", filtered.len()));
            })
        };

        {
            let render = render_list.clone();
            filter_entry.connect_search_changed(move |_| render());
        }
        {
            let render = render_list.clone();
            source_dropdown.connect_selected_notify(move |dd| {
                let value = match dd.selected() {
                    1 => "repo-only",
                    2 => "aur-only",
                    _ => "all",
                };
                crate::settings::update(|s| s.show_only_updates_from = value.to_string());
                render();
            });
        }
        {
            let render = render_list.clone();
            sort_dropdown.connect_selected_notify(move |_| render());
        }
        {
            let selected_updates = selected_updates.clone();
            let render = render_list.clone();
            clear_selection_btn.connect_clicked(move |_| {
                selected_updates.borrow_mut().clear();
                render();
            });
        }
        {
            let selected_updates = selected_updates.clone();
            let task_queue = task_queue.clone();
            let render = render_list.clone();
            update_selected_btn.connect_clicked(move |_| {
                let selected = selected_updates
                    .borrow()
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>();
                for pkg in selected {
                    task_queue.add_task(TaskType::UpdatePackage, pkg);
                }
                selected_updates.borrow_mut().clear();
                render();
            });
        }

        (vbox, packages, list_box_rc, render_list)
    }

    fn create_watchlist_view(
        task_queue: Arc<TaskQueue>,
        installed_packages: Rc<RefCell<Vec<Package>>>,
        updates: Rc<RefCell<Vec<Package>>>,
    ) -> (Box, Rc<dyn Fn()>) {
        let vbox = Box::new(Orientation::Vertical, 16);
        vbox.set_margin_start(20);
        vbox.set_margin_end(20);
        vbox.set_margin_top(16);
        vbox.set_margin_bottom(16);

        let header = Box::new(Orientation::Horizontal, 12);
        let icon = Image::from_icon_name("starred-symbolic");
        icon.set_pixel_size(24);
        header.append(&icon);

        let title = Label::new(Some("Watchlist"));
        title.add_css_class("title-2");
        header.append(&title);

        let count_label = Label::new(Some("0 items"));
        count_label.add_css_class("badge");
        count_label.set_hexpand(true);
        count_label.set_halign(gtk4::Align::Start);
        header.append(&count_label);

        vbox.append(&header);

        let scrolled = ScrolledWindow::new();
        scrolled.set_vexpand(true);
        let list_box = ListBox::new();
        list_box.add_css_class("boxed-list");
        scrolled.set_child(Some(&list_box));
        vbox.append(&scrolled);

        let list_box_rc = Rc::new(RefCell::new(list_box));
        let count_label_rc = Rc::new(RefCell::new(count_label));

        let render_watchlist: Rc<dyn Fn()> = {
            let list_box = list_box_rc.clone();
            let count_label = count_label_rc.clone();
            let installed_packages = installed_packages.clone();
            let updates = updates.clone();
            let task_queue = task_queue.clone();

            Rc::new(move || {
                let favorites = crate::data_store::favorites();
                count_label
                    .borrow()
                    .set_text(&format!("{} items", favorites.len()));

                let list_box = list_box.borrow();
                while let Some(child) = list_box.first_child() {
                    list_box.remove(&child);
                }

                if favorites.is_empty() {
                    let empty_box = Box::new(Orientation::Vertical, 12);
                    empty_box.set_margin_top(48);
                    empty_box.set_margin_bottom(48);
                    empty_box.set_halign(gtk4::Align::Center);
                    let empty_icon = Image::from_icon_name("starred-symbolic");
                    empty_icon.set_pixel_size(64);
                    empty_icon.add_css_class("dim-label");
                    empty_box.append(&empty_icon);
                    let msg = Label::new(Some("No watched packages yet"));
                    msg.add_css_class("dim-label");
                    empty_box.append(&msg);
                    list_box.append(&empty_box);
                    return;
                }

                let installed = installed_packages.borrow();
                let updates = updates.borrow();

                for pkg_name in favorites {
                    let installed_pkg = installed.iter().find(|p| p.name == pkg_name);
                    let update_pkg = updates.iter().find(|p| p.name == pkg_name);

                    let row = Box::new(Orientation::Horizontal, 12);
                    row.add_css_class("package-row");
                    row.set_margin_start(16);
                    row.set_margin_end(16);
                    row.set_margin_top(12);
                    row.set_margin_bottom(12);

                    let icon = Image::from_icon_name("starred-symbolic");
                    icon.set_pixel_size(28);
                    row.append(&icon);

                    let info = Box::new(Orientation::Vertical, 4);
                    info.set_hexpand(true);

                    let name = Label::new(Some(&pkg_name));
                    name.add_css_class("heading");
                    name.set_halign(gtk4::Align::Start);
                    info.append(&name);

                    let status = if let Some(upd) = update_pkg {
                        let old = upd
                            .installed_version
                            .clone()
                            .unwrap_or_else(|| "?".to_string());
                        format!("Update available: {} -> {}", old, upd.version)
                    } else if let Some(inst) = installed_pkg {
                        format!("Installed: {}", inst.version)
                    } else {
                        "Not installed".to_string()
                    };
                    let status_label = Label::new(Some(&status));
                    status_label.add_css_class("dim-label");
                    status_label.set_halign(gtk4::Align::Start);
                    info.append(&status_label);

                    row.append(&info);

                    let actions = Box::new(Orientation::Horizontal, 8);
                    let details_btn = Button::with_label("Details");
                    let name_for_details = pkg_name.clone();
                    let row_weak = row.downgrade();
                    details_btn.connect_clicked(move |_| {
                        if let Some(r) = row_weak.upgrade() {
                            if let Some(window) =
                                r.root().and_then(|w| w.downcast::<gtk4::Window>().ok())
                            {
                                Self::show_package_details_dialog(&window, &name_for_details);
                            }
                        }
                    });
                    actions.append(&details_btn);

                    if update_pkg.is_some() {
                        let update_btn = Button::with_label("Update");
                        update_btn.add_css_class("suggested-action");
                        let name_for_update = pkg_name.clone();
                        let tq = task_queue.clone();
                        update_btn.connect_clicked(move |_| {
                            tq.add_task(TaskType::UpdatePackage, name_for_update.clone());
                        });
                        actions.append(&update_btn);
                    }

                    let unwatch = Button::with_label("Unwatch");
                    let name_for_unwatch = pkg_name.clone();
                    unwatch.connect_clicked(move |_| {
                        if crate::data_store::is_favorite(&name_for_unwatch) {
                            let _ = crate::data_store::toggle_favorite(&name_for_unwatch);
                        }
                    });
                    actions.append(&unwatch);

                    row.append(&actions);
                    list_box.append(&row);
                }
            })
        };

        render_watchlist();
        {
            let refresh = render_watchlist.clone();
            glib::timeout_add_seconds_local(2, move || {
                refresh();
                glib::ControlFlow::Continue
            });
        }

        (vbox, render_watchlist)
    }

    fn update_package_list(
        list_box: &ListBox,
        packages: &[Package],
        show_actions: bool,
        task_queue: Arc<TaskQueue>,
    ) {
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

    fn create_favorite_button(package_name: &str) -> Button {
        let favorite_btn = Button::with_label(if crate::data_store::is_favorite(package_name) {
            "★"
        } else {
            "☆"
        });
        favorite_btn.add_css_class("flat");
        favorite_btn.add_css_class("circular");
        favorite_btn.set_tooltip_text(Some("Toggle watchlist"));

        let pkg_name = package_name.to_string();
        favorite_btn.connect_clicked(move |btn| {
            let enabled = crate::data_store::toggle_favorite(&pkg_name);
            btn.set_label(if enabled { "★" } else { "☆" });
        });

        favorite_btn
    }

    fn create_package_row(
        package: &Package,
        show_actions: bool,
        task_queue: Arc<TaskQueue>,
    ) -> Box {
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
        if crate::settings::get().show_package_sizes_in_lists {
            if let Some(size_text) = Self::query_package_size_text(&package.name) {
                let size_label = Label::new(Some(&size_text));
                size_label.add_css_class("caption");
                size_label.add_css_class("dim-label");
                size_label.set_halign(gtk4::Align::Start);
                info_box.append(&size_label);
            }
        }

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

        if crate::settings::get().show_package_details_on_single_click {
            let pkg_name_click = package.name.clone();
            let info_box_weak = info_box.downgrade();
            let click = gtk4::GestureClick::new();
            click.set_button(1);
            click.connect_released(move |_, _, _, _| {
                if let Some(info) = info_box_weak.upgrade() {
                    if let Some(window) =
                        info.root().and_then(|w| w.downcast::<gtk4::Window>().ok())
                    {
                        Self::show_package_details_dialog(&window, &pkg_name_click);
                    }
                }
            });
            info_box.add_controller(click);
        }

        row_box.append(&info_box);

        if show_actions {
            let action_box = Box::new(Orientation::Horizontal, 8);

            let favorite_btn = Self::create_favorite_button(&package.name);
            action_box.append(&favorite_btn);

            let info_icon = Image::from_icon_name("dialog-information-symbolic");
            let info_btn = Button::new();
            info_btn.set_child(Some(&info_icon));
            info_btn.add_css_class("flat");
            info_btn.add_css_class("circular");
            info_btn.set_tooltip_text(Some("View details"));

            let pkg_name_clone = package.name.clone();
            let row_weak = row_box.downgrade();
            info_btn.connect_clicked(move |_| {
                if let Some(row) = row_weak.upgrade() {
                    if let Some(window) = row.root().and_then(|w| w.downcast::<gtk4::Window>().ok())
                    {
                        Self::show_package_details_dialog(&window, &pkg_name_clone);
                    }
                }
            });
            action_box.append(&info_btn);

            if package.installed_version.is_some() {
                let remove_icon = Image::from_icon_name("user-trash-symbolic");
                let remove_btn = Button::new();
                remove_btn.set_child(Some(&remove_icon));
                remove_btn.add_css_class("destructive-action");
                remove_btn.add_css_class("circular");
                remove_btn.set_tooltip_text(Some("Remove this package"));

                let pkg_name = package.name.clone();
                let row_box_weak = row_box.downgrade();
                remove_btn.connect_clicked(move |_| {
                    let needs_confirm = crate::settings::get().confirm_remove
                        || crate::settings::get().confirm_actions;
                    if needs_confirm {
                        if let Some(row_box) = row_box_weak.upgrade() {
                            if let Some(window) = row_box
                                .root()
                                .and_then(|w| w.downcast::<gtk4::Window>().ok())
                            {
                                let tq = task_queue.clone();
                                let pkg = pkg_name.clone();
                                Self::show_confirmation_dialog(
                                    &window,
                                    "Confirm Package Removal",
                                    &format!("Remove package '{}'?", pkg_name),
                                    move || {
                                        log_info(&format!(
                                            "Adding remove task for package: {}",
                                            pkg
                                        ));
                                        tq.add_task(TaskType::Remove, pkg.clone());
                                    },
                                );
                            }
                        }
                    } else {
                        log_info(&format!("Adding remove task for package: {}", pkg_name));
                        task_queue.add_task(TaskType::Remove, pkg_name.clone());
                    }
                });

                action_box.append(&remove_btn);
            } else {
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

                    if is_aur && crate::settings::get().aur_pkgbuild_required {
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
            }
            row_box.append(&action_box);
        } else {
            // For updates view or others where show_actions is false, we still want details
            let action_box = Box::new(Orientation::Horizontal, 8);

            let favorite_btn = Self::create_favorite_button(&package.name);
            action_box.append(&favorite_btn);

            let info_icon = Image::from_icon_name("dialog-information-symbolic");
            let info_btn = Button::new();
            info_btn.set_child(Some(&info_icon));
            info_btn.add_css_class("flat");
            info_btn.add_css_class("circular");
            info_btn.set_tooltip_text(Some("View details"));

            let pkg_name_clone = package.name.clone();
            let row_weak = row_box.downgrade();
            info_btn.connect_clicked(move |_| {
                if let Some(row) = row_weak.upgrade() {
                    if let Some(window) = row.root().and_then(|w| w.downcast::<gtk4::Window>().ok())
                    {
                        Self::show_package_details_dialog(&window, &pkg_name_clone);
                    }
                }
            });
            action_box.append(&info_btn);
            row_box.append(&action_box);
        }

        row_box
    }

    fn create_update_row(
        package: &Package,
        task_queue: Arc<TaskQueue>,
        selected_updates: Rc<RefCell<HashSet<String>>>,
        selected_label: Rc<RefCell<Label>>,
        update_selected_btn: Button,
    ) -> Box {
        let row_box = Box::new(Orientation::Horizontal, 12);
        row_box.add_css_class("package-row");
        row_box.set_margin_start(16);
        row_box.set_margin_end(16);
        row_box.set_margin_top(12);
        row_box.set_margin_bottom(12);

        let select_btn = CheckButton::new();
        select_btn.set_active(selected_updates.borrow().contains(&package.name));
        let name_for_select = package.name.clone();
        let selected_updates_clone = selected_updates.clone();
        let selected_label_for_select = selected_label.clone();
        let update_selected_for_select = update_selected_btn.clone();
        select_btn.connect_toggled(move |btn| {
            if btn.is_active() {
                selected_updates_clone
                    .borrow_mut()
                    .insert(name_for_select.clone());
            } else {
                selected_updates_clone.borrow_mut().remove(&name_for_select);
            }
            let selected_count = selected_updates_clone.borrow().len();
            selected_label_for_select
                .borrow()
                .set_text(&format!("{} selected", selected_count));
            update_selected_for_select.set_sensitive(selected_count > 0);
        });
        row_box.append(&select_btn);

        let pkg_icon = Image::from_icon_name("software-update-available-symbolic");
        pkg_icon.set_pixel_size(28);
        pkg_icon.add_css_class("dim-label");
        row_box.append(&pkg_icon);

        let info_box = Box::new(Orientation::Vertical, 4);
        info_box.set_hexpand(true);

        let name_box = Box::new(Orientation::Horizontal, 8);
        let name_label = Label::new(Some(&package.name));
        name_label.add_css_class("heading");
        name_label.set_halign(gtk4::Align::Start);
        name_box.append(&name_label);

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

        let versions = Box::new(Orientation::Horizontal, 8);
        if let Some(installed) = &package.installed_version {
            let old = Label::new(Some(installed));
            old.add_css_class("version-badge");
            versions.append(&old);

            let arrow = Label::new(Some("→"));
            arrow.add_css_class("version-update");
            versions.append(&arrow);
        }
        let new_ver = Label::new(Some(&package.version));
        new_ver.add_css_class("version-badge");
        new_ver.add_css_class("version-update");
        versions.append(&new_ver);
        info_box.append(&versions);
        if crate::settings::get().show_package_sizes_in_lists {
            if let Some(size_text) = Self::query_package_size_text(&package.name) {
                let size_label = Label::new(Some(&size_text));
                size_label.add_css_class("caption");
                size_label.add_css_class("dim-label");
                size_label.set_halign(gtk4::Align::Start);
                info_box.append(&size_label);
            }
        }

        row_box.append(&info_box);

        let action_box = Box::new(Orientation::Horizontal, 8);

        let favorite_btn = Self::create_favorite_button(&package.name);
        action_box.append(&favorite_btn);

        let info_btn = Button::new();
        info_btn.set_child(Some(&Image::from_icon_name("dialog-information-symbolic")));
        info_btn.add_css_class("flat");
        info_btn.add_css_class("circular");
        info_btn.set_tooltip_text(Some("View details"));
        let pkg_for_info = package.name.clone();
        let row_weak = row_box.downgrade();
        info_btn.connect_clicked(move |_| {
            if let Some(row) = row_weak.upgrade() {
                if let Some(window) = row.root().and_then(|w| w.downcast::<gtk4::Window>().ok()) {
                    Self::show_package_details_dialog(&window, &pkg_for_info);
                }
            }
        });
        action_box.append(&info_btn);

        let is_ignored = crate::settings::get()
            .ignored_updates
            .iter()
            .any(|p| p.eq_ignore_ascii_case(&package.name));
        let ignore_btn = Button::with_label(if is_ignored { "Unignore" } else { "Ignore" });
        ignore_btn.add_css_class("flat");
        let pkg_for_ignore = package.name.clone();
        let selected_updates_for_ignore = selected_updates.clone();
        let selected_label_for_ignore = selected_label.clone();
        let update_selected_for_ignore = update_selected_btn.clone();
        let row_weak_ignore = row_box.downgrade();
        ignore_btn.connect_clicked(move |btn| {
            let now_ignored = crate::settings::update_and_get(|s| {
                let exists = s
                    .ignored_updates
                    .iter()
                    .any(|p| p.eq_ignore_ascii_case(&pkg_for_ignore));
                if exists {
                    s.ignored_updates
                        .retain(|p| !p.eq_ignore_ascii_case(&pkg_for_ignore));
                    false
                } else {
                    s.ignored_updates.push(pkg_for_ignore.clone());
                    s.ignored_updates.sort();
                    s.ignored_updates.dedup();
                    true
                }
            })
            .unwrap_or(false);

            if now_ignored {
                selected_updates_for_ignore
                    .borrow_mut()
                    .remove(&pkg_for_ignore);
                let count = selected_updates_for_ignore.borrow().len();
                selected_label_for_ignore
                    .borrow()
                    .set_text(&format!("{} selected", count));
                update_selected_for_ignore.set_sensitive(count > 0);
                if let Some(row) = row_weak_ignore.upgrade() {
                    row.set_visible(false);
                }
            } else {
                btn.set_label("Ignore");
            }
        });
        action_box.append(&ignore_btn);

        let update_btn = Button::with_label("Update");
        update_btn.add_css_class("suggested-action");
        let pkg_for_update = package.name.clone();
        update_btn.connect_clicked(move |_| {
            task_queue.add_task(TaskType::UpdatePackage, pkg_for_update.clone());
        });
        action_box.append(&update_btn);

        row_box.append(&action_box);
        row_box
    }

    fn create_search_row(
        package: &Package,
        task_queue: Arc<TaskQueue>,
        selected: Rc<RefCell<HashSet<String>>>,
        selected_label: Rc<RefCell<Label>>,
        install_selected_btn: Button,
    ) -> Box {
        let row_box = Self::create_package_row(package, true, task_queue);

        let select_btn = CheckButton::new();
        select_btn.set_active(selected.borrow().contains(&package.name));
        let pkg_name = package.name.clone();
        let selected_clone = selected.clone();
        select_btn.connect_toggled(move |btn| {
            if btn.is_active() {
                selected_clone.borrow_mut().insert(pkg_name.clone());
            } else {
                selected_clone.borrow_mut().remove(&pkg_name);
            }
            let count = selected_clone.borrow().len();
            selected_label
                .borrow()
                .set_text(&format!("{} selected", count));
            install_selected_btn.set_sensitive(count > 0);
        });
        row_box.prepend(&select_btn);

        row_box
    }

    fn update_package_list_with_remove(
        list_box: &ListBox,
        packages: &[Package],
        task_queue: Arc<TaskQueue>,
    ) {
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

    fn update_package_list_with_remove_selectable(
        list_box: &ListBox,
        packages: &[Package],
        task_queue: Arc<TaskQueue>,
        selected: Rc<RefCell<HashSet<String>>>,
        selected_label: Rc<RefCell<Label>>,
        remove_selected_btn: Button,
    ) {
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
            let select_btn = CheckButton::new();
            select_btn.set_active(selected.borrow().contains(&package.name));
            let pkg_name = package.name.clone();
            let selected_clone = selected.clone();
            let selected_label_clone = selected_label.clone();
            let remove_btn_clone = remove_selected_btn.clone();
            select_btn.connect_toggled(move |btn| {
                if btn.is_active() {
                    selected_clone.borrow_mut().insert(pkg_name.clone());
                } else {
                    selected_clone.borrow_mut().remove(&pkg_name);
                }
                let count = selected_clone.borrow().len();
                selected_label_clone
                    .borrow()
                    .set_text(&format!("{} selected", count));
                remove_btn_clone.set_sensitive(count > 0);
            });
            row.prepend(&select_btn);
            list_box.append(&row);
        }

        let selected_count = selected.borrow().len();
        selected_label
            .borrow()
            .set_text(&format!("{} selected", selected_count));
        remove_selected_btn.set_sensitive(selected_count > 0);
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
        if crate::settings::get().show_package_sizes_in_lists {
            if let Some(size_text) = Self::query_package_size_text(&package.name) {
                let size_label = Label::new(Some(&size_text));
                size_label.add_css_class("caption");
                size_label.add_css_class("dim-label");
                size_label.set_halign(gtk4::Align::Start);
                info_box.append(&size_label);
            }
        }

        if crate::settings::get().show_package_details_on_single_click {
            let pkg_name_click = package.name.clone();
            let info_box_weak = info_box.downgrade();
            let click = gtk4::GestureClick::new();
            click.set_button(1);
            click.connect_released(move |_, _, _, _| {
                if let Some(info) = info_box_weak.upgrade() {
                    if let Some(window) =
                        info.root().and_then(|w| w.downcast::<gtk4::Window>().ok())
                    {
                        Self::show_package_details_dialog(&window, &pkg_name_click);
                    }
                }
            });
            info_box.add_controller(click);
        }

        row_box.append(&info_box);

        // Remove button
        let action_box = Box::new(Orientation::Horizontal, 8);

        let favorite_btn = Self::create_favorite_button(&package.name);
        action_box.append(&favorite_btn);

        // Info button
        let info_icon = Image::from_icon_name("dialog-information-symbolic");
        let info_btn = Button::new();
        info_btn.set_child(Some(&info_icon));
        info_btn.add_css_class("flat");
        info_btn.add_css_class("circular");
        info_btn.set_tooltip_text(Some("View details"));

        let pkg_name_info = package.name.clone();
        let row_weak = row_box.downgrade();
        info_btn.connect_clicked(move |_| {
            if let Some(row) = row_weak.upgrade() {
                if let Some(window) = row.root().and_then(|w| w.downcast::<gtk4::Window>().ok()) {
                    Self::show_package_details_dialog(&window, &pkg_name_info);
                }
            }
        });
        action_box.append(&info_btn);

        let remove_icon = Image::from_icon_name("user-trash-symbolic");
        let remove_btn = Button::new();
        remove_btn.set_child(Some(&remove_icon));
        remove_btn.add_css_class("destructive-action");
        remove_btn.add_css_class("circular");
        remove_btn.set_tooltip_text(Some("Remove this package"));

        let pkg_name = package.name.clone();
        let row_box_weak2 = row_box.downgrade();
        remove_btn.connect_clicked(move |_btn| {
            let needs_confirm =
                crate::settings::get().confirm_remove || crate::settings::get().confirm_actions;
            if needs_confirm {
                if let Some(row_box) = row_box_weak2.upgrade() {
                    if let Some(window) = row_box
                        .root()
                        .and_then(|w| w.downcast::<gtk4::Window>().ok())
                    {
                        let tq = task_queue.clone();
                        let pkg = pkg_name.clone();
                        Self::show_confirmation_dialog(
                            &window,
                            "Confirm Package Removal",
                            &format!("Remove package '{}'?", pkg_name),
                            move || {
                                log_info(&format!("Adding remove task for package: {}", pkg));
                                tq.add_task(TaskType::Remove, pkg.clone());
                            },
                        );
                    }
                }
            } else {
                log_info(&format!("Adding remove task for package: {}", pkg_name));
                task_queue.add_task(TaskType::Remove, pkg_name.clone());
            }
        });

        action_box.append(&remove_btn);
        row_box.append(&action_box);

        row_box
    }

    fn refresh_installed(
        list_box: &Rc<RefCell<ListBox>>,
        packages: &Rc<RefCell<Vec<Package>>>,
        task_queue: Arc<TaskQueue>,
        render_installed: Option<Rc<dyn Fn()>>,
        refresh_label: Option<Rc<RefCell<Label>>>,
        refresh_timer: Option<Rc<RefCell<Option<glib::SourceId>>>>,
    ) {
        let list_box = list_box.clone();
        let packages = packages.clone();
        let render_installed = render_installed.clone();
        let refresh_label_ok = refresh_label.clone();
        let refresh_timer_ok = refresh_timer.clone();
        let refresh_label_err = refresh_label;
        let refresh_timer_err = refresh_timer;

        Self::run_blocking(
            move || ParuBackend::list_installed(),
            move |result| match result {
                Ok(pkgs) => {
                    crate::data_store::set_cached_installed(&pkgs);
                    *packages.borrow_mut() = pkgs.clone();
                    if let Some(render) = &render_installed {
                        render();
                    } else {
                        Self::update_package_list_with_remove(
                            &list_box.borrow(),
                            &pkgs,
                            task_queue,
                        );
                    }
                    if let (Some(label), Some(timer)) = (refresh_label_ok, refresh_timer_ok) {
                        Self::update_refresh_time_from_cache(&label, &timer);
                    }
                }
                Err(e) => {
                    log_error(&format!("Error loading installed packages: {}", e));
                    if let (Some(label), Some(timer)) = (refresh_label_err, refresh_timer_err) {
                        Self::set_refresh_stale_warning(&label, &timer, &e);
                    }
                }
            },
        );
    }

    fn refresh_updates(
        list_box: &Rc<RefCell<ListBox>>,
        packages: &Rc<RefCell<Vec<Package>>>,
        task_queue: Arc<TaskQueue>,
        render_updates: Option<Rc<dyn Fn()>>,
        refresh_label: Option<Rc<RefCell<Label>>>,
        refresh_timer: Option<Rc<RefCell<Option<glib::SourceId>>>>,
    ) {
        let list_box = list_box.clone();
        let packages = packages.clone();
        let render_updates_cloned = render_updates.clone();
        let refresh_label_ok = refresh_label.clone();
        let refresh_timer_ok = refresh_timer.clone();
        let refresh_label_err = refresh_label;
        let refresh_timer_err = refresh_timer;

        Self::run_blocking(
            move || ParuBackend::list_updates(),
            move |result| match result {
                Ok(pkgs) => {
                    let pkgs = Self::filter_updates_by_source(pkgs);
                    if !pkgs.is_empty() && crate::settings::get().notifications_enabled {
                        crate::utils::send_notification(
                            "Updates Available",
                            &format!(
                                "{} new updates including: {}",
                                pkgs.len(),
                                pkgs.first().map(|p| p.name.as_str()).unwrap_or("")
                            ),
                        );
                    }
                    crate::data_store::set_cached_updates(&pkgs);
                    *packages.borrow_mut() = pkgs.clone();
                    if let Some(render) = &render_updates_cloned {
                        render();
                    } else {
                        Self::update_package_list(&list_box.borrow(), &pkgs, false, task_queue);
                    }
                    if let (Some(label), Some(timer)) = (refresh_label_ok, refresh_timer_ok) {
                        Self::update_refresh_time_from_cache(&label, &timer);
                    }
                }
                Err(e) => {
                    log_error(&format!("Error loading updates: {}", e));
                    if let (Some(label), Some(timer)) = (refresh_label_err, refresh_timer_err) {
                        Self::set_refresh_stale_warning(&label, &timer, &e);
                    }
                }
            },
        );
    }

    fn update_refresh_time_from_cache(
        label: &Rc<RefCell<Label>>,
        timer_id: &Rc<RefCell<Option<glib::SourceId>>>,
    ) {
        if let Some(existing_id) = timer_id.borrow_mut().take() {
            existing_id.remove();
        }

        let newest = std::cmp::max(
            crate::data_store::cached_installed_at().unwrap_or(0),
            crate::data_store::cached_updates_at().unwrap_or(0),
        );
        if newest <= 0 {
            label.borrow().set_text("No cached data yet");
            return;
        }

        let label_clone = label.clone();
        let newest_ts = newest;
        label
            .borrow()
            .set_text(&Self::freshness_text(newest_ts, false));

        let id = glib::timeout_add_seconds_local(30, move || {
            label_clone
                .borrow()
                .set_text(&Self::freshness_text(newest_ts, false));
            glib::ControlFlow::Continue
        });
        *timer_id.borrow_mut() = Some(id);
    }

    fn set_refresh_stale_warning(
        label: &Rc<RefCell<Label>>,
        timer_id: &Rc<RefCell<Option<glib::SourceId>>>,
        error: &str,
    ) {
        if let Some(existing_id) = timer_id.borrow_mut().take() {
            existing_id.remove();
        }
        let newest = std::cmp::max(
            crate::data_store::cached_installed_at().unwrap_or(0),
            crate::data_store::cached_updates_at().unwrap_or(0),
        );
        if newest > 0 {
            label.borrow().set_text(&format!(
                "Refresh failed, showing cached data ({})",
                Self::freshness_text(newest, true)
            ));
        } else {
            label
                .borrow()
                .set_text(&format!("Refresh failed: {}", error));
        }
    }

    fn freshness_text(unix_ts: i64, only_age: bool) -> String {
        let now = chrono::Local::now().timestamp();
        let elapsed = now.saturating_sub(unix_ts);
        let ttl_secs = crate::settings::get().cache_ttl_minutes.saturating_mul(60) as i64;
        let age = if elapsed < 60 {
            "just now".to_string()
        } else if elapsed < 3600 {
            format!("{} min ago", elapsed / 60)
        } else if elapsed < 86400 {
            format!("{} hr ago", elapsed / 3600)
        } else {
            format!("{} days ago", elapsed / 86400)
        };

        if only_age {
            age
        } else if elapsed >= ttl_secs.max(60) {
            format!("Data synced {} (stale)", age)
        } else {
            format!("Data synced {}", age)
        }
    }

    fn is_cache_within_ttl(unix_ts: i64) -> bool {
        let ttl_secs = crate::settings::get().cache_ttl_minutes.saturating_mul(60) as i64;
        if ttl_secs == 0 {
            return true;
        }
        let now = chrono::Local::now().timestamp();
        now.saturating_sub(unix_ts) <= ttl_secs
    }

    fn query_package_size_text(package_name: &str) -> Option<String> {
        let query = |flag: &str, key: &str| -> Option<String> {
            let output = Command::new("pacman")
                .arg(flag)
                .arg(package_name)
                .output()
                .ok()?;
            if !output.status.success() {
                return None;
            }
            let stdout = String::from_utf8_lossy(&output.stdout);
            let line = stdout
                .lines()
                .find(|l| l.trim_start().starts_with(key))
                .and_then(|l| l.split_once(':'))
                .map(|(_, v)| v.trim().to_string())?;
            Some(format!("Size: {}", line))
        };

        query("-Qi", "Installed Size").or_else(|| query("-Si", "Download Size"))
    }

    fn format_duration(total_secs: u64) -> String {
        if total_secs < 60 {
            format!("{}s", total_secs)
        } else if total_secs < 3600 {
            format!("{}m {}s", total_secs / 60, total_secs % 60)
        } else {
            let hours = total_secs / 3600;
            let mins = (total_secs % 3600) / 60;
            format!("{}h {}m", hours, mins)
        }
    }

    fn filter_and_sort_packages(packages: &[Package], query: &str, sort_idx: u32) -> Vec<Package> {
        let query = query.to_lowercase();
        let mut filtered: Vec<Package> = if query.is_empty() {
            packages.to_vec()
        } else {
            packages
                .iter()
                .filter(|pkg| {
                    pkg.name.to_lowercase().contains(&query)
                        || pkg.description.to_lowercase().contains(&query)
                })
                .cloned()
                .collect()
        };

        match sort_idx {
            0 => filtered.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase())), // Name A-Z
            1 => filtered.sort_by(|a, b| b.name.to_lowercase().cmp(&a.name.to_lowercase())), // Name Z-A
            2 => filtered.sort_by(|a, b| {
                a.repository
                    .cmp(&b.repository)
                    .then_with(|| a.name.cmp(&b.name))
            }), // Repository then Name
            _ => {}
        }

        filtered
    }

    fn smart_search_packages(query: &str, limit: usize) -> Result<Vec<Package>, String> {
        let direct = ParuBackend::search_packages(query, Some(limit))?;
        if !direct.is_empty() {
            return Ok(Self::rank_packages_by_query(direct, query, limit));
        }

        if query.len() < 3 {
            return Ok(direct);
        }

        // Fallback for typo tolerance: broad search by prefix, then rank by similarity.
        let prefix: String = query.chars().take(3).collect();
        let fallback_limit = (limit.saturating_mul(3)).min(500);
        let broad = ParuBackend::search_packages(&prefix, Some(fallback_limit))?;
        Ok(Self::rank_packages_by_query(broad, query, limit))
    }

    fn rank_packages_by_query(
        mut packages: Vec<Package>,
        query: &str,
        limit: usize,
    ) -> Vec<Package> {
        let norm_q = query.to_lowercase();
        packages.sort_by(|a, b| {
            let sa = Self::package_match_score(a, &norm_q);
            let sb = Self::package_match_score(b, &norm_q);
            sa.cmp(&sb)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        packages.truncate(limit);
        packages
    }

    fn package_match_score(pkg: &Package, query: &str) -> i32 {
        let name = pkg.name.to_lowercase();
        let desc = pkg.description.to_lowercase();

        if name == query {
            return 0;
        }
        if name.starts_with(query) {
            return 1;
        }
        if name.contains(query) {
            return 2;
        }
        if desc.contains(query) {
            return 3;
        }

        let dist = Self::levenshtein_bounded(&name, query, 4).unwrap_or(10);
        10 + dist
    }

    fn levenshtein_bounded(a: &str, b: &str, max_dist: usize) -> Option<i32> {
        let a_bytes = a.as_bytes();
        let b_bytes = b.as_bytes();
        if a_bytes.is_empty() {
            return Some(b_bytes.len() as i32);
        }
        if b_bytes.is_empty() {
            return Some(a_bytes.len() as i32);
        }

        let mut prev: Vec<usize> = (0..=b_bytes.len()).collect();
        let mut cur = vec![0usize; b_bytes.len() + 1];

        for (i, &ac) in a_bytes.iter().enumerate() {
            cur[0] = i + 1;
            let mut row_min = cur[0];

            for (j, &bc) in b_bytes.iter().enumerate() {
                let cost = if ac == bc { 0 } else { 1 };
                cur[j + 1] = (prev[j + 1] + 1).min(cur[j] + 1).min(prev[j] + cost);
                row_min = row_min.min(cur[j + 1]);
            }

            if row_min > max_dist {
                return None;
            }

            std::mem::swap(&mut prev, &mut cur);
        }

        let d = prev[b_bytes.len()];
        if d > max_dist { None } else { Some(d as i32) }
    }

    fn populate_search_suggestions(
        recent_box: &Box,
        trending_box: &Box,
        search_entry: &SearchEntry,
    ) {
        while let Some(child) = recent_box.first_child() {
            recent_box.remove(&child);
        }
        while let Some(child) = trending_box.first_child() {
            trending_box.remove(&child);
        }

        let recent = crate::data_store::recent_searches(5);
        if !recent.is_empty() {
            let label = Label::new(Some("Recent:"));
            label.add_css_class("caption");
            label.add_css_class("dim-label");
            recent_box.append(&label);
            for term in recent {
                let btn = Button::with_label(&term);
                btn.add_css_class("flat");
                let entry = search_entry.clone();
                btn.connect_clicked(move |_| {
                    entry.set_text(&term);
                });
                recent_box.append(&btn);
            }
        }

        let trending = crate::data_store::trending_searches(5);
        if !trending.is_empty() {
            let label = Label::new(Some("Trending:"));
            label.add_css_class("caption");
            label.add_css_class("dim-label");
            trending_box.append(&label);
            for term in trending {
                let btn = Button::with_label(&term);
                btn.add_css_class("flat");
                let entry = search_entry.clone();
                btn.connect_clicked(move |_| {
                    entry.set_text(&term);
                });
                trending_box.append(&btn);
            }
        }
    }

    fn show_settings_dialog(parent_window: &(impl IsA<gtk4::Window> + gtk4::prelude::WidgetExt)) {
        let window = Window::builder()
            .title("Preferences")
            .default_width(560)
            .default_height(720)
            .modal(true)
            .transient_for(parent_window)
            .build();
        window.add_css_class("settings-dialog");

        let vbox = Box::new(Orientation::Vertical, 0);
        vbox.add_css_class("settings-root");
        vbox.append(&HeaderBar::new());

        let prefs = PreferencesPage::new();
        prefs.add_css_class("settings-page");
        prefs.set_margin_start(12);
        prefs.set_margin_end(12);
        prefs.set_margin_top(8);
        prefs.set_margin_bottom(12);
        let current = crate::settings::get();

        let style_switch_row = |row: &ActionRow, sw: &gtk4::Switch| {
            row.add_css_class("settings-row");
            sw.add_css_class("settings-switch");
            sw.set_valign(gtk4::Align::Center);
        };
        let style_combo_row = |row: &ComboRow| {
            row.add_css_class("settings-row");
        };
        let style_group = |group: &PreferencesGroup| {
            group.add_css_class("settings-group");
        };

        let general = PreferencesGroup::new();
        general.set_title("General");
        style_group(&general);

        let row_notify = ActionRow::new();
        row_notify.set_title("System Notifications");
        let switch_notify = gtk4::Switch::new();
        switch_notify.set_active(current.notifications_enabled);
        switch_notify.connect_state_set(|_, state| {
            crate::settings::update(|s| s.notifications_enabled = state);
            glib::Propagation::Proceed
        });
        style_switch_row(&row_notify, &switch_notify);
        row_notify.add_suffix(&switch_notify);
        general.add(&row_notify);

        let row_startup = ActionRow::new();
        row_startup.set_title("Check on Startup");
        let switch_startup = gtk4::Switch::new();
        switch_startup.set_active(current.check_updates_on_startup);
        switch_startup.connect_state_set(|_, state| {
            crate::settings::update(|s| s.check_updates_on_startup = state);
            glib::Propagation::Proceed
        });
        style_switch_row(&row_startup, &switch_startup);
        row_startup.add_suffix(&switch_startup);
        general.add(&row_startup);

        let auto_row = ComboRow::new();
        auto_row.set_title("Auto Refresh");
        auto_row.set_subtitle("Requires restart to apply");
        auto_row.set_model(Some(&StringList::new(&[
            "Off", "15 min", "30 min", "1 hour", "6 hours",
        ])));
        auto_row.set_selected(match current.auto_refresh_interval.as_str() {
            "15m" => 1,
            "30m" => 2,
            "1h" => 3,
            "6h" => 4,
            _ => 0,
        });
        auto_row.connect_selected_notify(|row| {
            let value = match row.selected() {
                1 => "15m",
                2 => "30m",
                3 => "1h",
                4 => "6h",
                _ => "off",
            };
            crate::settings::update(|s| s.auto_refresh_interval = value.to_string());
        });
        style_combo_row(&auto_row);
        general.add(&auto_row);

        let ttl_row = ComboRow::new();
        ttl_row.set_title("Cache Stale After");
        ttl_row.set_model(Some(&StringList::new(&[
            "15 min", "30 min", "1 hour", "3 hours", "6 hours",
        ])));
        ttl_row.set_selected(match current.cache_ttl_minutes {
            15 => 0,
            30 => 1,
            180 => 3,
            360 => 4,
            _ => 2,
        });
        ttl_row.connect_selected_notify(|row| {
            let value = match row.selected() {
                0 => 15,
                1 => 30,
                3 => 180,
                4 => 360,
                _ => 60,
            };
            crate::settings::update(|s| s.cache_ttl_minutes = value);
        });
        style_combo_row(&ttl_row);
        general.add(&ttl_row);

        let row_net_refresh = ActionRow::new();
        row_net_refresh.set_title("Refresh on Network Reconnect");
        let sw_net_refresh = gtk4::Switch::new();
        sw_net_refresh.set_active(current.refresh_on_network_reconnect);
        sw_net_refresh.connect_state_set(|_, state| {
            crate::settings::update(|s| s.refresh_on_network_reconnect = state);
            glib::Propagation::Proceed
        });
        style_switch_row(&row_net_refresh, &sw_net_refresh);
        row_net_refresh.add_suffix(&sw_net_refresh);
        general.add(&row_net_refresh);

        let updates_src_row = ComboRow::new();
        updates_src_row.set_title("Show Updates From");
        updates_src_row.set_model(Some(&StringList::new(&["All", "Repo Only", "AUR Only"])));
        updates_src_row.set_selected(match current.show_only_updates_from.as_str() {
            "repo-only" => 1,
            "aur-only" => 2,
            _ => 0,
        });
        updates_src_row.connect_selected_notify(|row| {
            let value = match row.selected() {
                1 => "repo-only",
                2 => "aur-only",
                _ => "all",
            };
            crate::settings::update(|s| s.show_only_updates_from = value.to_string());
        });
        style_combo_row(&updates_src_row);
        general.add(&updates_src_row);

        let update_scope_row = ComboRow::new();
        update_scope_row.set_title("Default Update Scope");
        update_scope_row.set_model(Some(&StringList::new(&["All", "Repo Only", "AUR Only"])));
        update_scope_row.set_selected(match current.default_update_scope.as_str() {
            "repo-only" => 1,
            "aur-only" => 2,
            _ => 0,
        });
        update_scope_row.connect_selected_notify(|row| {
            let value = match row.selected() {
                1 => "repo-only",
                2 => "aur-only",
                _ => "all",
            };
            crate::settings::update(|s| s.default_update_scope = value.to_string());
        });
        style_combo_row(&update_scope_row);
        general.add(&update_scope_row);

        let startup_tab_row = ComboRow::new();
        startup_tab_row.set_title("Startup Tab");
        startup_tab_row.set_subtitle("Applied on next launch");
        startup_tab_row.set_model(Some(&StringList::new(&[
            "Overview",
            "Search",
            "Installed",
            "Updates",
            "Watchlist",
        ])));
        startup_tab_row.set_selected(match current.startup_tab.as_str() {
            "search" => 1,
            "installed" => 2,
            "updates" => 3,
            "watchlist" => 4,
            _ => 0,
        });
        startup_tab_row.connect_selected_notify(|row| {
            let value = match row.selected() {
                1 => "search",
                2 => "installed",
                3 => "updates",
                4 => "watchlist",
                _ => "dashboard",
            };
            crate::settings::update(|s| s.startup_tab = value.to_string());
        });
        style_combo_row(&startup_tab_row);
        general.add(&startup_tab_row);

        let search_limit_row = ComboRow::new();
        search_limit_row.set_title("Search Result Limit");
        search_limit_row.set_model(Some(&StringList::new(&["50", "100", "250", "500"])));
        search_limit_row.set_selected(match current.search_result_limit {
            50 => 0,
            250 => 2,
            500 => 3,
            _ => 1,
        });
        search_limit_row.connect_selected_notify(|row| {
            let limit = match row.selected() {
                0 => 50,
                2 => 250,
                3 => 500,
                _ => 100,
            };
            crate::settings::update(|s| s.search_result_limit = limit);
        });
        style_combo_row(&search_limit_row);
        general.add(&search_limit_row);

        let row_show_arch_news = ActionRow::new();
        row_show_arch_news.set_title("Show Arch News on Overview");
        let sw_show_arch_news = gtk4::Switch::new();
        sw_show_arch_news.set_active(current.show_arch_news);
        sw_show_arch_news.connect_state_set(|_, state| {
            crate::settings::update(|s| s.show_arch_news = state);
            glib::Propagation::Proceed
        });
        style_switch_row(&row_show_arch_news, &sw_show_arch_news);
        row_show_arch_news.add_suffix(&sw_show_arch_news);
        general.add(&row_show_arch_news);

        let news_items_row = ComboRow::new();
        news_items_row.set_title("Arch News Items");
        news_items_row.set_subtitle("How many headlines to show on Overview");
        news_items_row.set_model(Some(&StringList::new(&["3", "5", "8", "10"])));
        news_items_row.set_selected(match current.arch_news_items {
            3 => 0,
            8 => 2,
            10 => 3,
            _ => 1,
        });
        news_items_row.connect_selected_notify(|row| {
            let value = match row.selected() {
                0 => 3,
                2 => 8,
                3 => 10,
                _ => 5,
            };
            crate::settings::update(|s| s.arch_news_items = value);
        });
        style_combo_row(&news_items_row);
        general.add(&news_items_row);

        let row_news_dates = ActionRow::new();
        row_news_dates.set_title("Show Arch News Dates");
        let sw_news_dates = gtk4::Switch::new();
        sw_news_dates.set_active(current.show_arch_news_dates);
        sw_news_dates.connect_state_set(|_, state| {
            crate::settings::update(|s| s.show_arch_news_dates = state);
            glib::Propagation::Proceed
        });
        style_switch_row(&row_news_dates, &sw_news_dates);
        row_news_dates.add_suffix(&sw_news_dates);
        general.add(&row_news_dates);

        let row_links = ActionRow::new();
        row_links.set_title("Open Links in External Browser");
        let sw_links = gtk4::Switch::new();
        sw_links.set_active(current.open_links_in_external_browser);
        sw_links.connect_state_set(|_, state| {
            crate::settings::update(|s| s.open_links_in_external_browser = state);
            glib::Propagation::Proceed
        });
        style_switch_row(&row_links, &sw_links);
        row_links.add_suffix(&sw_links);
        general.add(&row_links);

        let row_ignored = ActionRow::new();
        row_ignored.set_title("Ignored Updates");
        row_ignored.set_subtitle("Comma-separated package names");
        let ignored_entry = Entry::new();
        ignored_entry.set_hexpand(true);
        ignored_entry.set_valign(gtk4::Align::Center);
        ignored_entry.set_text(&current.ignored_updates.join(", "));
        ignored_entry.connect_changed(|entry| {
            let parsed: Vec<String> = entry
                .text()
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            crate::settings::update(|s| s.ignored_updates = parsed);
        });
        row_ignored.add_suffix(&ignored_entry);
        general.add(&row_ignored);
        prefs.add(&general);

        let safety = PreferencesGroup::new();
        safety.set_title("Safety");
        style_group(&safety);

        let row_aur_review = ActionRow::new();
        row_aur_review.set_title("Require AUR PKGBUILD Review");
        let sw_aur_review = gtk4::Switch::new();
        sw_aur_review.set_active(current.aur_pkgbuild_required);
        sw_aur_review.connect_state_set(|_, state| {
            crate::settings::update(|s| s.aur_pkgbuild_required = state);
            glib::Propagation::Proceed
        });
        style_switch_row(&row_aur_review, &sw_aur_review);
        row_aur_review.add_suffix(&sw_aur_review);
        safety.add(&row_aur_review);

        let row_confirm_remove = ActionRow::new();
        row_confirm_remove.set_title("Confirm Remove");
        let sw_confirm_remove = gtk4::Switch::new();
        sw_confirm_remove.set_active(current.confirm_remove);
        sw_confirm_remove.connect_state_set(|_, state| {
            crate::settings::update(|s| s.confirm_remove = state);
            glib::Propagation::Proceed
        });
        style_switch_row(&row_confirm_remove, &sw_confirm_remove);
        row_confirm_remove.add_suffix(&sw_confirm_remove);
        safety.add(&row_confirm_remove);

        let row_confirm_update = ActionRow::new();
        row_confirm_update.set_title("Confirm Update All");
        let sw_confirm_update = gtk4::Switch::new();
        sw_confirm_update.set_active(current.confirm_update_all);
        sw_confirm_update.connect_state_set(|_, state| {
            crate::settings::update(|s| s.confirm_update_all = state);
            glib::Propagation::Proceed
        });
        style_switch_row(&row_confirm_update, &sw_confirm_update);
        row_confirm_update.add_suffix(&sw_confirm_update);
        safety.add(&row_confirm_update);

        let row_confirm_clean = ActionRow::new();
        row_confirm_clean.set_title("Confirm Clean Cache");
        let sw_confirm_clean = gtk4::Switch::new();
        sw_confirm_clean.set_active(current.confirm_clean_cache);
        sw_confirm_clean.connect_state_set(|_, state| {
            crate::settings::update(|s| s.confirm_clean_cache = state);
            glib::Propagation::Proceed
        });
        style_switch_row(&row_confirm_clean, &sw_confirm_clean);
        row_confirm_clean.add_suffix(&sw_confirm_clean);
        safety.add(&row_confirm_clean);

        let row_confirm_orphans = ActionRow::new();
        row_confirm_orphans.set_title("Confirm Remove Orphans");
        let sw_confirm_orphans = gtk4::Switch::new();
        sw_confirm_orphans.set_active(current.confirm_remove_orphans);
        sw_confirm_orphans.connect_state_set(|_, state| {
            crate::settings::update(|s| s.confirm_remove_orphans = state);
            glib::Propagation::Proceed
        });
        style_switch_row(&row_confirm_orphans, &sw_confirm_orphans);
        row_confirm_orphans.add_suffix(&sw_confirm_orphans);
        safety.add(&row_confirm_orphans);

        let row_confirm_batch_install = ActionRow::new();
        row_confirm_batch_install.set_title("Confirm Batch Install");
        let sw_confirm_batch_install = gtk4::Switch::new();
        sw_confirm_batch_install.set_active(current.confirm_batch_install);
        sw_confirm_batch_install.connect_state_set(|_, state| {
            crate::settings::update(|s| s.confirm_batch_install = state);
            glib::Propagation::Proceed
        });
        style_switch_row(&row_confirm_batch_install, &sw_confirm_batch_install);
        row_confirm_batch_install.add_suffix(&sw_confirm_batch_install);
        safety.add(&row_confirm_batch_install);

        let row_confirm_batch_remove = ActionRow::new();
        row_confirm_batch_remove.set_title("Confirm Batch Remove");
        let sw_confirm_batch_remove = gtk4::Switch::new();
        sw_confirm_batch_remove.set_active(current.confirm_batch_remove);
        sw_confirm_batch_remove.connect_state_set(|_, state| {
            crate::settings::update(|s| s.confirm_batch_remove = state);
            glib::Propagation::Proceed
        });
        style_switch_row(&row_confirm_batch_remove, &sw_confirm_batch_remove);
        row_confirm_batch_remove.add_suffix(&sw_confirm_batch_remove);
        safety.add(&row_confirm_batch_remove);

        let row_strict_aur = ActionRow::new();
        row_strict_aur.set_title("Always Confirm AUR Batch Installs");
        let sw_strict_aur = gtk4::Switch::new();
        sw_strict_aur.set_active(current.always_show_pkgbuild_for_aur);
        sw_strict_aur.connect_state_set(|_, state| {
            crate::settings::update(|s| s.always_show_pkgbuild_for_aur = state);
            glib::Propagation::Proceed
        });
        style_switch_row(&row_strict_aur, &sw_strict_aur);
        row_strict_aur.add_suffix(&sw_strict_aur);
        safety.add(&row_strict_aur);
        prefs.add(&safety);

        let behavior = PreferencesGroup::new();
        behavior.set_title("Behavior");
        style_group(&behavior);

        let row_compact = ActionRow::new();
        row_compact.set_title("Compact Mode");
        let sw_compact = gtk4::Switch::new();
        sw_compact.set_active(current.compact_mode);
        let parent_clone = parent_window.clone();
        if current.compact_mode && !parent_clone.has_css_class("compact-mode") {
            parent_clone.add_css_class("compact-mode");
        }
        let parent_weak = parent_clone.downgrade();
        sw_compact.connect_state_set(move |_, state| {
            crate::settings::update(|s| s.compact_mode = state);
            if let Some(win) = parent_weak.upgrade() {
                if state {
                    win.add_css_class("compact-mode");
                } else {
                    win.remove_css_class("compact-mode");
                }
            }
            glib::Propagation::Proceed
        });
        style_switch_row(&row_compact, &sw_compact);
        row_compact.add_suffix(&sw_compact);
        behavior.add(&row_compact);

        let terminal_row = ComboRow::new();
        terminal_row.set_title("Preferred Terminal");
        terminal_row.set_model(Some(&StringList::new(&[
            "Auto",
            "GNOME Terminal",
            "Konsole",
            "XTerm",
            "XFCE4 Terminal",
            "Alacritty",
        ])));
        terminal_row.set_selected(match current.terminal_preference.as_str() {
            "gnome-terminal" => 1,
            "konsole" => 2,
            "xterm" => 3,
            "xfce4-terminal" => 4,
            "alacritty" => 5,
            _ => 0,
        });
        terminal_row.connect_selected_notify(|row| {
            let value = match row.selected() {
                1 => "gnome-terminal",
                2 => "konsole",
                3 => "xterm",
                4 => "xfce4-terminal",
                5 => "alacritty",
                _ => "auto",
            };
            crate::settings::update(|s| s.terminal_preference = value.to_string());
        });
        style_combo_row(&terminal_row);
        behavior.add(&terminal_row);

        let parallel_row = ComboRow::new();
        parallel_row.set_title("Max Parallel Tasks");
        parallel_row.set_model(Some(&StringList::new(&["1", "2", "3", "4"])));
        parallel_row.set_selected(match current.max_parallel_tasks {
            2 => 1,
            3 => 2,
            4 => 3,
            _ => 0,
        });
        parallel_row.connect_selected_notify(|row| {
            let value = match row.selected() {
                1 => 2,
                2 => 3,
                3 => 4,
                _ => 1,
            };
            crate::settings::update(|s| s.max_parallel_tasks = value);
        });
        style_combo_row(&parallel_row);
        behavior.add(&parallel_row);

        let output_limit_row = ComboRow::new();
        output_limit_row.set_title("Task Output Line Limit");
        output_limit_row.set_model(Some(&StringList::new(&["100", "300", "500", "1000"])));
        output_limit_row.set_selected(match current.task_output_lines_limit {
            100 => 0,
            500 => 2,
            1000 => 3,
            _ => 1,
        });
        output_limit_row.connect_selected_notify(|row| {
            let value = match row.selected() {
                0 => 100,
                2 => 500,
                3 => 1000,
                _ => 300,
            };
            crate::settings::update(|s| s.task_output_lines_limit = value);
        });
        style_combo_row(&output_limit_row);
        behavior.add(&output_limit_row);

        let auto_clear_row = ComboRow::new();
        auto_clear_row.set_title("Auto-Clear Completed Tasks");
        auto_clear_row.set_model(Some(&StringList::new(&[
            "Off", "5 min", "15 min", "60 min",
        ])));
        auto_clear_row.set_selected(match current.auto_clear_completed_tasks_minutes {
            5 => 1,
            15 => 2,
            60 => 3,
            _ => 0,
        });
        auto_clear_row.connect_selected_notify(|row| {
            let value = match row.selected() {
                1 => 5,
                2 => 15,
                3 => 60,
                _ => 0,
            };
            crate::settings::update(|s| s.auto_clear_completed_tasks_minutes = value);
        });
        style_combo_row(&auto_clear_row);
        behavior.add(&auto_clear_row);

        let row_single_click = ActionRow::new();
        row_single_click.set_title("Open Details on Single Click");
        let sw_single_click = gtk4::Switch::new();
        sw_single_click.set_active(current.show_package_details_on_single_click);
        sw_single_click.connect_state_set(|_, state| {
            crate::settings::update(|s| s.show_package_details_on_single_click = state);
            glib::Propagation::Proceed
        });
        style_switch_row(&row_single_click, &sw_single_click);
        row_single_click.add_suffix(&sw_single_click);
        behavior.add(&row_single_click);

        let row_sizes = ActionRow::new();
        row_sizes.set_title("Show Package Sizes in Lists");
        let sw_sizes = gtk4::Switch::new();
        sw_sizes.set_active(current.show_package_sizes_in_lists);
        sw_sizes.connect_state_set(|_, state| {
            crate::settings::update(|s| s.show_package_sizes_in_lists = state);
            glib::Propagation::Proceed
        });
        style_switch_row(&row_sizes, &sw_sizes);
        row_sizes.add_suffix(&sw_sizes);
        behavior.add(&row_sizes);
        prefs.add(&behavior);

        let notifications = PreferencesGroup::new();
        notifications.set_title("Task Notifications");
        style_group(&notifications);

        let row_task_done = ActionRow::new();
        row_task_done.set_title("Notify on Task Completion");
        let sw_task_done = gtk4::Switch::new();
        sw_task_done.set_active(current.notify_on_task_complete);
        sw_task_done.connect_state_set(|_, state| {
            crate::settings::update(|s| s.notify_on_task_complete = state);
            glib::Propagation::Proceed
        });
        style_switch_row(&row_task_done, &sw_task_done);
        row_task_done.add_suffix(&sw_task_done);
        notifications.add(&row_task_done);

        let row_task_failed = ActionRow::new();
        row_task_failed.set_title("Notify on Task Failure");
        let sw_task_failed = gtk4::Switch::new();
        sw_task_failed.set_active(current.notify_on_task_failed);
        sw_task_failed.connect_state_set(|_, state| {
            crate::settings::update(|s| s.notify_on_task_failed = state);
            glib::Propagation::Proceed
        });
        style_switch_row(&row_task_failed, &sw_task_failed);
        row_task_failed.add_suffix(&sw_task_failed);
        notifications.add(&row_task_failed);
        prefs.add(&notifications);

        let appearance = PreferencesGroup::new();
        appearance.set_title("Appearance");
        style_group(&appearance);
        let theme_row = ComboRow::new();
        theme_row.set_title("Color Scheme");
        theme_row.set_model(Some(&StringList::new(&["System Default", "Light", "Dark"])));
        theme_row.set_selected(match current.theme.as_str() {
            "light" => 1,
            "dark" => 2,
            _ => 0,
        });
        theme_row.connect_selected_notify(move |row| {
            let style_manager = StyleManager::default();
            match row.selected() {
                1 => {
                    crate::settings::update(|s| s.theme = "light".to_string());
                    style_manager.set_color_scheme(adw::ColorScheme::ForceLight);
                }
                2 => {
                    crate::settings::update(|s| s.theme = "dark".to_string());
                    style_manager.set_color_scheme(adw::ColorScheme::ForceDark);
                }
                _ => {
                    crate::settings::update(|s| s.theme = "default".to_string());
                    style_manager.set_color_scheme(adw::ColorScheme::Default);
                }
            }
        });
        style_combo_row(&theme_row);
        appearance.add(&theme_row);
        prefs.add(&appearance);

        let logging = PreferencesGroup::new();
        logging.set_title("Logging");
        style_group(&logging);

        let level_row = ComboRow::new();
        level_row.set_title("Log Level");
        level_row.set_model(Some(&StringList::new(&["Error", "Warn", "Info", "Debug"])));
        level_row.set_selected(match current.log_level.as_str() {
            "error" => 0,
            "warn" => 1,
            "debug" => 3,
            _ => 2,
        });
        level_row.connect_selected_notify(|row| {
            let value = match row.selected() {
                0 => "error",
                1 => "warn",
                3 => "debug",
                _ => "info",
            };
            crate::settings::update(|s| s.log_level = value.to_string());
        });
        style_combo_row(&level_row);
        logging.add(&level_row);

        let max_log_row = ComboRow::new();
        max_log_row.set_title("Max Log Size (MB)");
        max_log_row.set_model(Some(&StringList::new(&["5", "10", "25", "50"])));
        max_log_row.set_selected(match current.max_log_size_mb {
            5 => 0,
            25 => 2,
            50 => 3,
            _ => 1,
        });
        max_log_row.connect_selected_notify(|row| {
            let value = match row.selected() {
                0 => 5,
                2 => 25,
                3 => 50,
                _ => 10,
            };
            crate::settings::update(|s| s.max_log_size_mb = value);
        });
        style_combo_row(&max_log_row);
        logging.add(&max_log_row);
        prefs.add(&logging);

        vbox.append(&prefs);
        window.set_child(Some(&vbox));
        window.present();
    }
}
