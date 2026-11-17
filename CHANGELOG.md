# Changelog

All notable changes to Gestos are written here, since it was written in Rust. Versions before
6.0.0 were released as MyGestures.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and Gestos follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- Gestos now runs two background processes instead of three.

## [8.1.0] - 2026-11-16

### Added

- New option "Control Windows With the Middle Button" in Settings: dragging with the middle button moves the window under the pointer, and scrolling resizes it (GNOME).
- The action "Advanced Action" can now run a command as one of its steps.
- Each app's page can now create an "Advanced Action" that works only in that app.
- The actions "Run Command in Shell" and "Advanced Action" can now give their commands the copied text, the selected text and the copied files, with the new option "Use Variables".
- New option "Allow Middle Click Paste" in Settings, to turn off pasting with the middle button.
- New action "Toggle Middle Click Paste": turns middle click paste on or off.
- New action "Toggle Middle Button Window Control": turns "Control Windows With the Middle Button" on or off.

### Changed

- While a window is dragged, only the tiled windows in its way move now, instead of all of them.
- "Import Configuration" in Settings now lists the commands the imported gestures run, before you confirm.

## [8.0.1] - 2026-11-15

### Fixed

- Gestures drawn while holding Ctrl, Alt, Shift or Super were saved without those keys on X11.

## [8.0.0] - 2026-11-15

### Added

- Actions are now grouped in categories, such as Windows, Navigation and Sound and Media. You can still see all your gestures together by turning on "Show Gestures in the Main Page" in Settings.
- A page for each app, with its gestures and the shortcuts it offers.
- A panel that shows what a gesture ran, or why nothing ran (GNOME, KDE, X11).
- The gesture path is now also shown on KDE and X11.
- New action "Look Up Selection": opens the selected text as a link, works out a sum, or searches the web for it.
- Gestures per app now also work on Sway and Hyprland.

### Changed

- Gestures are now matched by the directions drawn, instead of the exact shape.
- The gesture editor now shows an example of a gesture being drawn.
- The shortcut recorder now also records shortcuts the desktop uses itself, like Ctrl+Alt+Up, and modifier keys alone.
- Deleting a gesture no longer asks for confirmation, and can be undone instead.
- Added keyboard shortcuts: Ctrl+W closes a window, Ctrl+Q quits, and Ctrl+? lists them all.

### Fixed

- GNOME's Extensions app showed the wrong version of the extension.
- Gestures for one app were slow with many apps installed.

## [7.1.0] - 2026-11-10

### Changed

- Windows snapped to half the screen now stay there, and tiling fits the other windows beside them.

## [7.0.0] - 2026-11-10

### Added

- Tiling now also works on other monitors. Dialogs and videos stay on top instead of taking a tile.
- New option "Show Overview With Both Buttons" in Settings: holding right and clicking left shows all windows (GNOME, KDE).

### Changed

- Windows now get back the size you gave them after tiling or maximizing, even after the app is reopened.

### Fixed

- Tiling didn't start on Ubuntu.
- Closing some windows could crash GNOME and log you out.
- Gestures and shortcuts didn't work with swapped keys, such as Ctrl and Win.

## [6.9.0] - 2026-11-09

### Added

- New actions "View Split on Left" and "View Split on Right": put a window on half the screen.

### Fixed

- A tiled window that was maximized lagged behind the other windows.
- A tiled window moved to a side, a corner or another screen didn't stay there.
- A window shown on every workspace covered the tiled windows.
- Gestos had no icon in Ubuntu's app list.

## [6.8.1] - 2026-11-09

### Fixed

- Text typed by a gesture lost its dots, slashes and other symbols on systems where the keyboard layout isn't known, such as Alpine.

## [6.8.0] - 2026-11-09

### Added

- An Alpine package for ARM.

### Changed

- Only the actions of the desktop in use are offered now, and a gesture made for another desktop says which one it needs.

### Fixed

- On systems without systemd, Gestos now shows the command that starts the input service when it isn't running.
- When no app can open the bug report, Gestos now shows where it was saved and can copy it.

## [6.7.0] - 2026-11-09

### Added

- Gestures per app now also work on KDE and X11.

### Fixed

- Apps with the same name, like Nemo and Nautilus, which are both called Files, couldn't be told apart.
- Newly installed apps only showed up after reopening Gestos.

## [6.6.1] - 2026-11-09

### Fixed

