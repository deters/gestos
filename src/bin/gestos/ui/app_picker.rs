//! Installed apps, as gestures name them: their names and icons, the dialog that picks one, and
//! what keeps a gesture for one app from running.

use gtk::prelude::*;
use gtk::gio;
use gtk4 as gtk;
use crate::extension::ExtensionStatus;

// The size of app icons in the dialog that picks one.
const ICON_SIZE: i32 = 32;

/// An installed app, by the desktop file ID gestures use for it, like `org.mozilla.firefox`.
fn app_info(id: &str) -> Option<gio::DesktopAppInfo> {
    gio::DesktopAppInfo::new(&format!("{id}.desktop"))
}

/// The app's name, or its ID for one that isn't installed.
pub fn app_name(id: &str) -> String {
    gestos::key_steps::app_name(id)
}

/// The app's icon at a size, or a generic one for an app that isn't installed. A program's is
/// its own, or a generic one, from the icon theme.
pub fn app_icon(id: &str, size: i32) -> gtk::Image {
    let image = if let Some(program) = gestos::programs::find(id) {
        gtk::Image::from_icon_name(program.icon.as_deref().unwrap_or("application-x-executable-symbolic"))
    } else {
        match app_info(id).and_then(|info| info.icon()) {
            Some(icon) => gtk::Image::from_gicon(&icon),
            None => gtk::Image::from_icon_name("application-x-executable"),
        }
    };
    image.set_pixel_size(size);
    image
}

/// The app an advanced action starts by opening, whose icon it shows, as the steps after go to it.
pub fn first_app(action: &gestos::config::ActionType) -> Option<&str> {
    match action {
        gestos::config::ActionType::Keyboard(steps) => match steps.first() {
            Some(gestos::key_steps::KeyStep::Open(app)) => Some(app),
            _ => None,
        },
        _ => None,
    }
}

/// The app an action only opens, which shows with that app's icon.
pub fn opened_app(action: &gestos::config::ActionType) -> Option<&str> {
    match action {
        gestos::config::ActionType::Keyboard(steps) => match steps.as_slice() {
            [gestos::key_steps::KeyStep::Open(app)] => Some(app),
            _ => None,
        },
        _ => None,
    }
}

/// The programs turned on, then the installed apps that show in the app grid, each by name, as
/// gestures name them.
fn installed_apps() -> Vec<String> {
    let mut programs = gestos::programs::turned_on();
    programs.sort_by_key(|program| program.name.to_lowercase());
    programs
        .iter()
        .map(gestos::programs::Program::app_id)
        .chain(gestos::key_steps::installed_apps().into_iter().map(|(_, id)| id))
        .collect()
}

/// What keeps a gesture limited to some apps from running, if anything. The daemon knows the app
/// in use from GNOME's extension, or, without GNOME, where the desktop can say: `elsewhere`.
pub fn apps_note(status: &ExtensionStatus, elsewhere: bool) -> Option<&'static str> {
    match status {
        ExtensionStatus::Running => None,
        ExtensionStatus::Unavailable if elsewhere => None,
        ExtensionStatus::Unavailable => {
            Some("Only works on GNOME, KDE, Sway, Hyprland and X11: here, a gesture limited to apps doesn't run")
        }
        // Named as GNOME's Extensions app lists it, so it can be found there.
        ExtensionStatus::Outdated(_) => {
            Some("Needs the \u{201c}Gestos\u{201d} GNOME extension to be updated")
        }
        _ => Some("Needs the \u{201c}Gestos\u{201d} GNOME extension, which tells the app in use"),
    }
}

