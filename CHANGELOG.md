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

- **Control windows with the middle button.** Hold it and drag to move the window under the pointer, or scroll to make it bigger or smaller, both in the same press. On GNOME, when you draw gestures with the right button.
- **Commands in advanced actions.** Add a step that runs a command, between opening apps, pressing shortcuts and typing text.
- **Advanced actions inside apps.** Each app's page offers one, made for that app alone.
- **Commands that know what you copied.** Check Use Variables, and a command gets the copied text, the selected text, the files copied in your file manager, the app in use and the gesture's name. Copy files in Files, draw, and your script gets them, just as from the file manager's own Scripts menu.
- **Turn middle click paste on or off.** Allow Middle Click Paste in Settings, or flip it with a gesture.
- **Turn window control on or off with a gesture.** Toggle Middle Button Window Control, under Windows.

### Changed

- **Smoother tiling.** As you drag a window into place, the others make room in step with it, and only the ones in its way move.
- **Safer imports.** Importing a configuration shows the commands its gestures would run before you agree.

## [8.0.1] - 2026-11-15

### Fixed

- **Gestures with held keys on X11.** A gesture drawn while holding Ctrl, Alt, Shift or Super is saved with those keys, so Super+draw and Alt+draw gestures work on X11 too.

## [8.0.0] - 2026-11-15

### Added

- **Categories.** Every action has its place, in Windows, Navigation, Screenshots, Sound and Media and more, each with the gestures you already made for it.
- **A page for each app.** Open an app to see its gestures, the shortcuts it offers, and add one for any of them in a click.
- **See what a gesture did.** A small panel says what ran, why it couldn't, or that the gesture wasn't recognized, on GNOME, KDE and other X11 desktops.
- **The path on KDE and X11.** Watch your gesture as you draw it, on KDE and on every X11 desktop, not only GNOME.
- **Look Up Selection.** Select text anywhere and draw: a link opens, a sum is worked out, and anything else is searched on the web.
- **Gestures per app on Sway and Hyprland.** One shape can do something different in each app there too.

### Changed

- **Gestures that read your intent.** Gestos follows the directions you draw, so a quick L with uneven sides or a rounded corner still counts, and a V is still told from a U.
- **Learn by watching.** The editor shows a mouse drawing a gesture with the right button, then shows yours as you draw it.
- **Record any shortcut.** Even ones your desktop keeps for itself, like Ctrl+Alt+Up, and modifier keys on their own.
- **Undo instead of asking.** Deleting a gesture takes a click, and undoing it another.
- **Keyboard shortcuts.** Ctrl+W closes a window, Ctrl+Q quits, and Ctrl+? lists them all.

### Fixed

- **The right version in Extensions.** GNOME's Extensions app shows the version you have.
- **Quicker with many apps installed.** Gestures for one app run without delay.

## [7.1.0] - 2026-11-10

### Changed

- **Snapped windows stay put.** A window you snap to half the screen stays there, and tiling fits the other windows beside it.

## [7.0.0] - 2026-11-10

### Added

- **Tiling on every screen.** Windows on your other monitors are tiled too, while dialogs and videos stay on top instead of taking a tile.
- **Show Overview With Both Buttons.** On GNOME and KDE, hold right and click left to see all your windows. Turn it on in Gestos's settings.

### Changed

- **Windows keep their size.** A window goes back to the size you gave it when you stop tiling or unmaximize it, even after you close and reopen the app.

### Fixed

- **Tiling works on Ubuntu.** Before, it didn't start at all there.
- **No more logouts.** Closing some windows could crash GNOME and log you out.
- **Swapped keys.** Gestures and shortcuts work if you've swapped keys such as Ctrl and Win.

## [6.9.0] - 2026-11-09

### Added

- **Snap to a side.** GNOME's View Split on Left and Right are now actions, so a gesture can put a window on half the screen.

### Fixed

- **Smoother maximizing.** A tiled window you maximize no longer lags behind the others moving into its place.
- **Moving tiled windows.** A tiled window moved to a side, a corner or another screen stays where you put it, and the others make room.
- **Windows on all workspaces.** A window shown on every workspace no longer covers the tiled windows, and rejoins tiling when it's back on one.
- **Gestos's icon on Ubuntu.** Gestos shows its icon in Ubuntu's app list.