- A closed tiled window stayed on screen while the other windows moved into its place.

## [6.6.0] - 2026-11-09

### Added

- Support for GNOME 51.
- A gesture that can't run because the GNOME extension is off or outdated now says so.

### Changed

- Gestos now looks like GNOME's own apps, follows dark style, and has new About and Preferences windows.
- Bug reports now always include what Gestos did with the gestures.

## [6.5.0] - 2026-11-08

### Added

- New "Open" action for every installed app, which also brings the app to the front if it's open.
- New option "Show Gesture Path" in Preferences: draws each gesture on screen as you draw it.
- New option "Draw With" in Preferences: draws gestures with the right, middle, back or forward button. Left-handed mice use the other side.
- New option "Debugging Information" in Preferences: creates a report to attach to a bug report.

### Changed

- Gestos now only notifies when something goes wrong.
- Preferences is now grouped into sections.
- Each step of a custom action now says what it is, and the drawing area keeps its size.

### Fixed

- Only the first mouse found could draw gestures.

## [6.4.1] - 2026-11-08

### Changed

- The steps of a custom action are now a list, with a button to add each kind of step.

### Fixed

- The buttons to add steps made the gesture editor wider.

## [6.4.0] - 2026-11-08

### Added

- Gestures limited to some apps (GNOME).
- The action "Custom Action" can now open an app, or bring it to the front, before its other steps.
- Each "Custom Action" now has a name, and a new one is named after what was searched for.

### Changed

- Renamed the action "Custom Keyboard Action" to "Custom Action", and "Run Command" to "Run Command in Shell".
- The gesture editor now shows the action first, then what triggers it.
- Starting and stopping gestures no longer shows a notification, unless it fails.

## [6.3.0] - 2026-11-07

### Added

- New action "Custom Keyboard Action", which replaces "Custom Keyboard Shortcut": presses shortcuts, types text and waits, in the order you set. The text can have several lines and accented letters.
- New option "Tile Automatically" in Preferences: tiles every workspace, new ones too.
- Gestos now says when its GNOME extension is out of date, and how to update it.

### Changed

- Tiling now keeps windows at their own sizes, moving them just enough not to overlap.
- Tiling now stays on until turned off, also after logging out.
- The card that asks which gesture was meant now appears at the pointer, and the window keeps its focus (GNOME).
- The actions "Run Command" and "Custom Keyboard Action" are now offered in every search.
- The shortcut recorder now also records Escape, Backspace, Tab and Enter.

### Fixed

- Gestos's own windows were tiled.
- Settings couldn't start gestures when they weren't running.

## [6.2.2] - 2026-11-04

### Fixed

- Windows of some apps overlapped their neighbors when tiled.
- Tiles could get too small to use. Up to 12 windows now fit, 4 across and 3 down.

## [6.2.1] - 2026-11-04

### Changed

- Moving a tiled window puts all windows back at their own sizes again, as before 6.2.0.

### Fixed

- Big windows overlapped when tiled. They're now laid out in a grid when they can't all shrink enough.

## [6.2.0] - 2026-11-03

### Changed

- Moving a tiled window now keeps the workspace tiled: the window takes a tile where it's left.

## [6.1.3] - 2026-11-03

### Fixed

- GNOME crashed when a window on a second monitor closed while tiling.
- The action "Close Window" now offers to force quit a window that doesn't respond.

## [6.1.2] - 2026-11-03

### Fixed

- The action "Close Window" now closes windows like their close button does (GNOME).
- Shortcuts pressed by gestures now start sooner (GNOME).
- The pointer could stutter on a busy desktop.

## [6.1.1] - 2026-11-02

### Fixed

- Tiled windows covered each other when an app couldn't shrink or resized itself.
- Tiles could get smaller than 100 pixels.
- Gaps of an odd number of pixels were off by one.
- Windows moved to a tiled workspace didn't get a tile.

## [6.1.0] - 2026-11-02

### Added

- New action "Toggle Tile Windows": fits a workspace's windows together without overlaps, and puts them back when used again (GNOME).
- The Gestos GNOME extension, which tiling needs. It comes with Gestos: log out and back in, then turn it on (Preferences).
- Tiled windows get their own size back before they're closed, so apps don't remember the tile's size.

### Changed

- For packagers: the GNOME extension installs to `share/gnome-shell/extensions/gestos@deters.github.io` and needs `glib-compile-schemas` to build. `-Dgnome_extension=false` leaves it out.

