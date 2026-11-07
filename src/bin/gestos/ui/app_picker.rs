//! The apps a gesture works in, as a list in its trigger: one row for each app, with a button to
//! remove it, and a row to add one from the installed apps. With no apps, it works in all of them.

use gtk::prelude::*;
use gtk::{gio, glib};
use gtk4 as gtk;
use std::cell::RefCell;
use std::rc::Rc;

const ICON_SIZE: i32 = 32;
const LIST_ICON_SIZE: i32 = 16;

/// An installed app, by the desktop file ID gestures use for it, like `org.mozilla.firefox`.
fn app_info(id: &str) -> Option<gio::DesktopAppInfo> {
    gio::DesktopAppInfo::new(&format!("{id}.desktop"))
}

/// The app's name, or its ID for one that isn't installed.
pub fn app_name(id: &str) -> String {
    app_info(id).map_or_else(|| id.to_string(), |info| info.display_name().to_string())
}

/// The app's icon at a size, or a generic one for an app that isn't installed.
pub fn app_icon(id: &str, size: i32) -> gtk::Image {
    let image = match app_info(id).and_then(|info| info.icon()) {
        Some(icon) => gtk::Image::from_gicon(&icon),
        None => gtk::Image::from_icon_name("application-x-executable"),
    };
    image.set_pixel_size(size);
    image
}

/// The installed apps that show in the app grid, by name, as their IDs.
fn installed_apps() -> Vec<String> {
    let mut apps: Vec<(String, String)> = gio::AppInfo::all()
        .into_iter()
        .filter(|info| info.should_show())
        .filter_map(|info| {
            let id = info.id()?.trim_end_matches(".desktop").to_string();
            Some((info.display_name().to_lowercase(), id))
        })
        .collect();
    apps.sort();
    apps.dedup_by(|a, b| a.1 == b.1);
    apps.into_iter().map(|(_, id)| id).collect()
}

/// The list of apps, kept in `apps`. `changed` is called after the user adds or removes one.
pub fn build_apps_list(
    parent: &gtk::Window,
    apps: Rc<RefCell<Vec<String>>>,
    changed: Rc<dyn Fn()>,
) -> gtk::ListBox {
    let list = gtk::ListBox::new();
    list.add_css_class("boxed-list");
    list.set_selection_mode(gtk::SelectionMode::None);
    fill(&list, parent, &apps, &changed);
    // The add row is the last one, and the only one that can be activated.
    let parent = parent.clone();
    list.connect_row_activated(move |list, _| {
        let (list, picker_parent) = (list.clone(), parent.clone());
        let (apps, changed) = (Rc::clone(&apps), Rc::clone(&changed));
        let taken = apps.borrow().clone();
        open_app_picker(&parent, &taken, move |id| {
            apps.borrow_mut().push(id);
            fill(&list, &picker_parent, &apps, &changed);
            changed();
        });
    });
    list
}

fn fill(
    list: &gtk::ListBox,
    parent: &gtk::Window,
    apps: &Rc<RefCell<Vec<String>>>,
    changed: &Rc<dyn Fn()>,
) {
    while let Some(row) = list.first_child() {
        list.remove(&row);
    }

    let title_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    title_row.add_css_class("settings-row");
    let title = gtk::Label::new(Some("Apps"));
    title.set_halign(gtk::Align::Start);
    title.set_hexpand(true);
    title_row.append(&title);
    if apps.borrow().is_empty() {
        let all = gtk::Label::new(Some("All Apps"));
        all.add_css_class("dim-label");
        title_row.append(&all);
    }
    list.append(&title_row);
    if let Some(row) = title_row.parent().and_downcast::<gtk::ListBoxRow>() {
        row.set_activatable(false);
    }

    for (i, id) in apps.borrow().iter().enumerate() {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        row.add_css_class("settings-row");
        row.append(&app_icon(id, ICON_SIZE));
        let name = gtk::Label::new(Some(&app_name(id)));
        name.set_halign(gtk::Align::Start);
        name.set_hexpand(true);
        row.append(&name);
        let remove = gtk::Button::from_icon_name("user-trash-symbolic");
        remove.set_tooltip_text(Some("Remove"));
        remove.add_css_class("flat");
        remove.set_valign(gtk::Align::Center);
        let (list_remove, parent_remove) = (list.clone(), parent.clone());
        let (apps_remove, changed_remove) = (Rc::clone(apps), Rc::clone(changed));
        remove.connect_clicked(move |_| {
            apps_remove.borrow_mut().remove(i);
            fill(&list_remove, &parent_remove, &apps_remove, &changed_remove);
            changed_remove();
        });
        row.append(&remove);
        list.append(&row);
        if let Some(row) = row.parent().and_downcast::<gtk::ListBoxRow>() {
            row.set_activatable(false);
        }
    }

    // A button row, as GNOME's lists add items. It opens a dialog, so its label ends in "…".
    let add_label = gtk::Label::with_mnemonic("_Add App…");
    add_label.add_css_class("settings-row");
    let add_row = gtk::ListBoxRow::new();
    add_row.set_child(Some(&add_label));
    list.append(&add_row);
}

/// A dialog to pick one of the installed apps, with a search. Picking one closes it.
fn open_app_picker(parent: &gtk::Window, taken: &[String], picked: impl Fn(String) + 'static) {
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
    let ids: Vec<String> =
        installed_apps().into_iter().filter(|id| !taken.contains(id)).collect();
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

    let keys = gtk::EventControllerKey::new();
    let dialog_keys = dialog.clone();
    keys.connect_key_pressed(move |_, keyval, _, _| {
        if keyval == gtk::gdk::Key::Escape {
            dialog_keys.destroy();
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    });
    dialog.add_controller(keys);
    dialog.present();
    search.grab_focus();
}

/// Small icons for the apps a gesture works in, for the gesture list.
pub fn app_icons(apps: &[String]) -> gtk::Box {
    let icons = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    icons.set_valign(gtk::Align::Center);
    for id in apps {
        let icon = app_icon(id, LIST_ICON_SIZE);
        let name = app_name(id);
        icon.set_tooltip_text(Some(&name));
        icon.update_property(&[gtk::accessible::Property::Label(&name)]);
        icons.append(&icon);
    }
    icons
}
