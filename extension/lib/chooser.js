// SPDX-License-Identifier: GPL-2.0-or-later

import Cairo from 'cairo';
import Clutter from 'gi://Clutter';
import GLib from 'gi://GLib';
import Shell from 'gi://Shell';
import St from 'gi://St';

import * as Main from 'resource:///org/gnome/shell/ui/main.js';

// How long the card waits for a choice before it gives up.
const CANCEL_AFTER_SECONDS = 15;
// How close the card gets to the edges of the screen.
const CARD_MARGIN = 12;
const PREVIEW_WIDTH = 72;
const PREVIEW_HEIGHT = 48;
// How much of a preview the gesture takes up, so its line and arrow fit inside.
const PREVIEW_FILL = 0.62;

// Where the card goes along one side: centered on the pointer, but kept on the screen.
function cardPosition(pointer, cardSize, start, size) {
    const centered = pointer - cardSize / 2;
    return Math.max(start + CARD_MARGIN, Math.min(centered, start + size - cardSize - CARD_MARGIN));
}

// Draws a gesture as the app does: a violet to pink line, with an arrow where it ends.
function drawGesture(cr, points, width, height) {
    if (points.length < 2)
        return;
    const xs = points.map(p => p.x);
    const ys = points.map(p => p.y);
    const [minX, maxX] = [Math.min(...xs), Math.max(...xs)];
    const [minY, maxY] = [Math.min(...ys), Math.max(...ys)];
    const largest = Math.max(maxX - minX, maxY - minY);
    const scale = largest > 1 ? Math.min(width, height) * PREVIEW_FILL / largest : 1;

    cr.save();
    cr.translate(width / 2, height / 2);
    cr.scale(scale, scale);
    cr.translate(-(minX + maxX) / 2, -(minY + maxY) / 2);

    cr.setLineWidth(4 / scale);
    cr.setLineCap(Cairo.LineCap.ROUND);
    cr.setLineJoin(Cairo.LineJoin.ROUND);
    const gradient = new Cairo.LinearGradient(minX, minY, maxX, maxY);
    gradient.addColorStopRGBA(0, 0.49, 0.27, 0.90, 0.9);
    gradient.addColorStopRGBA(1, 0.98, 0.72, 0.80, 0.9);
    cr.setSource(gradient);
    cr.moveTo(points[0].x, points[0].y);
    for (const p of points.slice(1))
        cr.lineTo(p.x, p.y);
    cr.stroke();

    const end = points[points.length - 1];
    const before = points[points.length - 2];
    cr.setSourceRGBA(0.49, 0.27, 0.90, 1);
    if (Math.hypot(end.x - before.x, end.y - before.y) > 0.001) {
        const angle = Math.atan2(end.y - before.y, end.x - before.x);
        const across = angle + Math.PI / 2;
        const halfWidth = 5 / scale;
        const length = 12 / scale;
        cr.moveTo(end.x + length * Math.cos(angle), end.y + length * Math.sin(angle));
        cr.lineTo(end.x + halfWidth * Math.cos(across), end.y + halfWidth * Math.sin(across));
        cr.lineTo(end.x - halfWidth * Math.cos(across), end.y - halfWidth * Math.sin(across));
        cr.closePath();
    } else {
        cr.arc(end.x, end.y, 5 / scale, 0, 2 * Math.PI);
    }
    cr.fill();
    cr.restore();
}

/*
 * Asks which of several similar gestures was meant, on a card at the pointer, styled like the
 * Shell's own menus. It's drawn by the Shell, so no window opens or takes focus. A left click on
 * a gesture picks it. Any other click, Escape, or waiting too long cancels.
 */
export class GestureChooser {
    // Each entry has a name, a description and the gesture's points. Done is called once, with
    // the index of the chosen one, or -1.
    constructor(entries, done) {
        this._entries = entries;
        this._done = done;
    }

    // A card that fails to open is taken down before the error goes back to Gestos, which then asks
    // in its own window, so the two never show together.
    open() {
        try {
            this._show();
        } catch (error) {
            this._takeDown();
            throw error;
        }
    }

