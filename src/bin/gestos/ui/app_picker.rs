//! The apps a gesture works in, as a list in its trigger: a title row with a button to add one from
//! the installed apps, and one row for each app, with a button to remove it. With no apps, it works
//! in all of them.

use gtk::prelude::*;
use gtk::gio;
use gtk4 as gtk;
use crate::extension::ExtensionStatus;
use std::cell::RefCell;
use std::rc::Rc;

// The size of app icons in the dialog that picks one, in the list of chosen ones, and in the list
// of gestures.
const ICON_SIZE: i32 = 32;
const ROW_ICON_SIZE: i32 = 24;
const LIST_ICON_SIZE: i32 = 16;

/// An installed app, by the desktop file ID gestures use for it, like `org.mozilla.firefox`.
fn app_info(id: &str) -> Option<gio::DesktopAppInfo> {
    gio::DesktopAppInfo::new(&format!("{id}.desktop"))
}

/// The app's name, or its ID for one that isn't installed.
pub fn app_name(id: &str) -> String {
    gestos::key_steps::app_name(id)
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

/// The installed apps that show in the app grid, by name, as their IDs.
fn installed_apps() -> Vec<String> {
    gestos::key_steps::installed_apps().into_iter().map(|(_, id)| id).collect()
}

/// What keeps a gesture limited to some apps from running, if anything. The daemon knows the app
/// in use from GNOME's extension, or, without GNOME, where the desktop can say: `elsewhere`.
pub fn apps_note(status: &ExtensionStatus, elsewhere: bool) -> Option<&'static str> {
    match status {
        ExtensionStatus::Running => None,
        ExtensionStatus::Unavailable if elsewhere => None,
        ExtensionStatus::Unavailable => {
            Some("Only works on GNOME, KDE and X11: here, a gesture limited to apps doesn't run")
        }
        // Named as GNOME's Extensions app lists it, so it can be found there.
        ExtensionStatus::Outdated(_) => {
            Some("Needs the \u{201c}Gestos\u{201d} GNOME extension to be updated")
        }
        _ => Some("Needs the \u{201c}Gestos\u{201d} GNOME extension, which tells the app in use"),
    }
}

/// The rows for the apps a gesture works in, at the end of a list that can hold other rows too,
/// such as the rest of its trigger.
struct AppRows {
    list: gtk::ListBox,
    parent: gtk::Window,
    apps: Rc<RefCell<Vec<String>>>,
    changed: Rc<dyn Fn()>,
    rows: RefCell<Vec<gtk::ListBoxRow>>,
    /// Apps can only be added where they'd work, and ones added elsewhere can still be removed.
    can_add: bool,
    note: Option<&'static str>,
}

/// Adds the apps in `apps` to the end of `list`, under a title row with a button to add more.
/// `changed` is called after the user adds or removes one. `status` is the GNOME extension's,
/// which says which app is in use.
pub fn append_apps_rows(
    list: &gtk::ListBox,
    parent: &gtk::Window,
    apps: Rc<RefCell<Vec<String>>>,
    changed: Rc<dyn Fn()>,
    status: &ExtensionStatus,
) {
    let app_rows = Rc::new(AppRows {
        list: list.clone(),
        parent: parent.clone(),
        apps,
        changed,
        rows: RefCell::new(Vec::new()),
        can_add: *status != ExtensionStatus::Unavailable
            || gestos::active_app::can_tell_without_extension(),
        note: apps_note(status, gestos::active_app::can_tell_without_extension()),
    });
    app_rows.fill();
}

impl AppRows {
    fn append(&self, child: &impl IsA<gtk::Widget>, activatable: bool) {
        let row = gtk::ListBoxRow::new();
        row.set_activatable(activatable);
        row.set_child(Some(child));
        self.list.append(&row);
        self.rows.borrow_mut().push(row);
    }

    fn fill(self: &Rc<Self>) {
        for row in self.rows.borrow_mut().drain(..) {
            self.list.remove(&row);
        }

        let title_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        title_row.add_css_class("settings-row");
        let title = gtk::Label::new(Some("Valid Apps"));
        title.set_halign(gtk::Align::Start);
        title.set_hexpand(true);
        title_row.append(&title);
        let scope = if self.apps.borrow().is_empty() {
            "Valid in All Apps"
        } else {
            "Valid Only in the Following Apps"
        };
        let scope = gtk::Label::new(Some(scope));
        scope.add_css_class("dim-label");
        title_row.append(&scope);
        // At the top of the list, as GNOME's lists can add items. It opens a dialog.
        let add = gtk::Button::from_icon_name("list-add-symbolic");
        add.set_tooltip_text(Some("Add App…"));
        add.update_property(&[gtk::accessible::Property::Label("Add App…")]);
        add.add_css_class("flat");
        add.set_valign(gtk::Align::Center);
        add.set_sensitive(self.can_add);
        let adding = Rc::clone(self);
        add.connect_clicked(move |_| {
            let picked = Rc::clone(&adding);
            let taken = adding.apps.borrow().clone();
            open_app_picker(&adding.parent, &taken, move |id| {
                picked.apps.borrow_mut().push(id);
                picked.fill();
                (picked.changed)();
            });
        });
        title_row.append(&add);
        self.append(&title_row, false);

        if let Some(note) = self.note {
            let label = gtk::Label::new(Some(note));
            label.add_css_class("dim-label");
            label.add_css_class("settings-row");
            label.set_halign(gtk::Align::Start);
            label.set_xalign(0.0);
            label.set_wrap(true);
            self.append(&label, false);
        }

        let apps = self.apps.borrow().clone();
        for (i, id) in apps.iter().enumerate() {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
            row.add_css_class("settings-row");
            row.append(&app_icon(id, ROW_ICON_SIZE));
            let name = gtk::Label::new(Some(&app_name(id)));
            name.set_halign(gtk::Align::Start);
            name.set_hexpand(true);
            row.append(&name);
            let remove = gtk::Button::from_icon_name("user-trash-symbolic");
            remove.set_tooltip_text(Some("Remove"));
            remove.add_css_class("flat");
            remove.set_valign(gtk::Align::Center);
            let removed = Rc::clone(self);
            remove.connect_clicked(move |_| {
                removed.apps.borrow_mut().remove(i);
                removed.fill();
                (removed.changed)();
            });
            row.append(&remove);
            self.append(&row, false);
        }
    }
}

/// A dialog to pick one of the installed apps, with a search. Picking one closes it.
pub fn open_app_picker(
    parent: &gtk::Window,
    taken: &[String],
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

    crate::ui::components::close_with_keys(&dialog);
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
}
