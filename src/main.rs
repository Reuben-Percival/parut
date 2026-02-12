use adw::prelude::*;
use adw::{Application, ApplicationWindow, StyleManager};
use gtk4::{CssProvider, gdk, glib};

mod logger;
mod paru;
mod settings;
mod task_queue;
mod ui;
mod utils;

use logger::log_info;
use ui::ParuGui;

const APP_ID: &str = "io.github.reubenpercival.parut";
const CSS: &str = include_str!("style.css");

fn main() -> glib::ExitCode {
    // Log application start
    log_info("Parut application starting");

    // Initialize settings
    settings::init();

    let app = Application::builder().application_id(APP_ID).build();

    app.connect_startup(|_| {
        // Use system color scheme
        let style_manager = StyleManager::default();
        match crate::settings::get().theme.as_str() {
            "light" => style_manager.set_color_scheme(adw::ColorScheme::ForceLight),
            "dark" => style_manager.set_color_scheme(adw::ColorScheme::ForceDark),
            _ => style_manager.set_color_scheme(adw::ColorScheme::Default),
        }

        // Load custom CSS
        load_css();
    });

    app.connect_activate(build_ui);

    let exit_code = app.run();

    // Log application exit
    log_info(&format!(
        "Parut application exiting with code: {:?}",
        exit_code
    ));

    exit_code
}

fn load_css() {
    let Some(display) = gdk::Display::default() else {
        log_info("No display available, skipping CSS provider registration");
        return;
    };

    let provider = CssProvider::new();
    provider.load_from_data(CSS);

    gtk4::style_context_add_provider_for_display(
        &display,
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
    if crate::settings::get().compact_mode {
        window.add_css_class("compact-mode");
    }

    let gui = ParuGui::new();
    window.set_content(Some(gui.main_widget()));

    window.present();

    log_info("UI presented successfully");
}
