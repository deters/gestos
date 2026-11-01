mod state;
mod suggestions;
mod actions;
mod daemon;
mod system;
mod ui;

use gtk::prelude::*;
use gtk::glib;
use gtk4 as gtk;
use ui::main_window::build_ui;
use ui::overlay::build_overlay_ui;

fn main() {
    gestos::config::migrate_legacy_user_files(
        &gestos::config::config_home(),
        gestos::config::autostart_dir().as_deref(),
    );

    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|arg| arg == "--start") {
        std::process::exit(daemon::run_command(true));
    }
    if args.iter().any(|arg| arg == "--stop") {
        std::process::exit(daemon::run_command(false));
    }

    let is_overlay = args.iter().any(|arg| arg == "--overlay");

    let app = gtk::Application::builder()
        .application_id(if is_overlay {
            "io.github.deters.Gestos.Overlay"
        } else {
            "io.github.deters.Gestos"
        })
        .build();

    if is_overlay {
        app.connect_activate(build_overlay_ui);
    } else {
        app.connect_activate(build_ui);
    }
    app.run_with_args::<glib::GString>(&[]);
}