### Fixed

- Gestos had no icon in GNOME's app list and dock.

## [6.0.0] - 2026-11-01

Renamed from MyGestures to Gestos. Gestures and settings carry over.

### Added

- Gestures that require Ctrl, Alt, Shift or Super to be held.
- New actions for standard shortcuts, such as "Copy" and "Zoom In".
- Creating gestures from the search box.
- A chooser for when a drawing matches several gestures.
- New option "Reduce Notifications": hides the notification of which gesture ran.
- Support for any keyboard layout.
- Gestures are now ignored while the screen is locked.
- A new icon and a smaller gesture editor, with Start and Stop in the header bar.

### Changed

- Gestos now reaches input devices through a system service, instead of being installed as Set-Group-ID. From source, run `sudo meson install -C build` or `sudo make install` again, or gestures won't start.
- Without systemd, the service needs elogind. On Alpine, start it with `rc-service gestos-input start`, and at every boot with `rc-update add gestos-input`. The Alpine package is experimental.
- Gestos no longer comes with gestures.
- Each gesture now has one action. Older configurations with several use the first.
- Opening Gestos now starts gestures, unless they were stopped.
- GNOME actions with no shortcut assigned now run, and sooner. If one sometimes does nothing, set `GESTOS_SHORTCUT_DELAY_MS` to `300`.
- Renamed the action "Keypress Shortcut" to "Custom Keyboard Shortcut".

### Removed

- Running as root: `sudo gestosd` no longer works.
- The need to be in the `input` group.

### Security

- What runs your gestures can no longer read what you type ([README](https://github.com/deters/gestos#security)).
- A configuration that other users can change is now refused.

## [4.2.2] - 2026-10-31

### Added

- Reordering gestures by dragging them.
- Searching for an action in the gesture editor.
- KDE's global shortcuts are now actions.
- New options "Import Configuration" and "Export Configuration" in Settings, and a way to load the default gestures.
- Keyboard navigation in every part of the window.

### Changed

- The main window now shows what each gesture does, with shortcuts as keycaps.

### Fixed

- Keys and clicks were sometimes not passed on, and gestures were sometimes not saved.

## [4.2.1] - 2026-06-11

### Fixed

- Fedora packages didn't build.

### Security

- MyGestures now fully drops the input group's rights it starts with.

## [4.2.0] - 2026-06-11

### Changed

- Code cleanup, with no visible changes.

## [4.1.11] - 2026-06-11

Rebuilt the packages, with no changes.

## [4.1.10] - 2026-06-11

### Added

- Dark style, following the desktop.
- Renaming gestures.
- An arrowhead at the end of a gesture's drawing, showing its direction.

### Changed

- MyGestures now starts through D-Bus, with or without systemd.
- The windows, lists and dialogs now follow GNOME's layout and colors, and shortcuts show as keycaps.
- Deleting a gesture moved to its editor.

### Removed

- The action "Mouse Click".

## [4.1.9] - 2026-06-10

### Added

- Errors that stop gestures from starting are now shown in the window, as text that can be copied.

### Fixed

- The uinput module didn't load at boot.

## [4.1.8] - 2026-06-10

### Fixed

- Debian packages weren't attached to the release.

## [4.1.7] - 2026-06-10

### Fixed

- Debian packages didn't build.

## [4.1.6] - 2026-06-10

### Fixed

- The Debian package didn't create the input group, and the Alpine package didn't build.

## [4.1.5] - 2026-06-10

Rebuilt the packages, with no changes.

## [4.1.4] - 2026-06-10

Rebuilt the packages, with no changes.

## [4.1.3] - 2026-06-10

### Fixed

- Alpine packages didn't build.

## [4.1.2] - 2026-06-10

### Fixed

- Packages didn't build on Debian, Fedora and Alpine.

## [4.1.1] - 2026-06-10

### Fixed

- Packages didn't build on Debian and Alpine.

## [4.1.0] - 2026-06-10

### Added

- A window for creating and editing gestures, with a shortcut recorder.
- GNOME's shortcuts are now actions.
- Support for XFCE.
- A warning when a gesture is too much like another.
- Packages for Debian and Ubuntu, Fedora and Alpine.

### Changed

- Rewritten in Rust, and built with Meson.
- Gestures are now recognized whatever their size or proportions.

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
