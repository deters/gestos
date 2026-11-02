// SPDX-License-Identifier: GPL-2.0-or-later

import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Meta from 'gi://Meta';
import Shell from 'gi://Shell';

import {Extension, InjectionManager} from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';

import {hasSize, keepInside, makeTiling, moveTileEdge, restoreRect, tilingRects} from './lib/tile.js';

const OBJECT_PATH = '/io/github/deters/Gestos/Extension';
const INTERFACE_XML = `
<node>
  <interface name="io.github.deters.Gestos.Extension">
    <method name="ToggleTileWindows">
      <arg type="b" direction="out" name="changed"/>
    </method>
  </interface>
</node>`;

// How long tiling waits for a window that was maximized to report its normal size.
const UNMAXIMIZE_TIMEOUT_MS = 1000;
// How long a window gets to take its original size back before it's closed or maximized anyway.
const RESIZE_TIMEOUT_MS = 500;
// How long a window that was asked to close stays frozen, in case its app doesn't close it.
const CLOSE_THAW_TIMEOUT_MS = 1000;
// How long a window that's maximized again stays frozen, if its app doesn't take the new size.
const MAXIMIZE_THAW_TIMEOUT_MS = 500;
// How long a size to undo is kept for an app that isn't opened again.
const CLOSED_SIZE_DAYS = 30;

// Apps without a desktop file get a made-up app for each window, so those go by their own id.
function appIdOf(window) {
    const app = Shell.WindowTracker.get_default().get_window_app(window);
    return app && !app.is_window_backed() ? app.get_id() : window.get_wm_class();
}

function isMoving(op) {
    return op === Meta.GrabOp.MOVING || op === Meta.GrabOp.MOVING_UNCONSTRAINED ||
        op === Meta.GrabOp.KEYBOARD_MOVING;
}

// Snapping a window to half the screen maximizes it in one direction.
function isMaximizedAtAll(window) {
    return window.get_maximize_flags() !== 0;
}

function isTileable(window) {
    // A maximized or snapped window can't be resized until it's restored, which tiling does.
    return !window.minimized &&
        window.get_window_type() === Meta.WindowType.NORMAL &&
        !window.is_fullscreen() && (isMaximizedAtAll(window) || window.allows_resize());
}

function copyRect(rect) {
    return {x: rect.x, y: rect.y, width: rect.width, height: rect.height};
}

/*
 * Apps remember the size their windows had while they weren't maximized, and save it as they close.
 * Every window this resizes has its original size remembered here, and gets it back, with the app
 * given time to take it, before anything could make the app keep the size it was given instead:
 * when tiling ends, when it's closed from the Shell, when it's maximized or made
 * fullscreen, and when the session ends. A window closed by its app can't be caught in time, so the
 * next window that app opens at the size it was given gets its original size instead.
 */
class WindowActions {
    // Windows this resized, with their original size, and the size they were given.
    _originals = new Map();
    // Workspaces that are tiled, each with its windows and their tiles.
    _tilings = new Map();
    // Windows whose picture is held while they change size where nobody should see it.
    _frozen = new Set();
    constructor(settings) {
        this._settings = settings;
        // Apps whose window closed at a size this gave it, with its original size and that size.
        // They're kept across sessions, as the app keeps the size it saved until it's opened again.
        this._closedSizes = this._loadClosedSizes();
        this._grabId = global.display.connect('grab-op-begin',
            (_display, window, op) => this._onGrabBegin(window, op));
        this._grabEndId = global.display.connect('grab-op-end',
            (_display, window, op) => this._onGrabEnd(window, op));
        this._workAreasId = global.display.connect('workareas-changed',
            () => this._onWorkAreasChanged());
        // Gestos's Preferences change the gap too, and windows follow it right away.
        this._gapId = settings.connect('changed::window-gap', () => this._onWorkAreasChanged());
        // The Shell says when the session is about to end once the user confirms it.
        this._endSessionId = Gio.DBus.session.signal_subscribe(null,
            'org.gnome.SessionManager.EndSessionDialog', null,
            '/org/gnome/SessionManager/EndSessionDialog', null, Gio.DBusSignalFlags.NONE,
            (_conn, _sender, _path, _iface, signal) => {
                if (signal.startsWith('Confirmed'))
                    this.restoreAll();
            });
        // A window is mapped once it's on screen at its real size, which is when it can join.
        this._mapId = global.window_manager.connect('map',
            (_wm, actor) => this._onWindowMapped(actor.meta_window));

        // Windows closed from the Shell, by its close shortcut, window menu, or overview, get their
        // original size back before they close.
        const actions = this;
        this._injections = new InjectionManager();
        this._injections.overrideMethod(Meta.Window.prototype, 'delete', close => function (time) {
            if (!actions._closeAtOriginalSize(this, () => close.call(this, time)))
                close.call(this, time);
        });
        // The close shortcut is handled inside Mutter, which wouldn't go through the above.
        Meta.keybindings_set_custom_handler('close',
            (_display, window) => window?.delete(global.get_current_time()));
    }

