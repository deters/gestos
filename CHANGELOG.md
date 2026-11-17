# Changelog

All notable changes to Gestos are written here, since it was written in Rust. Versions before
6.0.0 were released as MyGestures.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and Gestos follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- **One process fewer.** Gestos runs two processes in the background instead of three.

## [8.1.0] - 2026-11-16

### Added

- **Control windows with the middle button.** Drag to move a window and scroll to resize it, both in the same press (Control Windows With the Middle Button, in Settings; GNOME).
- **Commands in advanced actions.** An advanced action can run a command between its other steps (Advanced Action).
- **Advanced actions for one app.** Make an advanced action that works only in the app you choose (the app's page).
- **Commands that use what you copied.** A command can work on the text you copied or selected, and on the files you copied (Run Command in Shell, Advanced Action).
- **Turn off middle click paste.** Pasting with the middle button can be turned off, or on and off with a gesture (Allow Middle Click Paste, in Settings; Toggle Middle Click Paste).
- **A gesture for middle button window control.** It turns window control on or off (Toggle Middle Button Window Control).

### Changed

- **Tiling while you drag.** Only the windows in the way of the one you drag make room for it, as it moves.
- **Commands shown before importing.** Importing a configuration lists the commands its gestures would run (Import Configuration, in Settings).

## [8.0.1] - 2026-11-15

### Fixed

- **Gestures with held keys on X11.** Gestures drawn while holding Ctrl, Alt, Shift or Super work on X11 too.

## [8.0.0] - 2026-11-15

### Added

- **Categories.** Actions are grouped, such as Windows, Navigation and Sound and Media, each with the gestures you made for them.
- **A page for each app.** See an app's gestures and shortcuts, and make a gesture for any of those shortcuts.
- **See what a gesture did.** A small panel says what ran, why it couldn't, or that nothing matched (GNOME, KDE, X11).
- **The gesture path on KDE and X11.** See your gesture as you draw it there too.
- **Look up what you select.** Select text and draw: a link opens, a sum is worked out, anything else is searched on the web (Look Up Selection).
- **Gestures per app on Sway and Hyprland.** One shape can do something different in each app there too.

### Changed

- **Rough shapes are recognized.** An L with uneven sides or a rounded corner still counts, and a V is still told from a U.
- **An example in the editor.** It shows a mouse drawing a gesture, then shows yours as you draw it.
- **Record any shortcut.** Even ones your desktop keeps for itself, like Ctrl+Alt+Up, and modifier keys on their own.
- **Undo deleting a gesture.** Deleting no longer asks first, and can be undone.
- **Keyboard shortcuts.** Ctrl+W closes a window, Ctrl+Q quits, and Ctrl+? lists them all.

### Fixed

- **The right version in Extensions.** GNOME's Extensions app shows the version you have.
- **Gestures for one app with many apps installed.** They run without delay.

## [7.1.0] - 2026-11-10

### Changed

- **Snapped windows stay where you put them.** A window snapped to half the screen stays there, and the others are tiled beside it.

## [7.0.0] - 2026-11-10

### Added

- **Tiling on every screen.** Windows on all your monitors are tiled, while dialogs and videos stay on top.
- **Overview with both buttons.** Hold right and click left to see all your windows (Show Overview With Both Buttons, in Settings; GNOME, KDE).

### Changed

- **Windows keep their size.** A window gets back the size you gave it after tiling or maximizing, even after you reopen the app.

### Fixed

- **Tiling on Ubuntu.** It works there now.
- **Crash when closing some windows.** Closing them no longer crashes GNOME and logs you out.
- **Swapped keys.** Gestures and shortcuts work with keys swapped, such as Ctrl and Win.

## [6.9.0] - 2026-11-09

### Added

- **Snap to a side.** A gesture can put a window on half the screen (View Split on Left, View Split on Right).

### Fixed

- **Maximizing a tiled window.** It no longer lags behind the other windows.
- **Moving tiled windows.** A tiled window you move to a side, a corner or another screen stays there, and the others make room.
- **Windows on all workspaces.** A window shown on every workspace no longer covers the tiled ones.
- **The icon on Ubuntu.** Gestos shows its icon in Ubuntu's app list.

## [6.8.1] - 2026-11-09

### Fixed

- **Typed text keeps its punctuation.** Text a gesture types keeps its dots, slashes and other symbols on every system, Alpine included, so `www.google.com` is typed as written.

## [6.8.0] - 2026-11-09

### Added

- **Alpine for ARM.** A package for Alpine on ARM computers.

### Changed

- **Only actions that work here.** You're offered the actions of your own desktop, and a gesture made for another desktop says which one it needs.

### Fixed

- **Starting gestures without systemd.** When gestures can't start, Gestos shows the command that starts them.
- **Reports without a text editor.** When no app can open the report, Gestos shows where it is and can copy it.

## [6.7.0] - 2026-11-09

### Added

- **Gestures per app on KDE and X11.** One shape can do something different in each app there too.

### Fixed

- **Apps with the same name.** Apps that share a name are told apart, like Files (Nemo) and Files (Nautilus).
- **Newly installed apps.** They show up without reopening Gestos.

## [6.6.1] - 2026-11-09

### Fixed

- **Closing a tiled window.** It disappears at once, without waiting for the other windows to move.

## [6.6.0] - 2026-11-09

### Added

- **GNOME 51.** Gestures per app, the gesture path and tiling work on the new GNOME.
- **Why a gesture can't run.** A gesture that can't run says why, and what to do.

### Changed

- **Looks like GNOME.** Gestos matches GNOME's own apps, follows dark style, and has new About and Preferences windows.
- **Gestures in bug reports.** Reports include what Gestos did with your gestures.

## [6.5.0] - 2026-11-08

### Added

- **Open any app.** Every app has an action that opens it, or brings it to the front.
- **See your gesture as you draw it.** Its path follows your pointer and fades when you let go (Preferences).
- **Choose the button you draw with.** Right, middle, back or forward, and left-handed mice use the other side (Preferences).
- **Bug reports.** A button makes a report of your setup to attach to a bug report (Preferences).

### Changed

- **Fewer notifications.** Gestos only notifies you when something goes wrong.
- **Preferences in groups.** Settings are grouped by what they're for.
- **Labeled steps.** In the gesture editor, each step says what it is, and the drawing area keeps its size.

### Fixed

- **Several mice.** Every mouse draws gestures, not only the first one found.

## [6.4.1] - 2026-11-08

### Changed

- **Steps as rows.** Each step of a custom action is a row with an icon for its kind, and a button adds each kind.

### Fixed

- **The gesture editor keeps its width** when steps are added.

## [6.4.0] - 2026-11-08

### Added

- **Gestures for some apps.** The same shape can do something different in the apps you pick (GNOME).
- **Open an app first.** A custom action can open an app, or bring it to the front, and its next steps go to that app (Custom Action).
- **Named custom actions.** A new one is named after what you searched for, and you can rename it.

### Changed

- **New names.** Custom Keyboard Action is now Custom Action, and Run Command is now Run Command in Shell.
- **The action first.** The gesture editor shows what a gesture does first, then what triggers it.
- **No notification on start and stop.** Starting and stopping gestures notifies you only when it fails.

## [6.3.0] - 2026-11-07

### Added

- **Shortcuts, text and pauses, in any order.** One action can press shortcuts, type text, even several lines with accented letters, and wait in between (Custom Keyboard Action).
- **Tile automatically.** Every workspace is tiled, new ones too (Tile Automatically, in Preferences).
- **Out-of-date extension.** Gestos says when its GNOME extension is out of date, and how to update it.

### Changed

- **Tiling keeps windows at their own sizes.** Windows move just enough not to overlap, and only shrink when they must.
- **Tiling stays on.** It stays on until you turn it off, even after you log in again.
- **Pick the gesture you meant, at the pointer.** The card appears where you drew, and your window keeps its focus (GNOME).
- **Two actions in every search.** Run Command and Custom Keyboard Action show up whatever you search for.
- **Record any key.** The shortcut recorder takes Escape, Backspace, Tab and Enter too.

### Fixed

- **Gestos's own windows aren't tiled.**
- **Starting gestures from Settings.** Settings can start gestures when they weren't running.

## [6.2.2] - 2026-11-04

### Fixed

- **Overlapping windows of some apps.** They now get a tile like the others.
- **Smallest tile size.** Tiles stay big enough to use, so up to 12 windows fit, 4 across and 3 down.

## [6.2.1] - 2026-11-04

### Changed

- **Moving a tiled window puts all windows back** at their own sizes, as before 6.2.0.

### Fixed

- **Big windows tile without overlapping.** When they can't all shrink enough, they're laid out in a grid, and the biggest get the widest tiles.

## [6.2.0] - 2026-11-03

### Changed

- **Moving a tiled window keeps tiling.** The window you move takes a tile where you leave it, and the others fit around it.

## [6.1.3] - 2026-11-03

### Fixed

- **Crash with several monitors.** Closing a window on another monitor while tiling no longer ends your session.
- **Force quit hung apps.** Closing a window that doesn't respond offers to force quit it (Close Window).

## [6.1.2] - 2026-11-03

### Fixed

- **Close Window like its close button.** Windows close just as when you click their close button (Close Window; GNOME).
- **Shortcuts start sooner.** Gestures press their shortcuts with less delay (GNOME).
- **Pointer on a busy desktop.** It keeps moving smoothly when the computer is busy.

## [6.1.1] - 2026-11-02

### Fixed

- **Tiled windows don't overlap.** Even apps that won't shrink, or resize themselves, get a tile of their own.
- **Smallest tile size.** Tiles stay at least 100 pixels wide and tall.
- **Odd gaps.** A gap of an odd number of pixels is exact.
- **Windows moved to a tiled workspace.** They get a tile.

## [6.1.0] - 2026-11-02

### Added

- **Tile windows.** Fit a workspace's windows together without overlaps, and put them back with the same gesture (Toggle Tile Windows; GNOME).
- **The Gestos GNOME extension.** It comes with Gestos and brings tiling: log out and back in, then turn it on (Preferences).
- **Apps keep your window sizes.** They save the size you gave a window, not its tile.

### Changed

- **For packagers.** The GNOME extension installs to `share/gnome-shell/extensions/gestos@deters.github.io`, needs `glib-compile-schemas` to build, and `-Dgnome_extension=false` leaves it out.

### Fixed

- **Gestos's own icon.** Gestos shows its icon in GNOME's app list and dock.

## [6.0.0] - 2026-11-01

The first release named Gestos, formerly MyGestures. Your gestures and settings carry over.

### Added

- **Modifier keys.** A gesture can require Ctrl, Alt, Shift or Super to be held.
- **Standard shortcuts.** Copy, Zoom In and other common shortcuts are actions of their own.
- **Make gestures from search.** Search for what a gesture should do, and make it from there.
- **Similar gestures.** When a drawing matches several gestures, you pick the one you meant.
- **Hide which gesture ran.** A setting hides that notification (Reduce Notifications).
- **Any keyboard layout.** Shortcuts press the right keys on every layout.
- **Locked screen.** Gestures do nothing while the screen is locked.
- **A new look.** A new icon, a smaller gesture editor, and Start and Stop in the header bar.

### Changed

- **Upgrading from source.** Run `sudo meson install -C build` or `sudo make install` again, or gestures won't start. Packages take care of this.
- **Without systemd.** Gestos needs elogind. On Alpine, start it with `rc-service gestos-input start`, and at every boot with `rc-update add gestos-input`. The Alpine package is experimental.
- **No gestures to start with.** You make the ones you want.
- **One action per gesture.** An older configuration with several uses the first.
- **Gestures start with Gestos.** Opening it starts them, unless you stopped them yourself.
- **GNOME actions without a shortcut** run too, and sooner. If one sometimes does nothing, set `GESTOS_SHORTCUT_DELAY_MS` to `300`.
- **New name.** Custom Keyboard Shortcut replaces Keypress Shortcut.

### Removed

- **Running as root.** Gestos runs as your own user, so `sudo gestosd` no longer works.
- **The `input` group.** You can leave it if you only joined it for Gestos.

### Security

- **No access to your typing.** What runs your gestures can't read what you type ([README](https://github.com/deters/gestos#security)).
- **Configurations others can change.** They're refused, and the error says how to fix it.

## [4.2.2] - 2026-10-31

### Added

- **Reorder your gestures.** Drag them into the order you want.
- **Search for an action.** The gesture editor finds actions as you type.
- **KDE's shortcuts.** A gesture can run any of KDE's global shortcuts.
- **Import and export.** Save your gestures to a file, load them back, or start over with the defaults (Settings).
- **Use it from the keyboard.** Every part of the window can be reached without the mouse.

### Changed

- **What each gesture does.** The main window shows it, with shortcuts as keycaps.

### Fixed

- **Keys, clicks and saving.** Keys and clicks are passed on every time, and gestures are saved as you leave them.

## [4.2.1] - 2026-06-11

### Fixed

- **Fedora packages** build again.

### Security

- **Rights given up stay given up.** MyGestures fully drops the input group's rights it starts with.

## [4.2.0] - 2026-06-11

### Changed

- **Tidier code.** No change you'd notice.

## [4.1.11] - 2026-06-11

Released again to rebuild the packages.

## [4.1.10] - 2026-06-11

### Added

- **Dark style.** MyGestures follows your desktop's light or dark style.
- **Rename gestures.**
- **See which way a gesture goes.** Its drawing ends in an arrowhead.

### Changed

- **Starts without systemd.** MyGestures starts through D-Bus, with systemd or without it.
- **Looks like GNOME.** Windows, lists and dialogs follow GNOME's layout and colors, and shortcuts show as keycaps.
- **Delete a gesture from its editor.**

### Removed

- **Mouse Click** is no longer offered as an action.

## [4.1.9] - 2026-06-10

### Added

- **Errors in the window.** When gestures can't start, MyGestures says why, in text you can select and copy.

### Fixed

- **The uinput module loads at boot**, so gestures can press keys right after starting the computer.

## [4.1.8] - 2026-06-10

### Fixed

- **Debian packages** are attached to the release.

## [4.1.7] - 2026-06-10

### Fixed

- **Debian packages** build again.

## [4.1.6] - 2026-06-10

### Fixed

- **Packages.** The Debian package creates the input group, and the Alpine package builds again.

## [4.1.5] - 2026-06-10

Released again to rebuild the packages.

## [4.1.4] - 2026-06-10

Released again to rebuild the packages.

## [4.1.3] - 2026-06-10

### Fixed

- **Alpine packages** build again.

## [4.1.2] - 2026-06-10

### Fixed

- **Packages** build again on Debian, Fedora and Alpine.

## [4.1.1] - 2026-06-10

### Fixed

- **Packages** build again on Debian and Alpine.

## [4.1.0] - 2026-06-10

### Added

- **A window to set up your gestures.** Draw a gesture, pick what it does, and record the keys it presses.
- **GNOME's actions.** Any shortcut in GNOME's settings can be a gesture's action.
- **XFCE.** Actions for the XFCE desktop.
- **Similar gestures.** A gesture too much like another one is pointed out as you draw it.
- **Packages** for Debian and Ubuntu, Fedora and Alpine.

### Changed

- **Written in Rust**, and built with Meson.
- **Gestures of any size.** A gesture is recognized however big or small, or wide or tall, you draw it.

[Unreleased]: https://github.com/deters/gestos/compare/v8.1.0...HEAD
[8.1.0]: https://github.com/deters/gestos/compare/v8.0.1...v8.1.0
[8.0.1]: https://github.com/deters/gestos/compare/v8.0.0...v8.0.1
[8.0.0]: https://github.com/deters/gestos/compare/v7.1.0...v8.0.0
[7.1.0]: https://github.com/deters/gestos/compare/v7.0.0...v7.1.0
[7.0.0]: https://github.com/deters/gestos/compare/v6.9.0...v7.0.0
[6.9.0]: https://github.com/deters/gestos/compare/v6.8.1...v6.9.0
[6.8.1]: https://github.com/deters/gestos/compare/v6.8.0...v6.8.1
[6.8.0]: https://github.com/deters/gestos/compare/v6.7.0...v6.8.0
[6.7.0]: https://github.com/deters/gestos/compare/v6.6.1...v6.7.0
[6.6.1]: https://github.com/deters/gestos/compare/v6.6.0...v6.6.1
[6.6.0]: https://github.com/deters/gestos/compare/v6.5.0...v6.6.0
[6.5.0]: https://github.com/deters/gestos/compare/v6.4.1...v6.5.0
[6.4.1]: https://github.com/deters/gestos/compare/v6.4.0...v6.4.1
[6.4.0]: https://github.com/deters/gestos/compare/v6.3.0...v6.4.0
[6.3.0]: https://github.com/deters/gestos/compare/v6.2.2...v6.3.0
[6.2.2]: https://github.com/deters/gestos/compare/v6.2.1...v6.2.2
[6.2.1]: https://github.com/deters/gestos/compare/v6.2.0...v6.2.1
[6.2.0]: https://github.com/deters/gestos/compare/v6.1.3...v6.2.0
[6.1.3]: https://github.com/deters/gestos/compare/v6.1.2...v6.1.3
[6.1.2]: https://github.com/deters/gestos/compare/v6.1.1...v6.1.2
[6.1.1]: https://github.com/deters/gestos/compare/v6.1.0...v6.1.1
[6.1.0]: https://github.com/deters/gestos/compare/v6.0.0...v6.1.0
[6.0.0]: https://github.com/deters/gestos/compare/v4.2.2...v6.0.0
[4.2.2]: https://github.com/deters/gestos/compare/v4.2.1...v4.2.2
[4.2.1]: https://github.com/deters/gestos/compare/v4.2.0...v4.2.1
[4.2.0]: https://github.com/deters/gestos/compare/v4.1.11...v4.2.0
[4.1.11]: https://github.com/deters/gestos/compare/v4.1.10...v4.1.11
[4.1.10]: https://github.com/deters/gestos/compare/v4.1.9...v4.1.10
[4.1.9]: https://github.com/deters/gestos/compare/v4.1.8...v4.1.9
[4.1.8]: https://github.com/deters/gestos/compare/v4.1.7...v4.1.8
[4.1.7]: https://github.com/deters/gestos/compare/v4.1.6...v4.1.7
[4.1.6]: https://github.com/deters/gestos/compare/v4.1.5...v4.1.6
[4.1.5]: https://github.com/deters/gestos/compare/v4.1.4...v4.1.5
[4.1.4]: https://github.com/deters/gestos/compare/v4.1.3...v4.1.4
[4.1.3]: https://github.com/deters/gestos/compare/v4.1.2...v4.1.3
[4.1.2]: https://github.com/deters/gestos/compare/v4.1.1...v4.1.2
[4.1.1]: https://github.com/deters/gestos/compare/v4.1.0...v4.1.1
[4.1.0]: https://github.com/deters/gestos/releases/tag/v4.1.0
