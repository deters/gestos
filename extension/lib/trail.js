// The path of a gesture, drawn over everything on screen while it's being drawn, and faded once
// it's done, unless the show-gesture-path setting is off. The daemon says when a gesture starts and
// ends; in between, the path follows the pointer itself, read once a frame, so its tip stays right
// under the pointer.

import Cairo from 'cairo';
import Clutter from 'gi://Clutter';
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
        // Only as big as the path, so redrawing it each frame takes no time.
        this._area = new St.DrawingArea({reactive: false, visible: false});
        this._area.connect('repaint', area => this._draw(area));
        Main.layoutManager.uiGroup.add_child(this._area);
        // Runs once a frame while a gesture is drawn.
        this._frames = new Clutter.Timeline({actor: this._area, duration: 1000, repeat_count: -1});
        this._frames.connect('new-frame', () => this._add());

        // Only when gestures start and end. The daemon also says each time the pointer moves, which
        // is too often to handle here, and the pointer is read once a frame instead.
        this._signalIds = ['GestureStarted', 'GestureEnded'].map(signal =>
            Gio.DBus.session.signal_subscribe(null, DAEMON_INTERFACE, signal, DAEMON_PATH, null,
                Gio.DBusSignalFlags.NONE,
                () => (signal === 'GestureStarted' ? this._start() : this._end())));
    }

    destroy() {
        for (const id of this._signalIds)
            Gio.DBus.session.signal_unsubscribe(id);
        this._frames.stop();
        this._area.destroy();
    }

    _start() {
        this._area.remove_all_transitions();
        if (!this._settings.get_boolean('show-gesture-path')) {
            this._area.hide();
            return;
        }
        Main.layoutManager.uiGroup.set_child_above_sibling(this._area, null);
        this._area.opacity = 255;
        this._points = [];
        this._bounds = null;
        this._add();
        this._area.show();
        this._frames.start();
    }

    _add() {
        const [x, y] = global.get_pointer();
        const last = this._points.at(-1);
        if (last && last[0] === x && last[1] === y)
            return;
        this._points.push([x, y]);
        const b = this._bounds;
        this._bounds = b
            ? {left: Math.min(b.left, x), top: Math.min(b.top, y),
                right: Math.max(b.right, x), bottom: Math.max(b.bottom, y)}
            : {left: x, top: y, right: x, bottom: y};
        // Room for the line's width around the path.
        const {left, top, right, bottom} = this._bounds;
        this._area.set_position(left - LINE_WIDTH, top - LINE_WIDTH);
        this._area.set_size(right - left + 2 * LINE_WIDTH, bottom - top + 2 * LINE_WIDTH);
        this._area.queue_repaint();
    }

    _end() {
        this._frames.stop();
        if (!this._area.visible)
            return;
        // Where the pointer was let go, which may be after the last frame.
        this._add();
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
        const {left, top, right, bottom} = this._bounds;
        // The area starts the line's width before the path does.
        cr.translate(LINE_WIDTH - left, LINE_WIDTH - top);
        // The same violet to pink as in Gestos's own drawings.
        const gradient = new Cairo.LinearGradient(left, top, right, bottom);
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