    _show() {
        this._layer = new St.Widget({
            reactive: true,
            can_focus: true,
            x: 0,
            y: 0,
            width: global.stage.width,
            height: global.stage.height,
            style: 'background-color: rgba(0, 0, 0, 0.12);',
        });
        this._card = new St.BoxLayout({
            style_class: 'popup-menu-content',
            orientation: Clutter.Orientation.VERTICAL,
        });
        const menu = new St.Bin({style_class: 'popup-menu', child: this._card});
        this._entries.forEach((entry, i) => this._card.add_child(this._option(entry, i)));
        this._layer.add_child(menu);
        Main.layoutManager.uiGroup.add_child(this._layer);

        const [px, py] = global.get_pointer();
        const monitor = Main.layoutManager.monitors.find(m =>
            px >= m.x && px < m.x + m.width && py >= m.y && py < m.y + m.height) ??
            Main.layoutManager.primaryMonitor;
        const [, , width, height] = menu.get_preferred_size();
        menu.set_position(
            Math.round(cardPosition(px, width, monitor.x, monitor.width)),
            Math.round(cardPosition(py, height, monitor.y, monitor.height)));

        // Buttons recognize clicks with gestures, which leave the press to reach the layer too. So
        // only a press away from the card, or with another button, cancels. A left press on a
        // gesture goes on to pick it.
        this._layer.connect('button-press-event', (_actor, event) => {
            const [x, y] = event.get_coords();
            const [left, top] = menu.get_transformed_position();
            const [cardWidth, cardHeight] = menu.get_transformed_size();
            const onCard = x >= left && x < left + cardWidth && y >= top && y < top + cardHeight;
            if (onCard && event.get_button() === Clutter.BUTTON_PRIMARY)
                return Clutter.EVENT_PROPAGATE;
            this._finish(-1);
            return Clutter.EVENT_STOP;
        });
        this._layer.connect('key-press-event', (_actor, event) => this._onKeyPress(event));

        // The Shell's own dialogs take the grab the same way, which also gives the card key focus.
        this._grab = Main.pushModal(this._layer, {actionMode: Shell.ActionMode.POPUP});
        this._timeoutId = GLib.timeout_add_seconds(GLib.PRIORITY_DEFAULT, CANCEL_AFTER_SECONDS,
            () => {
                this._timeoutId = 0;
                this._finish(-1);
                return GLib.SOURCE_REMOVE;
            });
    }

    // Closes it as cancelled, as when the extension is turned off.
    close() {
        this._finish(-1);
    }

    _option(entry, i) {
        const content = new St.BoxLayout({style: 'spacing: 12px;'});
        const preview = new St.DrawingArea({width: PREVIEW_WIDTH, height: PREVIEW_HEIGHT});
        preview.connect('repaint', area => {
            const cr = area.get_context();
            const [width, height] = area.get_surface_size();
            drawGesture(cr, entry.points, width, height);
            cr.$dispose();
        });
        content.add_child(preview);

        const text = new St.BoxLayout({
            orientation: Clutter.Orientation.VERTICAL,
            y_align: Clutter.ActorAlign.CENTER,
        });
        text.add_child(new St.Label({text: entry.name, style: 'font-weight: bold;'}));
        if (entry.description) {
            const description = new St.Label({text: entry.description});
            description.opacity = 180;
            text.add_child(description);
        }
        content.add_child(text);

        // Only the left button picks, so any other click reaches the layer, which cancels.
        const button = new St.Button({
            style_class: 'popup-menu-item',
            child: content,
            can_focus: true,
            button_mask: St.ButtonMask.ONE,
        });
        button.connect('clicked', () => this._finish(i));
        return button;
    }

    // Escape cancels. The arrow keys and Tab move between gestures, and Enter or Space picks one.
    _onKeyPress(event) {
        const key = event.get_key_symbol();
        if (key === Clutter.KEY_Escape) {
            this._finish(-1);
            return Clutter.EVENT_STOP;
        }
        const direction = {
            [Clutter.KEY_Up]: St.DirectionType.UP,
            [Clutter.KEY_Down]: St.DirectionType.DOWN,
            [Clutter.KEY_Tab]: St.DirectionType.TAB_FORWARD,
            [Clutter.KEY_ISO_Left_Tab]: St.DirectionType.TAB_BACKWARD,
        }[key];
        if (direction === undefined)
            return Clutter.EVENT_PROPAGATE;
        const focus = global.stage.get_key_focus();
        const from = focus && this._card.contains(focus) ? focus : null;
        this._card.navigate_focus(from, direction, true);
        return Clutter.EVENT_STOP;
    }

    _finish(choice) {
        const done = this._done;
        this._takeDown();
        done?.(choice);
    }

    // Removes the card and lets go of the pointer and keyboard. After this, nothing is answered.
    _takeDown() {
        this._done = null;
        if (this._timeoutId) {
            GLib.source_remove(this._timeoutId);
            this._timeoutId = 0;
        }
        if (this._grab) {
            Main.popModal(this._grab);
            this._grab = null;
        }
        this._layer?.destroy();
        this._layer = null;
    }
}
