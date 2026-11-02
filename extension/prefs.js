// SPDX-License-Identifier: GPL-2.0-or-later

import Adw from 'gi://Adw';
import Gio from 'gi://Gio';

import {ExtensionPreferences} from 'resource:///org/gnome/Shell/Extensions/js/extensions/prefs.js';

export default class GestosPreferences extends ExtensionPreferences {
    fillPreferencesWindow(window) {
        // The window keeps the settings alive for as long as the binding needs them.
        window._settings = this.getSettings();

        const gapRow = Adw.SpinRow.new_with_range(0, 100, 1);
        gapRow.title = 'Window Gap';
        gapRow.subtitle = 'Space around tiled windows';
        window._settings.bind('window-gap', gapRow, 'value', Gio.SettingsBindFlags.DEFAULT);

        const group = new Adw.PreferencesGroup();
        group.add(gapRow);
        const page = new Adw.PreferencesPage();
        page.add(group);
        window.add(page);
    }
}