/// A dialog to pick one of the installed apps, with a search, and with `custom_apps`, those turned
/// on in Custom Apps too, which can't be opened. Picking one closes it.
pub fn open_app_picker(
    parent: &gtk::Window,
    taken: &[String],
    custom_apps: bool,
    picked: impl Fn(String) + 'static,
) {
    let dialog = gtk::Window::new();
    dialog.set_transient_for(Some(parent));
    dialog.set_modal(true);
    dialog.set_title(Some("Add App"));
    dialog.set_default_size(360, 480);

    let header = gtk::HeaderBar::new();
    header.set_show_title_buttons(false);
    let title = gtk::Label::new(Some("Add App"));
    title.add_css_class("title");
    header.set_title_widget(Some(&title));
    let cancel = gtk::Button::with_mnemonic("_Cancel");
    let dialog_cancel = dialog.clone();
    cancel.connect_clicked(move |_| dialog_cancel.destroy());
    header.pack_start(&cancel);
    dialog.set_titlebar(Some(&header));

    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    content.add_css_class("dialog-content");
    content.set_margin_start(12);
    content.set_margin_end(12);
    content.set_margin_top(12);
    content.set_margin_bottom(12);

    let search = gtk::SearchEntry::new();
    search.set_key_capture_widget(Some(&dialog));
    content.append(&search);

    let list = gtk::ListBox::new();
    list.add_css_class("boxed-list");
    list.set_selection_mode(gtk::SelectionMode::None);
    list.set_valign(gtk::Align::Start);
    let mut ids = installed_apps();
    if custom_apps {
        ids.extend(gestos::programs::list().into_iter().filter(|listed| listed.on).map(|listed| listed.program.app_id()));
        ids.sort_by_cached_key(|id| app_name(id).to_lowercase());
    }
    let ids: Vec<String> = ids.into_iter().filter(|id| !taken.contains(id)).collect();
    for id in &ids {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        row.add_css_class("settings-row");
        row.append(&app_icon(id, ICON_SIZE));
        let name = gtk::Label::new(Some(&app_name(id)));
        name.set_halign(gtk::Align::Start);
        row.append(&name);
        list.append(&row);
    }

    // Matched by name and by ID, as some apps are known by either.
    let names: Vec<String> = ids.iter().map(|id| app_name(id).to_lowercase()).collect();
    let (filter_ids, filter_search) = (ids.clone(), search.clone());
    list.set_filter_func(move |row| {
        let query = filter_search.text().to_lowercase();
        let i = row.index() as usize;
        query.is_empty() || names[i].contains(query.trim()) || filter_ids[i].contains(query.trim())
    });
    let list_search = list.clone();
    search.connect_search_changed(move |_| list_search.invalidate_filter());

    let dialog_pick = dialog.clone();
    list.connect_row_activated(move |_, row| {
        picked(ids[row.index() as usize].clone());
        dialog_pick.destroy();
    });
    // Enter in the search picks the first app it shows.
    let list_enter = list.clone();
    search.connect_activate(move |_| {
        let mut row = list_enter.first_child();
        while let Some(widget) = row {
            if let Ok(list_row) = widget.clone().downcast::<gtk::ListBoxRow>() {
                if list_row.is_child_visible() {
                    list_row.emit_activate();
                    return;
                }
            }
            row = widget.next_sibling();
        }
    });

    let scrolled = gtk::ScrolledWindow::new();
    scrolled.set_hscrollbar_policy(gtk::PolicyType::Never);
    scrolled.set_vexpand(true);
    scrolled.set_child(Some(&list));
    content.append(&scrolled);
    dialog.set_child(Some(&content));

    crate::ui::components::close_with_keys(&dialog);
    dialog.present();
    search.grab_focus();
}

/// The apps' names, as text says them: "Files", "Files and Ptyxis", "Files, Ptyxis and Text
/// Editor".
pub fn apps_named(apps: &[String]) -> String {
    let names: Vec<String> = apps.iter().map(|app| app_name(app)).collect();
    match names.split_last() {
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extension::OldCopy;

    #[test]
    fn apps_need_no_note_only_with_the_extension_running() {
        assert_eq!(apps_note(&ExtensionStatus::Running, false), None);
        assert_eq!(apps_note(&ExtensionStatus::Unavailable, true), None);
        assert!(apps_note(&ExtensionStatus::Unavailable, false).unwrap().contains("Only works on"));
        assert!(apps_note(&ExtensionStatus::Off, false).unwrap().contains("GNOME extension"));
        assert!(apps_note(&ExtensionStatus::Outdated(OldCopy::Replaced), false)
            .unwrap()
            .contains("updated"));
    }

    #[test]
    fn an_advanced_action_shows_the_app_it_opens_first() {
        use gestos::config::ActionType;
        use gestos::key_steps::KeyStep;
        let steps = |steps: Vec<KeyStep>| ActionType::Keyboard(steps);
        let open = KeyStep::Open("org.gnome.Nautilus.desktop".into());
        let press = KeyStep::Press("Control+L".into());
        assert_eq!(first_app(&steps(vec![open.clone(), press.clone()])), Some("org.gnome.Nautilus.desktop"));
        assert_eq!(first_app(&steps(vec![press, open])), None);
        assert_eq!(first_app(&ActionType::Kill), None);
    }
}