    destroy() {
        global.display.disconnect(this._grabId);
        global.display.disconnect(this._grabEndId);
        global.display.disconnect(this._workAreasId);
        this._settings.disconnect(this._gapId);
        Gio.DBus.session.signal_unsubscribe(this._endSessionId);
        global.window_manager.disconnect(this._mapId);
        Meta.keybindings_set_custom_handler('close', null);
        this._injections.clear();
        for (const tiling of this._tilings.values())
            this._stopTiling(tiling);
        this._tilings.clear();
        for (const window of [...this._originals.keys()])
            this._untrack(window);
        for (const frozen of [...this._frozen])
            frozen.release(true);
    }

    // What's needed to pick up where this left off, for when the extension is enabled again.
    saveState() {
        for (const tiling of this._tilings.values()) {
            this._stopWaiting(tiling);
            for (const window of tiling.windows.keys()) {
                const original = this._originals.get(window);
                if (original)
                    original.rect ??= copyRect(window.get_frame_rect());
            }
        }
        const copy = map => [...map].map(([key, value]) => [key, {...value}]);
        return {
            originals: [...this._originals]
                .filter(([, original]) => !original.fixing)
                .map(([window, {rect, appId, size, snapped}]) =>
                    [window, {rect, appId, size, snapped}]),
            tilings: [...this._tilings].map(([workspace, tiling]) =>
                [workspace, {windows: copy(tiling.windows), plans: tiling.plans}]),
        };
    }

    // Picks up from saveState, leaving out windows that closed in between.
    loadState(saved) {
        const open = new Set(global.get_window_actors().map(actor => actor.meta_window));
        for (const [window, original] of saved.originals) {
            if (open.has(window))
                Object.assign(this._track(window, original.rect), original);
        }
        for (const [workspace, {windows, plans}] of saved.tilings) {
            const tiling = this._startTiling(workspace);
            tiling.plans = plans;
            for (const [window, state] of windows) {
                if (this._originals.has(window))
                    tiling.windows.set(window, state);
            }
            if (tiling.windows.size < windows.length)
                this._layoutWhenReady(tiling);
        }
    }

    _loadClosedSizes() {
        const oldest = Date.now() / 1000 - CLOSED_SIZE_DAYS * 24 * 60 * 60;
        const saved = this._settings.get_value('closed-sizes').deepUnpack();
        return new Map(Object.entries(saved)
            .filter(([, [, , , , time]]) => time >= oldest)
            .map(([appId, [width, height, sizeWidth, sizeHeight, time]]) => [appId, {
                rect: {width, height},
                size: {width: sizeWidth, height: sizeHeight},
                time,
            }]));
    }

    _saveClosedSizes() {
        const entries = [...this._closedSizes].map(([appId, {rect, size, time}]) =>
            [appId, [rect.width, rect.height, size.width, size.height, time]]);
        this._settings.set_value('closed-sizes',
            new GLib.Variant('a{s(iiiix)}', Object.fromEntries(entries)));
    }

    // Windows that were already open when this started, as apps can open theirs at login before
    // the Shell enables extensions.
    fixOpenWindows() {
        for (const actor of global.get_window_actors()) {
            const window = actor.meta_window;
            const reopened = this._takeClosedSize(window);
            if (reopened)
                this._resizeReopened(window, reopened);
        }
    }

    _resizeReopened(window, reopened) {
        if (isMaximizedAtAll(window))
            window.unmaximize();
        const frame = window.get_frame_rect();
        const {x, y} = keepInside({x: frame.x, y: frame.y, ...reopened.rect},
            window.get_work_area_current_monitor());
        window.move_resize_frame(true, x, y, reopened.rect.width, reopened.rect.height);
    }