## [6.8.1] - 2026-11-09

### Fixed

- **Typed text keeps its punctuation.** On systems where Gestos can't tell the keyboard layout, such as Alpine, typed text no longer loses its dots, slashes and other symbols, so `www.google.com` comes out whole.

## [6.8.0] - 2026-11-09

### Added

- **Alpine for ARM.** An Alpine package for ARM computers, such as a virtual machine on an Apple silicon Mac.

### Changed

- **Only actions that work here.** GNOME's actions are offered on GNOME only, as KDE's are on KDE, and a gesture that uses another desktop's action says which desktop it works on.

### Fixed

- **Starting the input service on Alpine.** When it isn't running, Gestos now shows the command that starts it on systems without systemd.
- **Reports on systems without a text editor.** When no app can open the report, Gestos shows where it was saved and can copy it.

## [6.7.0] - 2026-11-09

### Added

- **Gestures per app on KDE and X11.** Gestures limited to some apps now work on KDE and on X11 desktops such as Cinnamon, not only on GNOME.

### Fixed

- **Apps with the same name are told apart.** Nemo and Nautilus, which both call themselves Files, show as Files (Nemo) and Files (Nautilus), so searching for either finds it.
- **New apps show up right away**, without reopening Gestos.

## [6.6.1] - 2026-11-09

### Fixed

- **Closing a tiled window is instant.** It disappears right away, instead of lingering while the other windows take its place.

## [6.6.0] - 2026-11-09

### Added

- **GNOME 51.** The Gestos extension works on the new GNOME, so gestures per app, the gesture path and window tiling are back after upgrading.
- **Know when the extension is missing.** A gesture that needs the GNOME extension tells you when it can't run, and whether to turn the extension on or update Gestos.

### Changed

- **Looks like GNOME.** Gestos now matches GNOME's own apps, follows dark style on every desktop, and has new About and Preferences windows.
- **Better bug reports.** The debugging report now always includes what Gestos did with your gestures.

## [6.5.0] - 2026-11-08

### Added

- **Open any app.** Every installed app has its own Open action, so a gesture can open your browser or bring it to the front without setting up a custom action.
- **See your gesture as you draw it.** Its path follows your pointer on screen and fades when you let go. You can turn it off in Preferences.
- **Choose the button you draw with.** Right, Middle, Back or Forward, in Preferences. Left-handed mice draw with the button on the other side.
- **Debugging information.** One button in Preferences creates a report of your setup and what Gestos logged, ready to attach to a bug report.

### Changed

- **Fewer notifications.** Gestos only notifies you when something goes wrong, such as a gesture that can't run in the app you're in.
- **A clearer Preferences page**, grouped into gestures, window tiling, the GNOME extension, your gestures and troubleshooting.
- **A tidier gesture editor.** Each step of a custom action says what it is, the drawing area keeps its size, and Valid Apps says when a gesture only works in some apps.

### Fixed

- **Every mouse draws gestures**, not just the first one Gestos found.

## [6.4.1] - 2026-11-08

### Changed

- **A custom action's steps are listed like other lists in GNOME.** A Steps row has a small button to add each kind of step, and each step is a row of its own, with an icon for its kind.

### Fixed

- **Action Details is as wide as the rest of the gesture editor.** Its buttons to add steps made it wider.

## [6.4.0] - 2026-11-08

### Added

- **Gestures for some apps.** A gesture can be limited to the apps you pick, in its trigger. It then only works in them, and when a drawing matches both, the app's own gesture wins, so the same shape can do something else in some apps. This needs the GNOME extension.
- **Custom Action can open an app** before its other steps, or bring its window to the front if it's open, and the steps after it go to that app.
- **Custom actions have names.** A new one is named after what you searched for, offered as Create New Action in the results, and you can change it.

### Changed

