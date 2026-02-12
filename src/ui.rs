use crate::logger::{log_error, log_info};
use crate::paru::{NewsItem, Package, ParuBackend};
use crate::task_queue::{TaskQueue, TaskStatus, TaskType, TaskWorker};
use adw::prelude::*;
use adw::{
    ActionRow, ComboRow, HeaderBar, PreferencesGroup, PreferencesPage, StatusPage, StyleManager,
    ViewStack, ViewSwitcher,
};
use gtk4::{
    Box, Button, DropDown, Image, Label, ListBox, Orientation, ProgressBar, ScrolledWindow,
    SearchEntry, Separator, Spinner, StringList, TextView, Window, glib,
};
use std::cell::RefCell;
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
            );
            Self::refresh_updates(
                &updates_list,
                &updates_clone,
                task_queue_for_refresh.clone(),
            );
            Self::update_refresh_time(&refresh_label_clone, &refresh_timer_clone);

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
        Self::refresh_installed(
            &installed_view.2,
            &gui.installed_packages,
            gui.task_queue.clone(),
        );

        if crate::settings::get().check_updates_on_startup {
            Self::refresh_updates(&updates_view.2, &gui.updates, gui.task_queue.clone());
        }

        Self::refresh_dashboard_stats(&dash_label_0_init, &dash_label_1_init, &dash_label_2_init);
        Self::refresh_arch_news(&dash_news_list_init, &dash_news_status_init);
        Self::update_refresh_time(&gui.last_refresh_label, &gui.refresh_timer_id);
        Self::setup_auto_refresh(
            &installed_view.2,
            &updates_view.2,
            &gui.installed_packages,
            &gui.updates,
            gui.task_queue.clone(),
            &dash_label_0_init,
            &dash_label_1_init,
            &dash_label_2_init,
            &dash_news_list_init,
            &dash_news_status_init,
            &gui.last_refresh_label,
            &gui.refresh_timer_id,
            &gui.auto_refresh_timer_id,
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
        match crate::settings::get().show_only_updates_from.as_str() {
            "repo-only" => packages
                .into_iter()
                .filter(|p| p.repository != "aur")
                .collect(),
            "aur-only" => packages.into_iter().filter(|p| p.repository == "aur").collect(),
            _ => packages,
        }
    }

    fn setup_auto_refresh(
        installed_list: &Rc<RefCell<ListBox>>,
        updates_list: &Rc<RefCell<ListBox>>,
        installed_packages: &Rc<RefCell<Vec<Package>>>,
        updates: &Rc<RefCell<Vec<Package>>>,
        task_queue: Arc<TaskQueue>,
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
        let dash_installed = dash_installed.clone();
        let dash_updates = dash_updates.clone();
        let dash_aur = dash_aur.clone();
        let dash_news_list = dash_news_list.clone();
        let dash_news_status = dash_news_status.clone();
        let refresh_label = refresh_label.clone();
        let refresh_timer = refresh_timer.clone();

        let id = glib::timeout_add_seconds_local(interval_secs, move || {
            Self::refresh_installed(&installed_list, &installed_packages, task_queue_cloned.clone());
            Self::refresh_updates(&updates_list, &updates, task_queue_cloned.clone());
            Self::refresh_dashboard_stats(&dash_installed, &dash_updates, &dash_aur);
            Self::refresh_arch_news(&dash_news_list, &dash_news_status);
            Self::update_refresh_time(&refresh_label, &refresh_timer);
            glib::ControlFlow::Continue
        });
        *auto_refresh_timer.borrow_mut() = Some(id);
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
            "Clean Cache",
            "user-trash-symbolic",
            "Clean package cache to free disk space",
        );
        let tq_clean = task_queue.clone();
        clean_btn.connect_clicked(move |btn| {
            let needs_confirm =
                crate::settings::get().confirm_clean_cache || crate::settings::get().confirm_actions;
            if needs_confirm {
                if let Some(window) = btn.root().and_then(|w| w.downcast::<gtk4::Window>().ok()) {
                    let tq_confirm = tq_clean.clone();
                    Self::show_confirmation_dialog(
                        &window,
                        "Confirm Cache Cleanup",
                        "Clean package cache now?",
                        move || {
                            log_info("Starting cache cleanup from dashboard");
                            tq_confirm.add_task(TaskType::CleanCache, "system".to_string());
                        },
                    );
                }
            } else {
                log_info("Starting cache cleanup from dashboard");
                tq_clean.add_task(TaskType::CleanCache, "system".to_string());
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
                let updates = ParuBackend::list_updates().ok().map(Self::filter_updates_by_source);
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

            let link_btn = gtk4::LinkButton::builder()
                .uri(&item.link)
                .label(&item.title)
                .build();
            link_btn.set_halign(gtk4::Align::Start);
            link_btn.add_css_class("flat");
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
            news_status.borrow().set_text("Arch News is disabled in settings.");
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
                        ("URL", &details.url),
                        ("Licenses", &details.licenses),
                        ("Repository", &details.groups), // Just re-using groups/repo if available, logic might need tuning
                        ("Size", &details.installed_size),
                        ("Packager", &details.packager),
                        ("Build Date", &details.build_date),
                        ("Install Date", &details.install_date),
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

                    // Dependencies section
                    if !details.depends_on.is_empty() || !details.required_by.is_empty() {
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
        let hint_label = Label::new(Some(
            &format!("Start typing to search. Results limited to {} packages.", search_limit),
        ));
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
        let sort_dropdown_clone = sort_dropdown.clone();

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
                hint_clone
                    .borrow()
                    .set_text(&format!(
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

            let list_box = list_box_clone.clone();
            let packages = packages_clone.clone();
            let task_queue = task_queue_clone.clone();
            let loading = loading_clone.clone();
            let hint = hint_clone.clone();
            let sort_dropdown = sort_dropdown_clone.clone();

            let timeout_id_inner = timeout_id_clone.clone();

            // Debounce: wait 300ms before searching
            let id = glib::timeout_add_local_once(Duration::from_millis(300), move || {
                // Clear the ID as it is executing so we don't try to remove it later
                let _ = timeout_id_inner.borrow_mut().take();

                Self::run_blocking(
                    move || {
                        let limit = crate::settings::get().search_result_limit;
                        ParuBackend::search_packages(&query, Some(limit))
                    },
                    move |result| match result {
                        Ok(results) => {
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
                            Self::update_package_list(
                                &list_box.borrow(),
                                &sorted,
                                true,
                                task_queue,
                            );
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

        let list_box_clone2 = list_box_rc.clone();
        let packages_clone2 = packages.clone();
        let task_queue_clone2 = task_queue.clone();
        sort_dropdown.connect_selected_notify(move |dd| {
            crate::settings::update(|s| s.default_sort_search = dd.selected().min(2));
            let sorted = Self::filter_and_sort_packages(&packages_clone2.borrow(), "", dd.selected());
            Self::update_package_list(&list_box_clone2.borrow(), &sorted, true, task_queue_clone2.clone());
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

        vbox.append(&controls_box);

        let scrolled = ScrolledWindow::new();
        scrolled.set_vexpand(true);

        let list_box = ListBox::new();
        list_box.add_css_class("boxed-list");
        scrolled.set_child(Some(&list_box));
        vbox.append(&scrolled);

        let packages = Rc::new(RefCell::new(Vec::new()));
        let list_box_rc = Rc::new(RefCell::new(list_box));
        let search_entry_rc = Rc::new(RefCell::new(search_entry.clone()));

        // Handlers
        let list_box_clone = list_box_rc.clone();
        let packages_clone = packages.clone();
        let task_queue_clone = task_queue.clone();
        let count_clone = count_label_rc.clone();
        let search_entry_clone = search_entry.clone();
        let sort_dropdown_clone = sort_dropdown.clone();

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
            Self::update_package_list_with_remove(
                &list_box_clone.borrow(),
                &filtered,
                task_queue_clone.clone(),
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

        (vbox, packages, list_box_rc, search_entry_rc)
    }

    fn create_updates_view(
        task_queue: Arc<TaskQueue>,
    ) -> (Box, Rc<RefCell<Vec<Package>>>, Rc<RefCell<ListBox>>) {
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
        vbox.append(&scrolled);

        let packages = Rc::new(RefCell::new(Vec::new()));
        let list_box_rc = Rc::new(RefCell::new(list_box));

        (vbox, packages, list_box_rc)
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
                    if let Some(window) = info.root().and_then(|w| w.downcast::<gtk4::Window>().ok())
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
                    let needs_confirm =
                        crate::settings::get().confirm_remove || crate::settings::get().confirm_actions;
                    if needs_confirm {
                        if let Some(row_box) = row_box_weak.upgrade() {
                            if let Some(window) =
                                row_box.root().and_then(|w| w.downcast::<gtk4::Window>().ok())
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

        if crate::settings::get().show_package_details_on_single_click {
            let pkg_name_click = package.name.clone();
            let info_box_weak = info_box.downgrade();
            let click = gtk4::GestureClick::new();
            click.set_button(1);
            click.connect_released(move |_, _, _, _| {
                if let Some(info) = info_box_weak.upgrade() {
                    if let Some(window) = info.root().and_then(|w| w.downcast::<gtk4::Window>().ok())
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
                    if let Some(window) = row_box.root().and_then(|w| w.downcast::<gtk4::Window>().ok())
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
    ) {
        let list_box = list_box.clone();
        let packages = packages.clone();

        Self::run_blocking(
            move || ParuBackend::list_installed(),
            move |result| match result {
                Ok(pkgs) => {
                    *packages.borrow_mut() = pkgs.clone();
                    Self::update_package_list_with_remove(&list_box.borrow(), &pkgs, task_queue);
                }
                Err(e) => {
                    log_error(&format!("Error loading installed packages: {}", e));
                }
            },
        );
    }

    fn refresh_updates(
        list_box: &Rc<RefCell<ListBox>>,
        packages: &Rc<RefCell<Vec<Package>>>,
        task_queue: Arc<TaskQueue>,
    ) {
        let list_box = list_box.clone();
        let packages = packages.clone();

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
                    *packages.borrow_mut() = pkgs.clone();
                    Self::update_package_list(&list_box.borrow(), &pkgs, false, task_queue);
                }
                Err(e) => {
                    log_error(&format!("Error loading updates: {}", e));
                }
            },
        );
    }

    fn update_refresh_time(
        label: &Rc<RefCell<Label>>,
        timer_id: &Rc<RefCell<Option<glib::SourceId>>>,
    ) {
        use std::time::{SystemTime, UNIX_EPOCH};

        if let Some(existing_id) = timer_id.borrow_mut().take() {
            existing_id.remove();
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let label_clone = label.clone();

        // Update immediately
        label.borrow().set_text("Refreshed just now");

        // Update every 30 seconds to show elapsed time
        let id = glib::timeout_add_seconds_local(30, move || {
            let elapsed = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(now)
                .saturating_sub(now);

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
        *timer_id.borrow_mut() = Some(id);
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
        auto_row.set_model(Some(&StringList::new(&["Off", "15 min", "30 min", "1 hour", "6 hours"])));
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