    // Puts every window this resized back at its original size: tiled and snapped ones.
    restoreAll() {
        for (const workspace of [...this._tilings.keys()])
            this._untile(workspace, null);
        for (const window of [...this._originals.keys()]) {
            if (isMaximizedAtAll(window))
                window.unmaximize();
            this._restoreOriginal(window);
        }
    }

    // Remembers a window's original size, and watches it from then on. A window that's already
    // remembered keeps its first original size.
    _track(window, rect) {
        const known = this._originals.get(window);
        if (known)
            return known;
        const original = {
            rect: rect && copyRect(rect),
            appId: appIdOf(window),
            size: null,
            snapped: false,
            fixing: false,
            closing: false,
            signalIds: [
                // A window that's going away can't be resized, and leaves its workspace as it goes.
                window.connect('unmanaging', () => {
                    this._originals.get(window).closing = true;
                }),
                window.connect('unmanaged', () => this._onUnmanaged(window)),
                window.connect('workspace-changed', () => this._onWorkspaceChanged(window)),
                ...['notify::maximized-horizontally', 'notify::maximized-vertically',
                    'notify::fullscreen'].map(signal =>
                    window.connect(signal, () => this._onStateChanged(window))),
            ],
            waitId: 0,
            timeoutId: 0,
        };
        this._originals.set(window, original);
        return original;
    }

    _untrack(window) {
        const original = this._originals.get(window);
        if (!original)
            return;
        this._originals.delete(window);
        for (const id of original.signalIds)
            window.disconnect(id);
        this._stopResizeWait(window, original);
    }

    // Gives a window a size, remembering that it's one this gave it.
    _resizeTo(window, rect) {
        this._originals.get(window).size = copyRect(rect);
        window.move_resize_frame(true, rect.x, rect.y, rect.width, rect.height);
    }

    // Gives a window its original size back, and stops watching it.
    _restoreOriginal(window) {
        const original = this._originals.get(window);
        if (original?.rect && !original.closing) {
            const {x, y, width, height} =
                keepInside(original.rect, window.get_work_area_current_monitor());
            window.move_resize_frame(true, x, y, width, height);
        }
        this._untrack(window);
    }

