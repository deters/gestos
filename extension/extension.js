// SPDX-License-Identifier: GPL-2.0-or-later

import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Meta from 'gi://Meta';
import Shell from 'gi://Shell';

import {Extension, InjectionManager} from 'resource:///org/gnome/shell/extensions/extension.js';
import {ExtensionState} from 'resource:///org/gnome/shell/misc/extensionUtils.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';

import {GestureChooser} from './lib/chooser.js';
import {GestureTrail} from './lib/trail.js';
import {addSize, readSizes, removeSize, takeSize, writeSizes} from './lib/sizes.js';
import {fitSize, freeArea, hasSize, isBelievableMinimum, keepInside, restoreRect, tileWindows} from './lib/tile.js';

const OBJECT_PATH = '/io/github/deters/Gestos/Extension';
const INTERFACE_XML = `
<node>
  <interface name="io.github.deters.Gestos.Extension">
    <method name="ToggleTileWindows">
      <arg type="b" direction="out" name="changed"/>
    </method>
    <method name="CloseWindow">
      <arg type="b" direction="out" name="closed"/>
    </method>
    <method name="FocusedApp">
      <arg type="s" direction="out" name="app"/>
    </method>
    <method name="OpenApp">
      <arg type="s" direction="in" name="app"/>
      <arg type="b" direction="out" name="found"/>
    </method>
    <method name="ChooseGesture">
      <arg type="a(ssad)" direction="in" name="gestures"/>
      <arg type="i" direction="out" name="choice"/>
    </method>
  </interface>
</node>`;

// How long tiling waits for a window that was maximized to report its normal size.
const UNMAXIMIZE_TIMEOUT_MS = 1000;
// How long a window gets to take its original size back before it's closed or maximized anyway.
const RESIZE_TIMEOUT_MS = 500;
// How long a window that was asked to close stays frozen, in case its app doesn't close it.
const CLOSE_THAW_TIMEOUT_MS = 1000;
// How long a window that was asked to close has to stay open to be tiled again, as some apps take
// long to close, and would save the size of the tile if it came back before.
const CLOSE_REJOIN_MS = 5000;
// How long a window that's maximized again stays frozen, if its app doesn't take the new size.
const MAXIMIZE_THAW_TIMEOUT_MS = 500;
// How long tiled windows get to settle, after they're placed or change, before they're checked.
const TILE_CHECK_DELAY_MS = 300;
// How many times a window that keeps leaving its tile is put back, within the time below.
const MAX_TILE_RETURNS = 3;
const TILE_RETURN_PERIOD_MS = 5000;
// How long a size to undo is kept for an app that isn't opened again.
const CLOSED_SIZE_DAYS = 30;
// How long sizes given to windows wait to be saved, so a layout writes them once.
const SIZES_SAVE_DELAY_MS = 1000;
// How long the Shell gets to move windows to the monitors there are now, before they're tiled.
const MONITORS_SETTLE_MS = 500;
// How much smaller than its tile a window may be and still fill it: terminals only take whole
// lines and columns, and leave the rest of the tile empty.
const TILE_SLACK = 40;
// How often tiled windows make room for one that's being moved or resized.
const DRAG_LAYOUT_DELAY_MS = 100;

// Apps without a desktop file get a made-up app for each window, so those go by their own id.
function appIdOf(window) {
    const app = Shell.WindowTracker.get_default().get_window_app(window);
    return app && !app.is_window_backed() ? app.get_id() : window.get_wm_class();
}

function isMoving(op) {
    return op === Meta.GrabOp.MOVING || op === Meta.GrabOp.MOVING_UNCONSTRAINED ||
        op === Meta.GrabOp.KEYBOARD_MOVING;
}

// Gestos's layers over the desktop. GTK 4 can't mark them as anything but normal windows. The
// overlay is fullscreen, which isn't tiled anyway. The app's gesture chooser is maximized, which
// would otherwise make it a tile, and shows on GNOME when the extension's own card fails to open.
const UNTILED_APP_IDS = ['io.github.deters.Gestos.Chooser', 'io.github.deters.Gestos.Overlay'];

// Snapping a window to half the screen maximizes it in one direction.
function isMaximizedAtAll(window) {
    return window.get_maximize_flags() !== 0;
}

// Snapped to a side, or maximized one way: it covers part of the screen, not all of it.
function isSnapped(window) {
    const flags = window.get_maximize_flags();
    return flags !== 0 && flags !== Meta.MaximizeFlags.BOTH && !window.is_fullscreen();
}

// With workspaces only on the primary monitor, as GNOME has by default, windows on the others are
// on every workspace. They're tiled with the workspace in use.
function isOnSharedMonitor(window) {
    return Meta.prefs_get_workspaces_only_on_primary() &&
        window.get_monitor() !== global.display.get_primary_monitor();
}

// Shown on every workspace because the user chose it, not for being on a shared monitor.
function isOnAllByChoice(window) {
    return window.is_on_all_workspaces() && !isOnSharedMonitor(window);
}

// Windows that stay over the others instead of taking a place among them: ones that belong to
// another window, ones kept on top, as picture-in-picture videos are, and ones left out of the
// taskbar, as drop-down terminals are.
function floats(window) {
    return window.get_transient_for() !== null || window.is_above() || window.is_skip_taskbar();
}

// Windows that can't be resized are tiled at their own size, and the others make room for them.
function isFixedSize(window) {
    return !window.allows_resize() && !isMaximizedAtAll(window) && !window.is_fullscreen();
}

function isTileable(window) {
    return !window.minimized && !UNTILED_APP_IDS.includes(window.get_gtk_application_id()) &&
        window.get_window_type() === Meta.WindowType.NORMAL && !isOnAllByChoice(window) &&
        !floats(window) && !window.is_fullscreen();
}

// Other extensions that tile windows, which would move the same windows as this. Tiling waits
// while one of them is on.
const OTHER_TILING_EXTENSIONS = [
    'tiling-assistant@ubuntu.com',
    'tiling-assistant@leleat-on-github',
    'pop-shell@system76.com',
    'forge@jmmaranan.com',
    'paperwm@paperwm.github.com',
    'paperwm@hedning:matrix.org',
    'tilingshell@ferrarodomenico.com',
    'material-shell@papyelgringo',
];

function otherTilingIsOn() {
    return OTHER_TILING_EXTENSIONS.some(uuid =>
        Main.extensionManager.lookup(uuid)?.state === ExtensionState.ACTIVE);
}