- **Custom Keyboard Action is now Custom Action**, and **Run Command is now Run Command in Shell**.
- **The gesture editor** shows what the action does first, then what triggers it: the modifier keys and the drawing together, and the apps it works in. Its parts are as wide as the main window's lists, it opens as tall as what's in it, and Delete Gesture… comes last.
- **Starting and stopping gestures** from the app icon's menu no longer shows a notification. Only a failure to start does.

## [6.3.0] - 2026-11-07

### Added

- **Custom Keyboard Action** replaces Custom Keyboard Shortcut. It presses shortcuts, types text and pauses, in any order you set. Text can have several lines, and accented letters are typed with your keyboard layout's dead keys. A single shortcut works as before.
- **Tile Automatically**, in Preferences, tiles every workspace, also new ones.
- **Gestos tells you when its GNOME extension is out of date**, and how to update it: by logging out and back in, or by removing an old copy in your home folder.

### Changed

- **Tiling keeps windows at their own sizes.** Windows move just enough not to overlap, and only shrink when moving isn't enough. Moving or resizing a window makes the others make room for it, and windows minimized for lack of room come back once there's room again.
- **Tiling stays on** until you turn it off, also after logging out and back in. Maximizing a window, making it fullscreen, or snapping it sets it aside until it's back to normal.
- **On GNOME, the card that asks which gesture you meant** appears at the pointer, styled like GNOME's menus, and your window keeps its focus.
- **Run Command and Custom Keyboard Action are always offered** when you search for an action, as they can do what the others can't.
- **The shortcut recorder records any key**, Escape, Backspace, Tab and Enter included.

### Fixed

- **Gestos's own windows aren't tiled.**
- **The daemon can be started through D-Bus.**

## [6.2.2] - 2026-11-04

### Fixed

- **Windows that misreport their smallest size tile like the others.** Some apps, like ones built on WebKit, say the size they open at is the smallest they can get, which left them overlapping their neighbors. A window that says it can't get smaller than half the screen now gets a tile like the others.
- **Tiles stay usable.** No tile gets smaller than a quarter of the screen's width or a third of its height, so up to 12 windows fit, 4 across and 3 down.

## [6.2.1] - 2026-11-04

### Changed

- **Moving a tiled window puts all the windows back** at their original sizes again, as before 6.2.0.

### Fixed

- **Big windows tile without overlapping where they fit.** When the windows can't all get as small as their tiles, they're laid out as a grid, as square as fits, and the windows that need the most room get the widest tiles. Windows that can't fit at all stay on the screen.

## [6.2.0] - 2026-11-03

### Changed

- **Moving a tiled window keeps the workspace tiled.** The window you move gets its original size back while it moves, and the other tiles stay where they are. Where you leave it, it takes a tile, on that monitor, and the tiles are placed again around it. Maximizing or snapping it while it moves only takes that window out of tiling.

## [6.1.3] - 2026-11-03

### Fixed

- **GNOME Shell no longer crashes** when a window on a monitor other than the primary one closes while the primary display's windows are tiled. The crash ended the session.
- Close Window lets GNOME check that the app is still responding as it closes, so an app that hangs can be offered to force quit.

## [6.1.2] - 2026-11-03

### Fixed

- **Close Window, on GNOME:** closes the focused window the way its close button does, through the “Gestos” GNOME extension. Without the extension it still presses GNOME's close shortcut.
- Shortcuts that gestures press on GNOME start sooner, as the keyboard layout is no longer looked up with `gsettings` each time.
- The cursor stays smooth on a busy desktop, as the input service now runs at a slightly higher priority.

## [6.1.1] - 2026-11-02

### Fixed

Toggle Tile Windows, in the “Gestos” GNOME extension:

- Windows no longer cover each other when an app can't get as small as its tile, sets its smallest size late, or resizes its window itself. Apps that keep resizing their window keep that size, still tiled, and the tiles beside them take the rest.
- Tiles stay at least 100 pixels wide and tall while the screen has room for them.
- The gap between tiles is exact when it's an odd number of pixels.
- Windows moved to a tiled workspace join once they're on screen, so GNOME's placement doesn't undo their tile.

## [6.1.0] - 2026-11-02

### Added