    // Gives a window its original size back, and calls done once the app has taken it, or after a
    // while if it doesn't.
    _restoreOriginalThen(window, original, done) {
        original.fixing = true;
        if (isMaximizedAtAll(window))
            window.unmaximize();
        if (window.is_fullscreen())
            window.unmake_fullscreen();
        if (!original.rect) {
            done();
            return;
        }
        const rect = keepInside(original.rect, window.get_work_area_current_monitor());
        const finish = () => {
            this._stopResizeWait(window, original);
            done();
        };
        original.waitId = window.connect('size-changed', () => {
            if (hasSize(window.get_frame_rect(), rect))
                finish();
        });
        original.timeoutId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, RESIZE_TIMEOUT_MS, () => {
            original.timeoutId = 0;
            finish();
            return GLib.SOURCE_REMOVE;
        });
        window.move_resize_frame(true, rect.x, rect.y, rect.width, rect.height);
    }

    _stopResizeWait(window, original) {
        if (original.waitId) {
            window.disconnect(original.waitId);
            original.waitId = 0;
        }
        if (original.timeoutId) {
            GLib.source_remove(original.timeoutId);
            original.timeoutId = 0;
        }
    }

    // Holds the window's picture as it is, so that resizing it doesn't show, until it's thawed: once
    // it changes size again if asked, or after a while. A window that closes is never thawed, as
    // its picture goes with it.
    _freeze(window) {
        const actor = window.get_compositor_private();
        if (!actor)
            return () => {};
        actor.freeze();
        return (onResize, timeout) => {
            const frozen = {ids: [], timeoutId: 0};
            frozen.release = thaw => {
                for (const id of frozen.ids)
                    window.disconnect(id);
                if (frozen.timeoutId)
                    GLib.source_remove(frozen.timeoutId);
                this._frozen.delete(frozen);
                if (thaw)
                    actor.thaw();
            };
            frozen.ids.push(window.connect('unmanaging', () => frozen.release(false)));
            if (onResize)
                frozen.ids.push(window.connect('size-changed', () => frozen.release(true)));
            frozen.timeoutId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, timeout, () => {
                frozen.timeoutId = 0;
                frozen.release(true);
                return GLib.SOURCE_REMOVE;
            });
            this._frozen.add(frozen);
        };
    }

    // Takes a window out of tiling. Maximizing or snapping a tiled window ends tiling for all of its
    // workspace, and other changes only take that window out.
    _leave(window, endTiling) {
        const [workspace, tiling] = this._findTiling(window);
        if (!tiling)
            return;
        if (endTiling) {
            this._untile(workspace, window);
        } else {
            tiling.windows.delete(window);
            this._layoutWhenReady(tiling);
        }
    }

    // The window is about to close. Returns false to let it close as it is.
    _closeAtOriginalSize(window, close) {
        const original = this._originals.get(window);
        if (!original?.rect || original.fixing || original.closing)
            return false;
        this._leave(window, false);
        // It closes looking as it did, with its original size taken where it can't be seen.
        const thawLater = this._freeze(window);
        this._restoreOriginalThen(window, original, () => {
            this._untrack(window);
            close();
            thawLater(false, CLOSE_THAW_TIMEOUT_MS);
        });
        return true;
    }

    _onUnmanaged(window) {
        const original = this._originals.get(window);
        // Its app saved the size this gave it, so the next window it opens at that size gets the
        // original size instead.
        if (original.rect && original.size && !original.fixing) {
            this._closedSizes.set(original.appId, {
                rect: original.rect,
                size: original.size,
                time: Math.floor(Date.now() / 1000),
            });
            this._saveClosedSizes();
        }
        this._leave(window, false);
        this._untrack(window);
    }

    // Maximized, fullscreen and snapped windows can't be tiled, so tiling ends for their workspace.
    // A maximized or fullscreen window gets its original size back first, so that's its normal
    // size. A snapped window can't be snapped again by the Shell, so it gets its original size back
    // once the snap is undone instead.
    _onStateChanged(window) {
        const original = this._originals.get(window);
        if (original.fixing || original.closing)
            return;
        const [, tiling] = this._findTiling(window);
        // Still being restored as it joins tiling.
        if (tiling && !tiling.windows.get(window).tile)
            return;

        const flags = window.get_maximize_flags();
        if (window.is_fullscreen() || flags === Meta.MaximizeFlags.BOTH) {
            const fullscreen = window.is_fullscreen();
            this._leave(window, true);
            // Its original size is taken where it can't be seen, until it's maximized again.
            const thawLater = this._freeze(window);
            this._restoreOriginalThen(window, original, () => {
                this._untrack(window);
                if (fullscreen)
                    window.make_fullscreen();
                else
                    window.maximize();
                thawLater(true, MAXIMIZE_THAW_TIMEOUT_MS);
            });
        } else if (flags !== 0) {
            if (!original.snapped) {
                this._leave(window, true);
                original.snapped = true;
            }
        } else if (original.snapped) {
            const frame = window.get_frame_rect();
            if (original.rect)
                window.move_resize_frame(true, frame.x, frame.y, original.rect.width, original.rect.height);
            this._untrack(window);
        }
    }

    // A tiled window moved to another workspace leaves its tiling, and joins the other one if
    // that's tiled, or gets its original size back if it isn't.
    _onWorkspaceChanged(window) {
        const [workspace, tiling] = this._findTiling(window);
        const target = window.get_workspace();
        // A window on all workspaces has none of its own, and stays where it was tiled.
        if (!tiling || !target || workspace === target || this._originals.get(window).closing)
            return;
        tiling.windows.delete(window);
        this._layoutWhenReady(tiling);
        const targetTiling = target && this._tilings.get(target);
        if (targetTiling) {
            this._addToTiling(targetTiling, window);
            this._layoutWhenReady(targetTiling);
        } else {
            this._restoreOriginal(window);
        }
    }

    // How a reopened window was before tiling, if its app saved the size it was given.
    // A window that opens at nearly the whole screen is maximized by the Shell, so it can be
    // maximized at that size too.
    _takeClosedSize(window) {
        const appId = appIdOf(window);
        const closed = appId && this._closedSizes.get(appId);
        if (!closed || window.get_window_type() !== Meta.WindowType.NORMAL)
            return null;
        this._closedSizes.delete(appId);
        this._saveClosedSizes();
        return hasSize(window.get_frame_rect(), closed.size) ? closed : null;
    }

    _startTiling(workspace) {
        const tiling = {workspace, windows: new Map(), plans: [], waits: [], timeoutId: 0};
        // Windows moved here from a workspace that isn't tiled join. New windows have no size yet,
        // and join once they're on screen.
        tiling.addedId = workspace.connect('window-added', (_workspace, window) => {
            if (this._findTiling(window)[1] || window.get_frame_rect().width === 0 ||
                !isTileable(window))
                return;
            this._addToTiling(tiling, window);
            this._layoutWhenReady(tiling);
        });
        this._tilings.set(workspace, tiling);
        return tiling;
    }

    _stopTiling(tiling) {
        this._stopWaiting(tiling);
        tiling.workspace.disconnect(tiling.addedId);
    }

    // Adds a window to the tiling. A maximized or snapped one is restored first, and its normal
    // size is only known once the app has resized, so it has no original size until then. A window
    // opened again after closing at a size this gave it gets its original size, at the place it
    // opened at now.
    _addToTiling(tiling, window, reopened = null) {
        const maximized = isMaximizedAtAll(window);
        const frame = window.get_frame_rect();
        let rect = maximized ? null : frame;
        if (reopened)
            rect = {x: frame.x, y: frame.y, width: reopened.rect.width, height: reopened.rect.height};
        const original = this._track(window, rect);
        original.snapped = false;
        tiling.windows.set(window, {tile: null});
        if (maximized)
            window.unmaximize();
    }

    // Places the tiles once every window's original size is known.
    _layoutWhenReady(tiling) {
        this._stopWaiting(tiling);
        const waiting = [...tiling.windows.keys()]
            .filter(window => !this._originals.get(window).rect);
        if (waiting.length === 0) {
            this._layout(tiling);
            return;
        }

        const finish = () => {
            this._stopWaiting(tiling);
            for (const window of tiling.windows.keys()) {
                const original = this._originals.get(window);
                original.rect ??= copyRect(window.get_frame_rect());
            }
            this._layout(tiling);
        };
        for (const window of waiting) {
            const id = window.connect('size-changed', () => {
                this._originals.get(window).rect = copyRect(window.get_frame_rect());
                if ([...tiling.windows.keys()].every(w => this._originals.get(w).rect))
                    finish();
            });
            tiling.waits.push([window, id]);
        }
        tiling.timeoutId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, UNMAXIMIZE_TIMEOUT_MS, () => {
            tiling.timeoutId = 0;
            finish();
            return GLib.SOURCE_REMOVE;
        });
    }

    _stopWaiting(tiling) {
        for (const [window, id] of tiling.waits)
            window.disconnect(id);
        tiling.waits = [];
        if (tiling.timeoutId) {
            GLib.source_remove(tiling.timeoutId);
            tiling.timeoutId = 0;
        }
    }

    // Plans the tiles of each monitor from the windows' original sizes, so the layout stays the same
    // as windows join and leave.
    _layout(tiling) {
        const monitors = new Map();
        for (const window of tiling.windows.keys()) {
            const monitor = window.get_monitor();
            monitors.set(monitor, [...monitors.get(monitor) ?? [], window]);
        }
        const gap = this._settings.get_int('window-gap');
        tiling.plans = [...monitors.values()].map(group => ({
            windows: group,
            tiling: makeTiling(group.map(window => this._originals.get(window).rect),
                group[0].get_work_area_current_monitor(), gap),
        }));
        this._place(tiling);
    }

    _place(tiling) {
        for (const plan of tiling.plans) {
            const tiles = tilingRects(plan.tiling);
            plan.windows.forEach((window, i) => {
                tiling.windows.get(window).tile = tiles[i];
                if (!this._originals.get(window).closing)
                    this._resizeTo(window, tiles[i]);
            });
        }
    }

    // Panels and docks change the room tiles have, and so does the gap, so they're placed again,
    // keeping their splits.
    _onWorkAreasChanged() {
        const gap = this._settings.get_int('window-gap');
        for (const tiling of this._tilings.values()) {
            for (const plan of tiling.plans) {
                plan.tiling.area = makeTiling([],
                    plan.windows[0].get_work_area_current_monitor(), gap).area;
                plan.tiling.gap = gap;
            }
            this._place(tiling);
        }
    }

    _onWindowMapped(window) {
        const reopened = this._takeClosedSize(window);
        const workspace = window.get_workspace();
        const tiling = workspace && this._tilings.get(workspace);
        if (tiling && !tiling.windows.has(window) && isTileable(window)) {
            this._addToTiling(tiling, window, reopened);
            this._layoutWhenReady(tiling);
        } else if (reopened) {
            this._resizeReopened(window, reopened);
        }
    }

    // Puts every tiled window on the workspace back at its original size, except the one given.
    _untile(workspace, except) {
        const tiling = this._tilings.get(workspace);
        this._tilings.delete(workspace);
        this._stopTiling(tiling);
        for (const window of tiling.windows.keys()) {
            if (window !== except)
                this._restoreOriginal(window);
        }
    }

    _findTiling(window) {
        for (const [workspace, tiling] of this._tilings) {
            if (tiling.windows.has(window))
                return [workspace, tiling];
        }
        return [null, null];
    }

    // A resized tile stays tiled. The edges that were dragged stay where they were left, and the
    // tiles beyond them make room.
    _onGrabEnd(window, op) {
        if (isMoving(op))
            return;
        const [, tiling] = this._findTiling(window);
        const plan = tiling?.plans.find(p => p.windows.includes(window));
        const tile = tiling?.windows.get(window).tile;
        if (!plan || !tile)
            return;

        const frame = window.get_frame_rect();
        const index = plan.windows.indexOf(window);
        const edges = [
            ['left', tile.x, frame.x],
            ['right', tile.x + tile.width, frame.x + frame.width],
            ['top', tile.y, frame.y],
            ['bottom', tile.y + tile.height, frame.y + frame.height],
        ];
        for (const [edge, before, after] of edges) {
            if (before !== after)
                moveTileEdge(plan.tiling, index, edge, after);
        }
        this._place(tiling);
    }

    // Moving a tiled window gives it its original size back under the pointer, and ends tiling for
    // its workspace.
    _onGrabBegin(window, op) {
        if (!isMoving(op))
            return;
        const [workspace, tiling] = this._findTiling(window);
        if (!tiling)
            return;
        this._untile(workspace, window);

        const original = this._originals.get(window);
        if (original?.rect) {
            const [pointerX] = global.get_pointer();
            const {x, y, width, height} =
                restoreRect(window.get_frame_rect(), original.rect, pointerX);
            window.move_resize_frame(true, x, y, width, height);
        }
        this._untrack(window);
    }

    /**
     * Tiles the windows on the current workspace, or puts them back if it's tiled.
     *
     * Each window stays near where it was, and the windows share each monitor in proportion to
     * their original sizes, without overlaps. Maximized and snapped windows are restored first,
     * and windows that open, close, or move in or out while it's tiled join or leave.
     *
     * @returns {boolean} whether any window changed
     */
    ToggleTileWindows() {
        const workspace = global.workspace_manager.get_active_workspace();
        if (this._tilings.has(workspace)) {
            this._untile(workspace, null);
            return true;
        }

        // A workspace with no windows can be tiled too, for the windows that open there next.
        const windows = workspace.list_windows().filter(isTileable);
        const tiling = this._startTiling(workspace);
        for (const window of windows)
            this._addToTiling(tiling, window);
        this._layoutWhenReady(tiling);
        return windows.length > 0;
    }
}

// What the extension was doing when the screen locked, to carry on with once it's enabled again.
let lockedState = null;

export default class GestosExtension extends Extension {
    enable() {
        this._actions = new WindowActions(this.getSettings());
        if (lockedState) {
            this._actions.loadState(lockedState);
            lockedState = null;
        } else {
            this._actions.fixOpenWindows();
        }
        this._dbus = Gio.DBusExportedObject.wrapJSObject(INTERFACE_XML, this._actions);
        this._dbus.export(Gio.DBus.session, OBJECT_PATH);
    }

    // This runs when the extension is turned off, and also when the screen locks: other extensions
    // are turned off there, and the Shell turns this one off and on again after them. Windows are
    // only put back in the first case.
    disable() {
        this._dbus.unexport();
        this._dbus = null;
        if (Main.sessionMode.isLocked)
            lockedState = this._actions.saveState();
        else
            this._actions.restoreAll();
        this._actions.destroy();
        this._actions = null;
    }
}