// The smallest the app lets its window get, as frame size: the Shell reports it with the window's
// shadows, which aren't part of the frame.
function minimumFrameSize(window) {
    const [known, width, height] = window.get_min_size();
    if (!known)
        return null;
    const frame = window.get_frame_rect();
    const buffer = window.get_buffer_rect();
    return {
        width: Math.max(0, width - (buffer.width - frame.width)),
        height: Math.max(0, height - (buffer.height - frame.height)),
    };
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
    // Windows kept open as they were asked to close, waiting to join their tiling again.
    _rejoins = new Set();
    constructor(settings) {
        this._settings = settings;
        // Sizes this gave windows, with their original sizes, for each app. They're kept across
        // sessions, as an app keeps the size it saved until it's opened again.
        this._givenSizes = this._loadGivenSizes();
        this._sizesSaveId = 0;
        this._grabId = global.display.connect('grab-op-begin',
            (_display, window, op) => this._onGrabBegin(window, op));
        this._grabEndId = global.display.connect('grab-op-end',
            (_display, window) => this._onGrabEnd(window));
        this._workAreasId = global.display.connect('workareas-changed',
            () => this._onWorkAreasChanged());
        // Gestos's Preferences change the gap too, and windows follow it right away.
        this._gapId = settings.connect('changed::window-gap', () => this._onWorkAreasChanged());
        // Gestos's Preferences can tile every workspace, as they come, without tiling each one.
        this._autoId = settings.connect('changed::tile-automatically', () => this._onAutoChanged());
        // The Shell says when the session is about to end once the user confirms it.
        this._endSessionId = Gio.DBus.session.signal_subscribe(null,
            'org.gnome.SessionManager.EndSessionDialog', null,
            '/org/gnome/SessionManager/EndSessionDialog', null, Gio.DBusSignalFlags.NONE,
            (_conn, _sender, _path, _iface, signal) => {
                if (signal.startsWith('Confirmed')) {
                    // Which workspaces are tiled stays as it was, for the next session.
                    this._ending = true;
                    this.restoreAll();
                }
            });
        // Workspaces are remembered by their place, which changes as others come and go.
        this._pendingTiled = new Set();
        // Workspaces the user stopped tiling while every workspace is tiled, which stay that way.
        this._keptUntiled = new Set();
        this._workspaceAddedId = global.workspace_manager.connect('workspace-added',
            () => this.tileRemembered());
        this._workspaceRemovedId = global.workspace_manager.connect('workspace-removed',
            () => this._onWorkspaceRemoved());
        this._workspacesReorderedId = global.workspace_manager.connect('workspaces-reordered',
            () => this._saveTiled());
        // A window is mapped once it's on screen at its real size, which is when it can join.
        this._mapId = global.window_manager.connect('map',
            (_wm, actor) => this._onWindowMapped(actor.meta_window));
        // Tiled windows make room for one that's shown again, and take the room one leaves.
        this._minimizeId = global.window_manager.connect('minimize',
            (_wm, actor) => this._onMinimized(actor.meta_window));
        this._unminimizeId = global.window_manager.connect('unminimize',
            (_wm, actor) => this._onUnminimized(actor.meta_window));
        // Windows on shared monitors go with the workspace in use.
        this._activeWorkspaceId = global.workspace_manager.connect('active-workspace-changed',
            () => this._onActiveWorkspaceChanged());
        // Monitors that come and go move windows, which isn't the user placing them.
        this._monitorsSettleId = 0;
        this._monitorsId = Main.layoutManager.connect('monitors-changed',
            () => this._onMonitorsChanged());
        // Another extension that tiles windows puts tiling on hold while it's on.
        this._otherTiling = otherTilingIsOn();
        this._extensionStateId = Main.extensionManager.connect('extension-state-changed',
            () => this._onExtensionStateChanged());

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
        this._settings.disconnect(this._autoId);
        Gio.DBus.session.signal_unsubscribe(this._endSessionId);
        global.window_manager.disconnect(this._mapId);
        global.window_manager.disconnect(this._minimizeId);
        global.window_manager.disconnect(this._unminimizeId);
        global.workspace_manager.disconnect(this._activeWorkspaceId);
        Main.layoutManager.disconnect(this._monitorsId);
        Main.extensionManager.disconnect(this._extensionStateId);
        if (this._monitorsSettleId)
            GLib.source_remove(this._monitorsSettleId);
        global.workspace_manager.disconnect(this._workspaceAddedId);
        global.workspace_manager.disconnect(this._workspaceRemovedId);
        global.workspace_manager.disconnect(this._workspacesReorderedId);
        Meta.keybindings_set_custom_handler('close', null);
        this._chooser?.close();
        this._injections.clear();
        for (const tiling of this._tilings.values())
            this._stopTiling(tiling);
        this._tilings.clear();
        for (const window of [...this._originals.keys()])
            this._untrack(window);
        for (const frozen of [...this._frozen])
            frozen.release(true);
        for (const finish of [...this._rejoins])
            finish();
        if (this._sizesSaveId) {
            GLib.source_remove(this._sizesSaveId);
            this._saveGivenSizes();
        }
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
                [workspace, {windows: copy(tiling.windows), plans: tiling.plans,
                    order: [...tiling.order],
                    parked: [...tiling.parked].map(([window, {rect}]) => [window, rect])}]),
        };
    }

    // Picks up from saveState, leaving out windows that closed in between.
    loadState(saved) {
        const open = new Set(global.get_window_actors().map(actor => actor.meta_window));
        for (const [window, original] of saved.originals) {
            if (!open.has(window))
                continue;
            const tracked = Object.assign(this._track(window, original.rect), original);
            // Its size was saved as given, which it's still at, so it's the same one.
            if (tracked.size)
                takeSize(this._givenSizes, tracked.appId, tracked.size);
            this._remember(tracked);
        }
        for (const [workspace, {windows, plans, order, parked}] of saved.tilings) {
            const tiling = this._startTiling(workspace);
            tiling.plans = plans;
            for (const [window, state] of windows) {
                if (this._originals.has(window))
                    this._joinTiling(tiling, window, state);
            }
            tiling.order = (order ?? []).filter(window => tiling.windows.has(window));
            for (const [window, rect] of parked ?? []) {
                if (open.has(window))
                    this._park(tiling, window, rect, false);
            }
            if (tiling.windows.size < windows.length)
                this._layoutWhenReady(tiling);
        }
    }

    // Versions before kept one size for each app, which is moved over.
    _loadGivenSizes() {
        const oldest = Date.now() / 1000 - CLOSED_SIZE_DAYS * 24 * 60 * 60;
        const saved = this._settings.get_value('given-sizes').deepUnpack();
        const older = this._settings.get_value('closed-sizes').deepUnpack();
        for (const [appId, entry] of Object.entries(older))
            saved[appId] = [...saved[appId] ?? [], entry];
        const sizes = readSizes(saved, oldest);
        if (Object.keys(older).length > 0) {
            this._settings.set_value('given-sizes', new GLib.Variant('a{sa(iiiix)}', writeSizes(sizes)));
            this._settings.reset('closed-sizes');
        }
        return sizes;
    }

    _saveGivenSizes() {
        this._sizesSaveId = 0;
        this._settings.set_value('given-sizes',
            new GLib.Variant('a{sa(iiiix)}', writeSizes(this._givenSizes)));
    }

    _scheduleSizesSave() {
        if (this._sizesSaveId)
            return;
        this._sizesSaveId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, SIZES_SAVE_DELAY_MS, () => {
            this._saveGivenSizes();
            return GLib.SOURCE_REMOVE;
        });
    }

    // Keeps the size this gave a window, with its original size, as its app may save it.
    _remember(original) {
        if (!original.rect || !original.size)
            return;
        const now = Math.floor(Date.now() / 1000);
        const size = {width: original.size.width, height: original.size.height};
        const rect = {width: original.rect.width, height: original.rect.height};
        if (original.entry) {
            Object.assign(original.entry, {rect, size, time: now});
        } else {
            original.entry = {rect, size, time: now};
            addSize(this._givenSizes, original.appId, original.entry);
        }
        this._scheduleSizesSave();
    }

    // The window is back at its original size, or closed at it, so there's nothing to undo.
    _forgetGiven(original) {
        if (!original?.entry)
            return;
        removeSize(this._givenSizes, original.appId, original.entry);
        original.entry = null;
        this._scheduleSizesSave();
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
            this._untile(workspace);
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
                    'notify::fullscreen', 'notify::on-all-workspaces', 'notify::above'].map(signal =>
                    window.connect(signal, () => this._onStateChanged(window))),
            ],
            entry: null,
            changedMeanwhile: false,
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
        const original = this._originals.get(window);
        original.size = copyRect(rect);
        this._remember(original);
        window.move_resize_frame(true, rect.x, rect.y, rect.width, rect.height);
    }

    // Gives a window its original size back, and stops watching it.
    _restoreOriginal(window) {
        const original = this._originals.get(window);
        if (original?.rect && !original.closing) {
            const {x, y, width, height} =
                keepInside(original.rect, window.get_work_area_current_monitor());
            window.move_resize_frame(true, x, y, width, height);
            this._forgetGiven(original);
        }
        this._untrack(window);
    }

    // Gives a window its original size back, and calls done once the app has taken it, or after a
    // while if it doesn't.
    _restoreOriginalThen(window, original, done) {
        original.fixing = true;
        original.changedMeanwhile = false;
        // Changes this makes itself aren't the user changing their mind.
        original.ownChange = true;
        if (isMaximizedAtAll(window))
            window.unmaximize();
        if (window.is_fullscreen())
            window.unmake_fullscreen();
        original.ownChange = false;
        if (!original.rect) {
            done();
            return;
        }
        // No bigger than the screen, as that's the most it can take, so it's not waited for longer.
        const area = window.get_work_area_current_monitor();
        const rect = keepInside({...original.rect, ...fitSize(original.rect, area)}, area);
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
    // its picture goes with it. Thawing also shows a window that was hidden while frozen. `ended`
    // is called once it's thawed or gone, with whether it was thawed. What's returned can thaw it
    // early with `release(true)`.
    _freeze(window) {
        const actor = window.get_compositor_private();
        if (!actor) {
            return (_onResize, _timeout, ended = null) => {
                ended?.(false);
                return {release: () => {}};
            };
        }
        actor.freeze();
        return (onResize, timeout, ended = null) => {
            const frozen = {ids: [], timeoutId: 0};
            frozen.release = thaw => {
                for (const id of frozen.ids)
                    window.disconnect(id);
                if (frozen.timeoutId)
                    GLib.source_remove(frozen.timeoutId);
                this._frozen.delete(frozen);
                frozen.release = () => {};
                if (thaw) {
                    actor.thaw();
                    actor.opacity = 255;
                }
                ended?.(thaw);
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
            return frozen;
        };
    }

    // Takes a window out of tiling, and gives the others the room it had.
    _leave(window) {
        const [, tiling] = this._findTiling(window);
        if (!tiling)
            return null;
        this._dropFromTiling(tiling, window);
        this._layoutWhenReady(tiling);
        return tiling;
    }

    // The window is about to close. Returns false to let it close as it is.
    _closeAtOriginalSize(window, close) {
        const original = this._originals.get(window);
        if (!original?.rect || original.fixing || original.closing)
            return false;
        const tiling = this._leave(window);
        // It's gone at once, as the others take its room at once, and takes its original size out
        // of sight before it closes.
        const thawLater = this._freeze(window);
        const actor = window.get_compositor_private();
        if (actor)
            actor.opacity = 0;
        this._restoreOriginalThen(window, original, () => {
            this._forgetGiven(original);
            this._untrack(window);
            // If its app keeps it open, as to ask whether to save first, it's shown again: at
            // once if it opens a window to ask in, or once thawed.
            let frozen = null;
            let dialog = null;
            const dialogId = global.display.connect('window-created', (_display, created) => {
                if (created.get_transient_for() !== window)
                    return;
                dialog = created;
                frozen?.release(true);
            });
            frozen = thawLater(false, CLOSE_THAW_TIMEOUT_MS, thawed => {
                global.display.disconnect(dialogId);
                if (thawed && tiling)
                    this._rejoinIfKeptOpen(tiling, window, dialog);
            });
            close();
        });
        return true;
    }

    // A window its app kept open as it was asked to close joins its tiling again: once the window
    // it asked in is closed, or after a while.
    _rejoinIfKeptOpen(tiling, window, dialog) {
        let closing = false;
        let timeoutId = 0;
        const ids = [];
        const finish = () => {
            for (const [object, id] of ids)
                object.disconnect(id);
            ids.length = 0;
            if (timeoutId)
                GLib.source_remove(timeoutId);
            timeoutId = 0;
            this._rejoins.delete(finish);
        };
        this._rejoins.add(finish);
        const rejoin = () => {
            finish();
            if (closing || this._tilings.get(tiling.workspace) !== tiling ||
                this._findTiling(window)[1] || !isTileable(window))
                return;
            this._addToTiling(tiling, window);
            this._layoutWhenReady(tiling);
        };
        ids.push([window, window.connect('unmanaging', () => {
            closing = true;
            finish();
        })]);
        if (dialog) {
            // Once the Shell is done with the dialog, and the window's app has had a moment to
            // close it as the user answered.
            ids.push([dialog, dialog.connect('unmanaged', () => {
                timeoutId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, CLOSE_THAW_TIMEOUT_MS, () => {
                    timeoutId = 0;
                    rejoin();
                    return GLib.SOURCE_REMOVE;
                });
            })]);
        } else {
            timeoutId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, CLOSE_REJOIN_MS, () => {
                timeoutId = 0;
                rejoin();
                return GLib.SOURCE_REMOVE;
            });
        }
    }

    _onUnmanaged(window) {
        const original = this._originals.get(window);
        // Its app saved the size this gave it, so the next window it opens at that size gets the
        // original size instead. That stays remembered.
        if (original.fixing)
            this._forgetGiven(original);
        else
            this._remember(original);
        this._leave(window);
        this._untrack(window);
    }

    // Maximized, fullscreen and snapped windows can't be tiled, so they're set aside, and the others
    // take their room until they're back to normal. A maximized or fullscreen window gets its
    // original size back first, so that's its normal size. A snapped window can't be snapped again
    // by the Shell, so it gets its original size back once the snap is undone instead.
    _onStateChanged(window) {
        const original = this._originals.get(window);
        // One maximized again while it gets its original size back, after this restored it, was
        // toggled once more by the user, who changed their mind. Fullscreen isn't, as apps can
        // ask for it again themselves.
        if (original.fixing && !original.ownChange && isMaximizedAtAll(window))
            original.changedMeanwhile = true;
        if (original.fixing || original.closing)
            return;
        const [, tiling] = this._findTiling(window);
        // Still being restored as it joins tiling.
        if (tiling && !tiling.windows.get(window).tile)
            return;

        // One on all workspaces belongs to no one tiling, and one that floats over the others has no
        // place among them, so they're set aside at their original size until that changes.
        if (isOnAllByChoice(window) || floats(window)) {
            this._setAsideFrom(tiling, window, original)?.settle();
            this._restoreOriginal(window);
            return;
        }

        const flags = window.get_maximize_flags();
        if (window.is_fullscreen() || flags === Meta.MaximizeFlags.BOTH) {
            const fullscreen = window.is_fullscreen();
            // The others only take its room once it's shown maximized, over them, as it takes a
            // moment to get there.
            const aside = this._setAsideFrom(tiling, window, original, false);
            // Its original size is taken where it can't be seen, until it's maximized again.
            const thawLater = this._freeze(window);
            this._restoreOriginalThen(window, original, () => {
                const changedMeanwhile = original.changedMeanwhile;
                this._forgetGiven(original);
                this._untrack(window);
                // Undone in the meantime, as by a second double-click on its title, so it's not
                // maximized again, and joins tiling as it is.
                if (changedMeanwhile && !fullscreen) {
                    thawLater(false, 0);
                    aside?.settle();
                    if (isMaximizedAtAll(window))
                        window.unmaximize();
                    aside?.check();
                    return;
                }
                // Watched before it's maximized, as an X11 window takes its new size at once.
                thawLater(true, MAXIMIZE_THAW_TIMEOUT_MS, () => {
                    if (aside && this._tilings.get(tiling.workspace) === tiling)
                        this._layoutWhenReady(tiling);
                });
                if (fullscreen)
                    window.make_fullscreen();
                else
                    window.maximize();
                aside?.settle();
            });
        } else if (flags !== 0) {
            if (!original.snapped) {
                this._setAsideFrom(tiling, window, original)?.settle();
                original.snapped = true;
            }
        } else if (original.snapped) {
            const frame = window.get_frame_rect();
            if (original.rect)
                window.move_resize_frame(true, frame.x, frame.y, original.rect.width, original.rect.height);
            this._forgetGiven(original);
            this._untrack(window);
        }
    }

    // Takes a tiled window out to set it aside, with its original size to join again at. The others
    // take its room now, or once `_layoutWhenReady` is called if not.
    _setAsideFrom(tiling, window, original, makeRoom = true) {
        if (!tiling || !original.rect)
            return null;
        if (makeRoom)
            this._leave(window);
        else
            this._dropFromTiling(tiling, window);
        this._setAside(tiling, window, original.rect);
        return tiling.aside.get(window);
    }

    // A tiled window moved to another workspace leaves its tiling, and joins the other one if
    // that's tiled, or gets its original size back if it isn't.
    _onWorkspaceChanged(window) {
        const [workspace, tiling] = this._findTiling(window);
        const target = window.get_workspace();
        // A window on all workspaces has none of its own, and stays where it was tiled.
        if (!tiling || !target || workspace === target || this._originals.get(window).closing)
            return;
        this._dropFromTiling(tiling, window);
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
        if (!appId || window.get_window_type() !== Meta.WindowType.NORMAL)
            return null;
        const closed = takeSize(this._givenSizes, appId, window.get_frame_rect());
        if (closed)
            this._scheduleSizesSave();
        return closed;
    }

    // A tiled workspace keeps the order the user touched its windows in, the last one first.
    _startTiling(workspace) {
        const tiling = {workspace, windows: new Map(), plans: [], waits: [], timeoutId: 0,
            checkId: 0, joining: new Map(), order: [], dragId: 0, parked: new Map(),
            aside: new Map()};
        // Windows moved here from a workspace that isn't tiled join. New windows join once they're
        // on screen, as the Shell places them before that, which would undo their tile.
        tiling.addedId = workspace.connect('window-added', (_workspace, window) => {
            if (this._findTiling(window)[1] || !window.mapped || !isTileable(window))
                return;
            this._joinLater(tiling, window);
        });
        this._tilings.set(workspace, tiling);
        return tiling;
    }

    // A window on all workspaces, as on a monitor other than the primary one, comes back to a single
    // workspace as it closes. It can't be resized then without crashing the Shell, so windows that
    // come here join a moment later, if they're still open.
    _joinLater(tiling, window) {
        if (tiling.joining.has(window))
            return;
        const join = {
            unmanagedId: window.connect('unmanaged', () => this._stopJoining(tiling, window)),
            idleId: GLib.idle_add(GLib.PRIORITY_DEFAULT_IDLE, () => {
                join.idleId = 0;
                this._stopJoining(tiling, window);
                if (window.get_workspace() !== tiling.workspace || this._findTiling(window)[1] ||
                    !window.mapped || !isTileable(window))
                    return GLib.SOURCE_REMOVE;
                this._addToTiling(tiling, window);
                this._layoutWhenReady(tiling);
                return GLib.SOURCE_REMOVE;
            }),
        };
        tiling.joining.set(window, join);
    }

    _stopJoining(tiling, window) {
        const join = tiling.joining.get(window);
        if (!join)
            return;
        tiling.joining.delete(window);
        window.disconnect(join.unmanagedId);
        if (join.idleId)
            GLib.source_remove(join.idleId);
    }

    _stopTiling(tiling) {
        this._stopWaiting(tiling);
        this._stopTileCheck(tiling);
        this._stopDragLayout(tiling);
        for (const window of [...tiling.joining.keys()])
            this._stopJoining(tiling, window);
        for (const window of [...tiling.parked.keys()])
            this._forgetParked(tiling, window);
        for (const window of [...tiling.aside.keys()])
            this._forgetAside(tiling, window);
        tiling.workspace.disconnect(tiling.addedId);
        for (const [window, state] of tiling.windows)
            this._unwatchTile(window, state);
    }

    _stopTileCheck(tiling) {
        if (tiling.checkId) {
            GLib.source_remove(tiling.checkId);
            tiling.checkId = 0;
        }
    }

    // A tiled window is watched, so that one that leaves its tile is found: apps can resize their
    // windows themselves, or turn out bigger than they said they could get.
    _joinTiling(tiling, window, state) {
        state.watchIds = ['position-changed', 'size-changed'].map(signal =>
            window.connect(signal, () => this._onTileChanged(tiling, window)));
        tiling.windows.set(window, state);
    }

    _unwatchTile(window, state) {
        for (const id of state.watchIds ?? [])
            window.disconnect(id);
        state.watchIds = [];
    }

    _dropFromTiling(tiling, window) {
        const state = tiling.windows.get(window);
        if (state)
            this._unwatchTile(window, state);
        tiling.windows.delete(window);
        tiling.order = tiling.order.filter(w => w !== window);
    }

    _onTileChanged(tiling, window) {
        if (this._originals.get(window)?.closing)
            return;
        if (this._grabbed !== window)
            this._scheduleTileCheck(tiling);
        else
            this._scheduleDragLayout(tiling);
    }

    _scheduleTileCheck(tiling) {
        if (tiling.checkId)
            return;
        tiling.checkId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, TILE_CHECK_DELAY_MS, () => {
            tiling.checkId = 0;
            this._checkTiles(tiling);
            return GLib.SOURCE_REMOVE;
        });
    }

    // Adds a window to the tiling. A maximized or snapped one is restored first, and its normal
    // size is only known once the app has resized, so it has no original size until then. A window
    // opened again after closing at a size this gave it gets its original size, at the place it
    // opened at now.
    _addToTiling(tiling, window, reopened = null) {
        const maximized = isMaximizedAtAll(window);
        const frame = window.get_frame_rect();
        // One that has no size yet gets its original size once it does, as a maximized one does.
        let rect = maximized || frame.width === 0 || frame.height === 0 ? null : frame;
        if (reopened)
            rect = {x: frame.x, y: frame.y, width: reopened.rect.width, height: reopened.rect.height};
        const original = this._track(window, rect);
        original.snapped = false;
        this._joinTiling(tiling, window, {tile: null});
        // A window that joins is the one the user touched last, as it's just been opened or shown.
        tiling.order.unshift(window);
        if (maximized) {
            window.unmaximize();
            // An X11 window takes its normal size at once, so there's nothing to wait for.
            if (window.get_client_type() === Meta.WindowClientType.X11)
                original.rect ??= copyRect(window.get_frame_rect());
        }
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

    // Fits the windows of each monitor together at their original sizes and places, the one the
    // user touched last first. One being moved or resized is where the user has it, and the others
    // make room for it, but only once it's left there do they get minimized for lack of room, or
    // come back where there's room again. Those come after the others, so they only take room no
    // other window needs.
    _layout(tiling, dragged = null) {
        const gap = this._settings.get_int('window-gap');
        const parked = dragged ? []
            : [...tiling.parked.keys()].filter(window => window.get_workspace() === tiling.workspace);
        const windows = [...tiling.order.filter(window => tiling.windows.has(window)), ...parked];
        tiling.plans = this._byMonitor(windows).map(group => {
            const workArea = group[0].get_work_area_current_monitor();
            // Snapped windows keep their side, and the others are tiled in the room left.
            const area = freeArea(workArea, this._snappedOn(tiling, group[0].get_monitor()));
            const rects = group.map(window => window === dragged
                ? copyRect(window.get_frame_rect())
                : tiling.parked.get(window)?.rect ?? this._originals.get(window).rect);
            return {windows: group, rects: tileWindows(rects, area, gap,
                this._minimumsOf(tiling, group, workArea))};
        });
        for (const plan of tiling.plans) {
            plan.windows.forEach((window, i) => {
                const rect = plan.rects[i];
                if (tiling.parked.has(window)) {
                    if (rect)
                        this._unpark(tiling, window, rect);
                    return;
                }
                const state = tiling.windows.get(window);
                // One that's no longer on screen is going away, and can't be resized.
                if (window === dragged || !state || this._originals.get(window).closing ||
                    !window.get_compositor_private())
                    return;
                if (!rect) {
                    if (!dragged)
                        this._minimizeForRoom(tiling, window);
                    return;
                }
                state.tile = rect;
                state.monitor = window.get_monitor();
                const frame = window.get_frame_rect();
                if (frame.x !== rect.x || frame.y !== rect.y || frame.width !== rect.width ||
                    frame.height !== rect.height)
                    this._resizeTo(window, rect);
            });
        }
        this._stopTileCheck(tiling);
        this._scheduleTileCheck(tiling);
    }

    // Where the workspace's snapped windows on a monitor are.
    _snappedOn(tiling, monitor) {
        return tiling.workspace.list_windows()
            .filter(window => !window.minimized && window.get_monitor() === monitor &&
                isSnapped(window) && !tiling.windows.has(window))
            .map(window => window.get_frame_rect());
    }

    _byMonitor(windows) {
        const monitors = new Map();
        for (const window of windows) {
            const monitor = window.get_monitor();
            monitors.set(monitor, [...monitors.get(monitor) ?? [], window]);
        }
        return [...monitors.values()];
    }

    // Windows are planned with how small each app lets its window get, so that none has to move
    // again. One that turned out not to get as small as it said keeps what it showed instead. One
    // that says it can't get smaller than most of the screen gets as small as the others. One that
    // can't be resized at all keeps its size.
    _minimumsOf(tiling, group, area) {
        const minimums = new Map();
        group.forEach((window, i) => {
            if (isFixedSize(window)) {
                const frame = window.get_frame_rect();
                minimums.set(i, {width: frame.width, height: frame.height});
                return;
            }
            const believable = size => size && isBelievableMinimum(size, area) ? size : null;
            const reported = believable(minimumFrameSize(window));
            const seen = believable(tiling.windows.get(window)?.minimum);
            if (reported || seen) {
                minimums.set(i, {
                    width: Math.max(reported?.width ?? 0, seen?.width ?? 0),
                    height: Math.max(reported?.height ?? 0, seen?.height ?? 0),
                });
            }
        });
        return minimums;
    }

    // A window there's no room for leaves, at its original size, as it would come back. It's
    // remembered, to come back once there's room for it.
    _minimizeForRoom(tiling, window) {
        const rect = copyRect(this._originals.get(window).rect);
        this._dropFromTiling(tiling, window);
        this._restoreOriginal(window);
        this._park(tiling, window, rect, true);
        window.minimize();
    }

    // Windows minimized for lack of room, the one minimized last first, as it was touched last.
    _park(tiling, window, rect, first) {
        const entry = {rect, unmanagedId: window.connect('unmanaged',
            () => this._forgetParked(tiling, window))};
        tiling.parked = first ? new Map([[window, entry], ...tiling.parked])
            : new Map([...tiling.parked, [window, entry]]);
    }

    _forgetParked(tiling, window) {
        const entry = tiling.parked.get(window);
        if (!entry)
            return;
        window.disconnect(entry.unmanagedId);
        tiling.parked.delete(window);
    }

    // A window minimized for lack of room comes back as the one touched longest ago, at the place
    // there's room for it.
    _unpark(tiling, window, rect) {
        const {rect: original} = tiling.parked.get(window);
        this._forgetParked(tiling, window);
        this._addToTiling(tiling, window);
        this._originals.get(window).rect = original;
        tiling.order = [...tiling.order.filter(w => w !== window), window];
        tiling.windows.get(window).tile = rect;
        this._resizeTo(window, rect);
        window.unminimize();
    }

    // A maximized, fullscreen, snapped, floating or all-workspaces window is set aside, with the size
    // it had while tiled, and joins again as the one touched last once it's back to normal, on the
    // tiled workspace it's on then. One that opened maximized has no such size, and joins at the
    // size it takes then. It's only watched once it's settled, as getting its original size back
    // first restores it for a moment.
    _setAside(tiling, window, rect) {
        if (tiling.aside.has(window))
            return;
        const aside = {rect, settled: false, closing: false, checkId: 0};
        const join = () => {
            if (!aside.settled || aside.closing || this._originals.get(window)?.fixing ||
                isMaximizedAtAll(window) || window.is_fullscreen() || isOnAllByChoice(window) ||
                floats(window))
                return;
            this._forgetAside(tiling, window);
            // It joins the workspace it's on now, which isn't the one it left when it was on all.
            const workspace = window.get_workspace();
            const joining = workspace && this._tilings.get(workspace);
            if (!joining || window.minimized || this._findTiling(window)[1] || !isTileable(window))
                return;
            this._addToTiling(joining, window);
            const original = this._originals.get(window);
            if (aside.rect)
                original.rect = copyRect(aside.rect);
            // Still at the size it had maximized, until its app takes its normal size, which an
            // X11 window has already.
            else if (window.get_client_type() !== Meta.WindowClientType.X11)
                original.rect = null;
            this._layoutWhenReady(joining);
        };
        // Once the Shell is done changing the window, as resizing windows in the middle of that
        // can crash it.
        const check = () => {
            if (!aside.checkId) {
                aside.checkId = GLib.idle_add(GLib.PRIORITY_DEFAULT, () => {
                    aside.checkId = 0;
                    join();
                    return GLib.SOURCE_REMOVE;
                });
            }
        };
        aside.ids = [
            ...['notify::maximized-horizontally', 'notify::maximized-vertically',
                'notify::fullscreen', 'notify::on-all-workspaces', 'notify::above'].map(signal =>
                window.connect(signal, check)),
            // A closing window can stop being maximized or fullscreen as it goes, and mustn't
            // join, as it can't be resized anymore.
            window.connect('unmanaging', () => {
                aside.closing = true;
            }),
            // A snapped one only has its side once the app takes that size, and the others are
            // tiled beside it again then.
            ...['size-changed', 'position-changed'].map(signal => window.connect(signal, () => {
                if (aside.settled && !aside.closing && isSnapped(window))
                    this._layoutWhenReady(tiling);
            })),
            // One that's minimized or moved away leaves its side to the others.
            ...['notify::minimized', 'workspace-changed'].map(signal => window.connect(signal, () => {
                if (aside.settled && !aside.closing)
                    this._layoutWhenReady(tiling);
            })),
            // Gone, so the others take its room: a snapped one's side, or the room of one that was
            // still being maximized.
            window.connect('unmanaged', () => {
                this._forgetAside(tiling, window);
                this._layoutWhenReady(tiling);
            }),
        ];
        aside.settle = () => {
            aside.settled = true;
        };
        aside.check = check;
        tiling.aside.set(window, aside);
    }

    _forgetAside(tiling, window) {
        const aside = tiling.aside.get(window);
        if (!aside)
            return;
        for (const id of aside.ids)
            window.disconnect(id);
        if (aside.checkId)
            GLib.source_remove(aside.checkId);
        tiling.aside.delete(window);
    }

    _onMinimized(window) {
        const [, tiling] = this._findTiling(window);
        if (!tiling)
            return;
        this._dropFromTiling(tiling, window);
        this._restoreOriginal(window);
        this._layout(tiling);
    }

    // One the user shows again joins as the one they touched last, wherever it was minimized.
    _onUnminimized(window) {
        for (const tiling of this._tilings.values())
            this._forgetParked(tiling, window);
        const workspace = window.get_workspace();
        const tiling = workspace && this._tilings.get(workspace);
        if (!tiling || tiling.windows.has(window) || !isTileable(window))
            return;
        this._addToTiling(tiling, window);
        this._layoutWhenReady(tiling);
    }

    _scheduleDragLayout(tiling) {
        if (tiling.dragId)
            return;
        tiling.dragId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, DRAG_LAYOUT_DELAY_MS, () => {
            tiling.dragId = 0;
            if (this._grabbed && tiling.windows.has(this._grabbed))
                this._layout(tiling, this._grabbed);
            return GLib.SOURCE_REMOVE;
        });
    }

    _stopDragLayout(tiling) {
        if (tiling.dragId) {
            GLib.source_remove(tiling.dragId);
            tiling.dragId = 0;
        }
    }

    // Checks that tiled windows are where they were put. One that's bigger than it was given can't
    // get that small, which the others make room for. One that was moved without changing size
    // stays where it was moved to. One that changed size goes back a few times, and after that
    // keeps what its app wants, as its original size.
    _checkTiles(tiling) {
        // Windows the Shell moves to the monitors there are now are placed once it's done.
        if (this._monitorsSettleId)
            return;
        const now = Date.now();
        let changed = false;
        for (const plan of tiling.plans) {
            plan.windows.forEach(window => {
                const state = tiling.windows.get(window);
                const original = this._originals.get(window);
                if (!state?.tile || !original || original.closing || this._grabbed === window)
                    return;
                const frame = window.get_frame_rect();
                const tile = state.tile;
                const area = window.get_work_area_current_monitor();
                // A bit smaller where it was put fills its tile as well as it can, as a terminal
                // does with whole lines and columns.
                if (Math.abs(frame.x - tile.x) <= 1 && Math.abs(frame.y - tile.y) <= 1 &&
                    frame.width <= tile.width &&
                    frame.height <= tile.height && tile.width - frame.width < TILE_SLACK &&
                    tile.height - frame.height < TILE_SLACK)
                    return;
                const wider = frame.width > tile.width + 1;
                const taller = frame.height > tile.height + 1;
                if ((wider || taller) && tile.width < original.rect.width + 1 &&
                    tile.height < original.rect.height + 1) {
                    const minimum = fitSize({
                        width: Math.max(state.minimum?.width ?? 0, wider ? frame.width : 0),
                        height: Math.max(state.minimum?.height ?? 0, taller ? frame.height : 0),
                    }, area);
                    if (minimum.width !== state.minimum?.width ||
                        minimum.height !== state.minimum?.height) {
                        state.minimum = minimum;
                        changed = true;
                        return;
                    }
                }
                const moved = Math.abs(frame.x - tile.x) > 1 || Math.abs(frame.y - tile.y) > 1;
                if (!moved && hasSize(frame, tile))
                    return;
                // Moved as it was, as the Shell's shortcuts for moving a window to a side do, or to
                // another screen, which can make it smaller to fit: the user put it there, so it
                // stays, at its original size as far as it fits, and the others make room for it.
                if (hasSize(frame, tile) ||
                    (state.monitor !== undefined && window.get_monitor() !== state.monitor)) {
                    tiling.order = [window, ...tiling.order.filter(w => w !== window)];
                    original.rect = keepInside({x: frame.x, y: frame.y,
                        ...fitSize(original.rect, area)}, area);
                    state.returns = [];
                    changed = true;
                    return;
                }
                state.returns = (state.returns ?? []).filter(t => now - t < TILE_RETURN_PERIOD_MS);
                if (state.returns.length >= MAX_TILE_RETURNS) {
                    if (!hasSize(frame, original.rect) || moved) {
                        original.rect = copyRect(frame);
                        state.returns = [];
                        changed = true;
                    }
                    return;
                }
                state.returns.push(now);
                this._resizeTo(window, tile);
            });
        }
        // Smallest sizes only grow, up to the screen, and an app's own size is only taken after it
        // keeps it, so laying out again for them comes to an end.
        if (changed)
            this._layout(tiling);
    }

    // Panels and docks change the room windows have, and so does the gap.
    _onWorkAreasChanged() {
        if (this._monitorsSettleId)
            return;
        for (const tiling of this._tilings.values())
            this._layout(tiling);
    }

    // A monitor that's plugged in or out moves windows, and numbers the monitors again, which isn't
    // the user moving them. Once the Shell is done, they're tiled where they are, keeping what
    // the user did before.
    _onMonitorsChanged() {
        if (this._monitorsSettleId)
            GLib.source_remove(this._monitorsSettleId);
        this._monitorsSettleId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, MONITORS_SETTLE_MS, () => {
            this._monitorsSettleId = 0;
            for (const tiling of this._tilings.values()) {
                for (const state of tiling.windows.values())
                    state.monitor = undefined;
                this._layout(tiling);
            }
            return GLib.SOURCE_REMOVE;
        });
    }

    // Windows on shared monitors are tiled with the workspace in use, so they go to its tiling, in
    // the order they had, or get their original size back if it isn't tiled.
    _onActiveWorkspaceChanged() {
        const active = global.workspace_manager.get_active_workspace();
        const target = this._tilings.get(active);
        let moved = false;
        for (const tiling of this._tilings.values()) {
            if (tiling === target)
                continue;
            const shared = tiling.order.filter(window => isOnSharedMonitor(window));
            for (const window of shared) {
                moved = true;
                const state = tiling.windows.get(window);
                this._dropFromTiling(tiling, window);
                if (target) {
                    this._joinTiling(target, window, state);
                    target.order.push(window);
                } else {
                    this._restoreOriginal(window);
                }
            }
            for (const [window, {rect}] of [...tiling.parked]) {
                if (!isOnSharedMonitor(window))
                    continue;
                this._forgetParked(tiling, window);
                if (target)
                    this._park(target, window, rect, false);
            }
        }
        if (!target)
            return;
        for (const window of active.list_windows()) {
            if (!isOnSharedMonitor(window) || !isTileable(window) || this._findTiling(window)[1] ||
                [...this._tilings.values()].some(tiling => tiling.aside.has(window)))
                continue;
            // A maximized one stays that way, as one that opens maximized does.
            if (isMaximizedAtAll(window)) {
                this._setAside(target, window, null);
                target.aside.get(window).settle();
            } else {
                this._addToTiling(target, window);
            }
            moved = true;
        }
        if (moved)
            this._layoutWhenReady(target);
    }

    // One that opens maximized or snapped stays that way, set aside until it's back to normal. One
    // that opens again at a size this gave it is maximized by the Shell only for being that big,
    // and joins.
    _onWindowMapped(window) {
        const reopened = this._takeClosedSize(window);
        const workspace = window.get_workspace();
        const tiling = workspace && this._tilings.get(workspace);
        if (tiling && !tiling.windows.has(window) && isTileable(window)) {
            if (isMaximizedAtAll(window) && !reopened) {
                this._setAside(tiling, window, null);
                tiling.aside.get(window).settle();
                if (isSnapped(window))
                    this._layoutWhenReady(tiling);
                return;
            }
            this._addToTiling(tiling, window, reopened);
            this._layoutWhenReady(tiling);
        } else if (reopened) {
            this._resizeReopened(window, reopened);
        }
    }

    // Puts every tiled window on the workspace back at its original size.
    _untile(workspace) {
        const tiling = this._tilings.get(workspace);
        this._tilings.delete(workspace);
        this._stopTiling(tiling);
        for (const window of tiling.windows.keys())
            this._restoreOriginal(window);
    }

    _findTiling(window) {
        for (const [workspace, tiling] of this._tilings) {
            if (tiling.windows.has(window))
                return [workspace, tiling];
        }
        return [null, null];
    }

    // A tiled window keeps the size and place the user left it at, as its own.
    _onGrabEnd(window) {
        this._grabbed = null;
        const [, tiling] = this._findTiling(window);
        if (!tiling)
            return;
        this._stopDragLayout(tiling);
        this._originals.get(window).rect = copyRect(window.get_frame_rect());
        this._layout(tiling);
    }

    // The tiled window the user takes becomes the one they touched last. One that's moved gets its
    // original size back under the pointer, and the others make room for it as it goes.
    _onGrabBegin(window, op) {
        // What the user does to a window isn't undone while they do it.
        this._grabbed = window;
        const [, tiling] = this._findTiling(window);
        if (!tiling)
            return;
        tiling.order = [window, ...tiling.order.filter(w => w !== window)];
        const original = this._originals.get(window);
        const frame = window.get_frame_rect();
        if (isMoving(op) && original?.rect && !hasSize(frame, original.rect)) {
            const [pointerX] = global.get_pointer();
            const {x, y, width, height} = restoreRect(frame, original.rect, pointerX);
            window.move_resize_frame(true, x, y, width, height);
        }
        this._scheduleDragLayout(tiling);
    }

    /**
     * Tiles the windows on the current workspace, or puts them back if it's tiled.
     *
     * Windows keep their original sizes, and are moved just enough not to overlap. They only shrink
     * when they don't fit otherwise, and the ones touched longest ago are minimized when even that
     * isn't enough, until there's room for them again. Moving or resizing a window makes the
     * others make room for it. Windows that open, close, or move in or out while it's tiled join or
     * leave, and ones that are maximized, made fullscreen or snapped are set aside until they're
     * back to normal. It stays tiled until this is used again, also in the sessions after this.
     *
     * @returns {boolean} whether any window changed
     */
    ToggleTileWindows() {
        if (this._otherTiling)
            return false;
        const workspace = global.workspace_manager.get_active_workspace();
        let changed = true;
        if (this._tilings.has(workspace)) {
            this._untile(workspace);
            this._keptUntiled.add(workspace);
        } else {
            changed = this._tile(workspace);
            this._keptUntiled.delete(workspace);
        }
        this._saveTiled();
        return changed;
    }

    // A workspace with no windows can be tiled too, for the windows that open there next.
    // Windows join from the bottom up, so the top one ends up as the one touched last. Windows on
    // shared monitors only join the workspace in use.
    _tile(workspace) {
        const active = global.workspace_manager.get_active_workspace();
        const windows = global.display.sort_windows_by_stacking(workspace.list_windows()
            .filter(window => isTileable(window) &&
                (workspace === active || !isOnSharedMonitor(window))));
        const tiling = this._startTiling(workspace);
        for (const window of windows)
            this._addToTiling(tiling, window);
        this._layoutWhenReady(tiling);
        return windows.length > 0;
    }

    _workspaces() {
        const manager = global.workspace_manager;
        return Array.from({length: manager.get_n_workspaces()},
            (_, i) => manager.get_workspace_by_index(i));
    }

    // Remembers which workspaces are tiled, for the next session, and for when this is turned on
    // again. Ones that didn't exist yet stay remembered.
    _saveTiled() {
        if (this._ending)
            return;
        const indices = new Set(this._pendingTiled);
        this._workspaces().forEach((workspace, i) => {
            if (this._tilings.has(workspace))
                indices.add(i);
        });
        this._settings.set_value('tiled-workspaces',
            new GLib.Variant('ai', [...indices].sort((a, b) => a - b)));
    }

    /**
     * Tiles the workspaces that were tiled before this was turned off or the session ended. Ones
     * that don't exist yet are tiled once they do.
     */
    tileRemembered() {
        if (!this._started) {
            this._started = true;
            this._pendingTiled = new Set(this._settings.get_value('tiled-workspaces').deepUnpack());
        }
        if (this._otherTiling)
            return;
        const workspaces = this._workspaces();
        for (const index of [...this._pendingTiled]) {
            if (index >= workspaces.length)
                continue;
            this._pendingTiled.delete(index);
            if (!this._tilings.has(workspaces[index]))
                this._tile(workspaces[index]);
        }
        if (this._settings.get_boolean('tile-automatically'))
            this._tileAll();
    }

    // Every workspace not tiled yet is tiled, except ones the user stopped tiling. Those are tiled
    // again in the next session.
    _tileAll() {
        for (const workspace of this._workspaces()) {
            if (!this._tilings.has(workspace) && !this._keptUntiled.has(workspace))
                this._tile(workspace);
        }
        this._saveTiled();
    }

    // Turned on, it tiles every workspace. Turned off, it puts them all back.
    _onAutoChanged() {
        this._keptUntiled.clear();
        if (this._otherTiling)
            return;
        if (this._settings.get_boolean('tile-automatically')) {
            this._tileAll();
            return;
        }
        for (const workspace of [...this._tilings.keys()])
            this._untile(workspace);
        this._saveTiled();
    }

    // While another extension tiles windows, this puts its own back, and tiles them again once it's
    // off, as they were.
    _onExtensionStateChanged() {
        const on = otherTilingIsOn();
        if (on === this._otherTiling)
            return;
        this._otherTiling = on;
        if (!on) {
            this.tileRemembered();
            return;
        }
        this._workspaces().forEach((workspace, i) => {
            if (this._tilings.has(workspace))
                this._pendingTiled.add(i);
        });
        for (const workspace of [...this._tilings.keys()])
            this._untile(workspace);
    }

    // A workspace GNOME removes, as it does with empty ones, isn't tiled anymore, and the places
    // of the ones after it change.
    _onWorkspaceRemoved() {
        const open = new Set(this._workspaces());
        for (const workspace of [...this._keptUntiled]) {
            if (!open.has(workspace))
                this._keptUntiled.delete(workspace);
        }
        for (const [workspace, tiling] of [...this._tilings]) {
            if (!open.has(workspace)) {
                this._stopTiling(tiling);
                this._tilings.delete(workspace);
            }
        }
        this._saveTiled();
    }

    /**
     * The app of the focused window, by its desktop file ID without the extension, as gestures name
     * the apps they work in: `org.mozilla.firefox`. A window without a desktop file goes by its
     * window class, and with no window focused, it's empty.
     *
     * @returns {string} the app
     */
    FocusedApp() {
        const window = global.display.focus_window;
        return window ? appIdOf(window)?.replace(/\.desktop$/, '') ?? '' : '';
    }

    /**
     * Opens an app, or brings its window to the front if it's open, as clicking it in the dash
     * does. The Shell lets nothing else bring a window to the front.
     *
     * @param {string} app - the app's desktop file ID without the extension
     * @returns {boolean} whether the app is installed
     */
    OpenApp(app) {
        const found = Shell.AppSystem.get_default().lookup_app(`${app}.desktop`);
        // Outside of an input event the Shell's current time is 0, which counts as older than
        // anything the user did, so a window that's open wouldn't be given focus.
        found?.activate_full(-1, global.display.get_current_time_roundtrip());
        return found !== null;
    }

    /**
     * Asks which of several similar gestures was meant, on a card at the pointer. Gestos asks here
     * on GNOME, as the Shell can show it without a window that takes focus. A new question
     * replaces one still open, which counts as cancelled.
     *
     * @param {Array} params - the gestures, each its name, description, and points as x and y in
     *   turn
     * @param {Gio.DBusMethodInvocation} invocation - answered with the chosen one's index, or -1
     */
    ChooseGestureAsync([gestures], invocation) {
        this._chooser?.close();
        const entries = gestures.map(([name, description, coordinates]) => ({
            name,
            description,
            points: Array.from({length: Math.floor(coordinates.length / 2)},
                (_, i) => ({x: coordinates[2 * i], y: coordinates[2 * i + 1]})),
        }));
        const chooser = new GestureChooser(entries, choice => {
            if (this._chooser === chooser)
                this._chooser = null;
            invocation.return_value(new GLib.Variant('(i)', [choice]));
        });
        this._chooser = chooser;
        chooser.open();
    }

    /**
     * Asks the focused window to close, the way its close button does.
     *
     * @returns {boolean} whether there was a window to close
     */
    CloseWindow() {
        const window = global.display.focus_window;
        if (!window?.can_close())
            return false;
        // Outside of an input event the Shell's current time is 0, which Mutter can't ping the
        // window with to find out whether its app is still responding.
        window.delete(global.display.get_current_time_roundtrip());
        return true;
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
        this._actions.tileRemembered();
        this._dbus = Gio.DBusExportedObject.wrapJSObject(INTERFACE_XML, this._actions);
        this._dbus.export(Gio.DBus.session, OBJECT_PATH);
        this._trail = new GestureTrail(this.getSettings());
    }

    // This runs when the extension is turned off, and also when the screen locks: other extensions
    // are turned off there, and the Shell turns this one off and on again after them. Windows are
    // only put back in the first case.
    disable() {
        this._trail.destroy();
        this._trail = null;
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