- **Toggle Tile Windows, on GNOME:** fits the windows on a workspace together without overlaps, each near where it was and sized in proportion to the others. Resizing a tile keeps the edge where you leave it. Using it again, or moving, maximizing, or snapping a window, puts them all back.
- **The “Gestos” GNOME extension:** tiling runs in it, and it comes with Gestos. After installing, log out and back in, then turn it on in Gestos's Preferences, which also shows whether it's running and has a Window Gap setting. Gestos works without it, and other desktops don't use it.
- **Original sizes kept:** tiled windows get their original size back before they're closed from GNOME, maximized, or left at logout, so apps don't save the tiled size. If an app closes one itself, the next window it opens at that size gets the original size instead.

### Fixed

- Gestos shows with its own icon in GNOME's app list and dock.

### Changed

- **Packaging.** The extension installs to `share/gnome-shell/extensions/gestos@deters.github.io`. Build with `-Dgnome_extension=false` to leave it out.
- **Packaging.** Building it needs `glib-compile-schemas`, and its tests run with `gjs` when it's installed.

## [6.0.0] - 2026-11-01

The first release under the name Gestos, formerly MyGestures. Your MyGestures config and autostart entry are carried over the first time it runs.

### Added

- **Modifier keys:** a gesture can require Ctrl, Alt, Shift or Super to be held.
- **Standard shortcuts:** the GNOME shortcuts, such as Copy (Ctrl+C) and Zoom In (Ctrl++), are now actions of their own.
- **Search first:** gestures are created from the search box, which suggests every action no gesture uses yet.
- **Similar gestures:** if a drawing matches several gestures, a chooser appears at the pointer.
- **Reduce Notifications:** a setting for hiding the notification that says which gesture ran.
- **Keyboard layouts:** shortcuts press the right keys on any layout.
- **Locked screen:** gestures are ignored while the screen is locked, on desktops that report it to logind.
- **A new look:** Start and Stop in the header bar, a smaller gesture editor following the GNOME guidelines, a new icon, and icons of their own for GNOME actions.

### Changed

- **A system service reaches your input devices.** `gestosd` is no longer installed as Set-Group-ID. Packages take care of the upgrade. From source, run `sudo meson install -C build` or `sudo make install` again, or gestures can't start.
- **Without systemd,** the service needs elogind. On Alpine, start it with `rc-service gestos-input start`, and add it with `rc-update add gestos-input` to start it at boot. The Alpine package is experimental: its service setup is new, runs without the systemd sandbox, and has not been tested.
- Gestos ships without any gestures, so you create the ones you want.
- Each gesture has exactly one action. An older config listing several still loads, using the first.
- Opening Gestos starts gestures, unless you stopped them yourself.
- GNOME actions with no shortcut assigned now run, and start sooner. If one sometimes does nothing, try setting `GESTOS_SHORTCUT_DELAY_MS` to `300`.
- Custom Keyboard Shortcut replaces Keypress Shortcut.

### Removed

- **Running as root.** `gestosd` must run as your own user, so `sudo gestosd` no longer works.
- **The `input` group.** Being in it no longer has any effect on Gestos, so you can leave it if you only joined for this.

### Security

- **What each part can access.** The [README](https://github.com/deters/gestos#security) says. In short, `gestosd` has no special rights and can't read what you type.
- **Config permissions.** A config that other users can change is refused, as is one in a folder they can change or that belongs to them. The error says what to fix.

## [4.2.2] - 2026-10-31

### Added

- **Reorder your gestures.** Drag them in the list into the order you want.
- **Search for an action.** The gesture editor finds actions as you type, with an icon for each.
- **KDE's shortcuts.** A gesture can run any of KDE's global shortcuts.
- **Import and export.** Settings can import or export your gestures, or load the default ones.
- **Use it from the keyboard.** Every part of the window can be reached without the mouse.

### Changed

- **A clearer main window.** Each gesture shows what it does, with shortcuts as keycaps, and a New Gesture button, a search bar and a menu laid out as in GNOME's own apps.

### Fixed

- **A more reliable daemon.** Keys and clicks are passed on dependably, and your gestures are saved as you leave them.

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
