use gtk4::prelude::*;
use gtk4::{glib, gdk, CssProvider};
use adw::prelude::*;
use adw::{Application, ApplicationWindow, StyleManager};

mod ui;
mod paru;
mod task_queue;
mod logger;

use ui::ParuGui;
use logger::log_info;

const APP_ID: &str = "com.example.Parut";
const CSS: &str = include_str!("style.css");

fn main() -> glib::ExitCode {
    // Log application start
    log_info("Parut application starting");
    
    let app = Application::builder()
        .application_id(APP_ID)
        .build();

    app.connect_startup(|_| {
        // Use system color scheme
        let _style_manager = StyleManager::default();

        
        // Load custom CSS
        load_css();
    });

    app.connect_activate(build_ui);
    
    let exit_code = app.run();
    
    // Log application exit
    log_info(&format!("Parut application exiting with code: {:?}", exit_code));
    
    exit_code
}

fn load_css() {
    let provider = CssProvider::new();
    provider.load_from_data(CSS);
    
    gtk4::style_context_add_provider_for_display(
        &gdk::Display::default().expect("Could not get default display"),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    
    log_info("Custom CSS loaded successfully");
}

fn build_ui(app: &Application) {
    log_info("Building UI");
    
    let window = ApplicationWindow::builder()
        .application(app)
        .title("Parut")
        .default_width(1200)
        .default_height(800)
        .build();

    // Add window CSS class for custom styling
    window.add_css_class("main-window");

    let gui = ParuGui::new();
    window.set_content(Some(gui.main_widget()));

    window.present();
    
    log_info("UI presented successfully");
}
