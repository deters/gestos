// The path of a gesture, drawn over everything on screen while it's being drawn, and faded once
// it's done, unless the show-gesture-path setting is off. The daemon says when a gesture starts,
// moves on, and ends; the path follows the pointer itself, so it's drawn right where the user
// draws.

import Cairo from 'cairo';
import Gio from 'gi://Gio';
import St from 'gi://St';

import * as Main from 'resource:///org/gnome/shell/ui/main.js';

const DAEMON_INTERFACE = 'io.github.deters.Gestos.Daemon';
const DAEMON_PATH = '/io/github/deters/Gestos/Daemon';

const LINE_WIDTH = 5;
const FADE_TIME = 300;

export class GestureTrail {
    constructor(settings) {
        this._settings = settings;
        this._points = [];
        this._area = new St.DrawingArea({reactive: false, visible: false});
        this._area.connect('repaint', area => this._draw(area));
        Main.layoutManager.uiGroup.add_child(this._area);

        this._signalId = Gio.DBus.session.signal_subscribe(null, DAEMON_INTERFACE, null,
            DAEMON_PATH, null, Gio.DBusSignalFlags.NONE,
            (_conn, _sender, _path, _iface, signal) => {
                if (signal === 'GestureStarted')
                    this._start();
                else if (signal === 'GestureUpdated')
                    this._add();
                else if (signal === 'GestureEnded')
                    this._end();
            });
    }

    destroy() {
        Gio.DBus.session.signal_unsubscribe(this._signalId);
        this._area.destroy();
    }

    _start() {
        this._area.remove_all_transitions();
        if (!this._settings.get_boolean('show-gesture-path')) {
            this._area.hide();
            return;
        }
        // Over every monitor, and above the windows and panels.
        this._area.set_position(0, 0);
        this._area.set_size(global.stage.width, global.stage.height);
        Main.layoutManager.uiGroup.set_child_above_sibling(this._area, null);
        this._area.opacity = 255;
        this._area.show();
        this._points = [];
        this._add();
    }

    _add() {
        if (!this._area.visible)
            return;
        const [x, y] = global.get_pointer();
        this._points.push([x, y]);
        this._area.queue_repaint();
    }

    _end() {
        this._area.ease({
            opacity: 0,
            duration: FADE_TIME,
            onComplete: () => {
                this._area.hide();
                this._points = [];
            },
        });
    }

    _draw(area) {
        if (this._points.length < 2)
            return;
        const cr = area.get_context();
        const xs = this._points.map(([x]) => x);
        const ys = this._points.map(([, y]) => y);
        // The same violet to pink as in Gestos's own drawings.
        const gradient = new Cairo.LinearGradient(Math.min(...xs), Math.min(...ys),
            Math.max(...xs), Math.max(...ys));
        gradient.addColorStopRGBA(0, 0.49, 0.27, 0.90, 0.9);
        gradient.addColorStopRGBA(1, 0.98, 0.72, 0.80, 0.9);
        cr.setSource(gradient);
        cr.setLineWidth(LINE_WIDTH);
        cr.setLineCap(Cairo.LineCap.ROUND);
        cr.setLineJoin(Cairo.LineJoin.ROUND);
        cr.moveTo(...this._points[0]);
        for (const point of this._points.slice(1))
            cr.lineTo(...point);
        cr.stroke();
        cr.$dispose();
    }
}
