// Replace the create_search_view() function in ui.rs with this optimized version

fn create_search_view(task_queue: Arc<TaskQueue>) -> (Box, Rc<RefCell<Vec<Package>>>, Rc<RefCell<ListBox>>) {
    let vbox = Box::new(Orientation::Vertical, 12);
    vbox.set_margin_start(12);
    vbox.set_margin_end(12);
    vbox.set_margin_top(12);
    vbox.set_margin_bottom(12);

    // Search entry
    let search_entry = SearchEntry::new();
    search_entry.set_placeholder_text(Some("Search packages..."));
    vbox.append(&search_entry);

    // Status label for search feedback
    let status_label = Label::new(Some(""));
    status_label.add_css_class("dim-label");
    status_label.set_halign(gtk4::Align::Start);
    vbox.append(&status_label);
    let status_label_rc = Rc::new(RefCell::new(status_label));

    // Results list
    let scrolled = ScrolledWindow::new();
    scrolled.set_vexpand(true);

    let list_box = ListBox::new();
    list_box.add_css_class("boxed-list");
    scrolled.set_child(Some(&list_box));
    vbox.append(&scrolled);

    let packages = Rc::new(RefCell::new(Vec::new()));
    let list_box_rc = Rc::new(RefCell::new(list_box));

    // Debounce timer
    let search_timeout_id: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));

    // Search handler with debouncing
    let list_box_clone = list_box_rc.clone();
    let packages_clone = packages.clone();
    let task_queue_clone = task_queue.clone();
    let status_label_clone = status_label_rc.clone();
    let timeout_id_clone = search_timeout_id.clone();
    
    search_entry.connect_search_changed(move |entry| {
        let query = entry.text().to_string();
        
        // Cancel previous timeout if exists
        if let Some(id) = timeout_id_clone.borrow_mut().take() {
            id.remove();
        }
        
        if query.is_empty() {
            status_label_clone.borrow().set_text("");
            let list_box = list_box_clone.borrow();
            while let Some(child) = list_box.first_child() {
                list_box.remove(&child);
            }
            return;
        }

        status_label_clone.borrow().set_text("Searching...");

        // Set up debounced search (300ms delay)
        let list_box = list_box_clone.clone();
        let packages = packages_clone.clone();
        let task_queue = task_queue_clone.clone();
        let status_label = status_label_clone.clone();
        
        let timeout_id = glib::timeout_add_local(std::time::Duration::from_millis(300), move || {
            let query_owned = query.clone();
            let list_box = list_box.clone();
            let packages = packages.clone();
            let task_queue = task_queue.clone();
            let status_label = status_label.clone();
            
            glib::spawn_future_local(async move {
                match ParuBackend::search_packages(&query_owned) {
                    Ok(results) => {
                        let count = results.len();
                        *packages.borrow_mut() = results.clone();
                        
                        // Get installed packages for comparison
                        let installed_names = Self::get_installed_package_names();
                        
                        Self::update_search_results(&list_box.borrow(), &results, &installed_names, task_queue);
                        
                        status_label.borrow().set_text(&format!("Found {} packages", count));
                    }
                    Err(e) => {
                        eprintln!("Search error: {}", e);
                        status_label.borrow().set_text("Search failed");
                    }
                }
            });
            
            glib::ControlFlow::Break
        });
        
        *timeout_id_clone.borrow_mut() = Some(timeout_id);
    });

    (vbox, packages, list_box_rc)
}
