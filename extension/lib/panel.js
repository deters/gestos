// A small dark panel at the bottom of the screen the pointer is on, saying what a gesture did, that
// it couldn't and why, or that it wasn't one of the user's. It fades away by itself, and takes no
// clicks, as the panels apps show pressed keys in do.

import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import St from 'gi://St';

import * as Main from 'resource:///org/gnome/shell/ui/main.js';

const DAEMON_INTERFACE = 'io.github.deters.Gestos.Daemon';
const DAEMON_PATH = '/io/github/deters/Gestos/Daemon';

// How long it shows, in milliseconds: longer for a failure, which there's more to read in.
const SHOWN_TIME = 1200;
const FAILED_SHOWN_TIME = 3500;
const FADE_TIME = 200;
// How far above the bottom of the screen it is, in pixels.
const BOTTOM_MARGIN = 72;

const STYLE = 'background-color: rgba(0, 0, 0, 0.72); border-radius: 14px; padding: 10px 18px; spacing: 10px;';
const FAILED_STYLE = 'background-color: rgba(120, 20, 20, 0.85); border-radius: 14px; padding: 10px 18px; spacing: 10px;';
const LABEL_STYLE = 'color: white; font-weight: bold;';
// What a command printed, in a fixed-width font and lined up on the left, as it printed it.
const PRINTED_STYLE = 'color: white; font-family: monospace; text-align: left;';

export class ActionPanel {
    constructor() {
        this._box = new St.BoxLayout({reactive: false, visible: false, opacity: 0, style: STYLE});
        this._icon = new St.Icon({icon_size: 20, style: 'color: white;', y_align: Clutter.ActorAlign.CENTER});
        this._label = new St.Label({style: LABEL_STYLE, y_align: Clutter.ActorAlign.CENTER});
        this._box.add_child(this._icon);
        this._box.add_child(this._label);
        Main.layoutManager.uiGroup.add_child(this._box);
        this._hideId = 0;

        const handlers = {
            ActionExecuted: params => {
                const [name, , icon] = params.deepUnpack();
                this._show(icon, name, false);
            },
            CommandOutput: params => {
                const [text, seconds] = params.deepUnpack();
                this._show('utilities-terminal-symbolic', text, false, seconds * 1000);
            },
            ActionFailed: params => {
                const [reason] = params.deepUnpack();
                this._show('dialog-warning-symbolic', reason, true);
            },
            GestureUnrecognized: () => {
                this._show('dialog-question-symbolic', 'Gesture not recognized', false);
            },
        };
        this._signalIds = Object.entries(handlers).map(([signal, handler]) =>
            Gio.DBus.session.signal_subscribe(null, DAEMON_INTERFACE, signal, DAEMON_PATH, null,
                Gio.DBusSignalFlags.NONE, (...args) => handler(args.at(-1))));
    }

    destroy() {
        for (const id of this._signalIds)
            Gio.DBus.session.signal_unsubscribe(id);
        if (this._hideId)
            GLib.source_remove(this._hideId);
        this._box.destroy();
    }

    // A command's output is shown for the time given, in milliseconds.
    _show(iconName, text, failed, printedTime = 0) {
        this._icon.icon_name = iconName;
        this._label.text = text;
        this._label.style = printedTime ? PRINTED_STYLE : LABEL_STYLE;
        this._box.style = failed ? FAILED_STYLE : STYLE;

        const monitor = Main.layoutManager.monitors[global.display.get_current_monitor()] ??
            Main.layoutManager.primaryMonitor;
        const [, width] = this._box.get_preferred_width(-1);
        const [, height] = this._box.get_preferred_height(width);
        this._box.set_position(
            Math.round(monitor.x + (monitor.width - width) / 2),
            Math.round(monitor.y + monitor.height - height - BOTTOM_MARGIN));
        Main.layoutManager.uiGroup.set_child_above_sibling(this._box, null);

        this._box.remove_all_transitions();
        this._box.show();
        this._box.ease({opacity: 255, duration: FADE_TIME / 2});

        if (this._hideId)
            GLib.source_remove(this._hideId);
        const time = printedTime || (failed ? FAILED_SHOWN_TIME : SHOWN_TIME);
        this._hideId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, time, () => {
            this._hideId = 0;
            this._box.ease({
                opacity: 0,
                duration: FADE_TIME,
                onComplete: () => this._box.hide(),
            });
            return GLib.SOURCE_REMOVE;
        });
    }
}
