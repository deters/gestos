mod state;
mod suggestions;
mod actions;
mod daemon;
mod debug_info;
mod extension;
mod system;
mod ui;

use gtk::prelude::*;
use gtk::glib;
use gtk4 as gtk;
use ui::main_window::build_ui;
use ui::overlay::build_overlay_ui;

/// Run by the daemon when gestures are too similar to tell apart. It reads the options from
/// standard input and prints the one picked.
fn run_gesture_chooser() {
    let mut input = String::new();
    if std::io::Read::read_to_string(&mut std::io::stdin(), &mut input).is_err() {
        std::process::exit(2);
    }
    let entries = gestos::chooser::decode_entries(&input);
    if entries.is_empty() {
        std::process::exit(2);
    }

    let app = gtk::Application::builder()
        .application_id("io.github.deters.Gestos.Chooser")
        .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.connect_activate(move |app| ui::gesture_chooser::build_chooser_ui(app, &entries));
    app.run_with_args::<glib::GString>(&[]);
}

fn main() {
    gestos::config::migrate_legacy_user_files(
        &gestos::config::config_home(),
        gestos::config::autostart_dir().as_deref(),
    );

    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|arg| arg == "--debug-info") {
        print!("{}", debug_info::report());
        return;
    }
    if args.iter().any(|arg| arg == "--start") {
        std::process::exit(daemon::run_command(true));
    }
    if args.iter().any(|arg| arg == "--stop") {
        std::process::exit(daemon::run_command(false));
    }

    if args.iter().any(|arg| arg == "--choose-gesture") {
        run_gesture_chooser();
        return;
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
