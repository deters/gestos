# Changelog

All notable changes to Gestos are written here, since it was written in Rust. Versions before
6.0.0 were released as MyGestures.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and Gestos follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- New row "Shortcuts in Apps" on the "Keyboard Shortcuts" page: opens the list of apps, where each app's page has its own shortcuts.

### Changed

- Rows that open the gesture editor no longer end in an arrow. Only rows that open another page, like an app or a category, have one.
- The install script now also removes an older copy of Gestos installed from the source in `/usr/local`, which ran in place of the one it installed, and lists the files it removed.

## [9.1.3] - 2026-11-21

### Changed

- The install script now removes an older copy of Gestos installed for your user in `~/.local`, which ran in place of the one it installed, and lists the files it removed. Before, it only warned about that copy. Action templates of your own in `~/.local/share/gestos/action-templates` stay.

### Fixed

- The handle for reordering gestures didn't show a hand pointer, and Gestos printed warnings about it when started from a terminal.

## [9.1.2] - 2026-11-21

### Changed

- The install script now warns when another copy of Gestos installed for your user, in `~/.local`, runs in place of the one it installed, and says how to remove it.

### Fixed

- On Fedora, installing or updating Gestos could leave the input service turned off. Gestures then didn't start, and Gestos said "Missing permissions: the Gestos input service isn't running". Installing Gestos now turns it on and starts it.

## [9.1.1] - 2026-11-21

### Fixed

- After updating Gestos, it could say "Missing permissions: the Gestos input service isn't running", and gestures stayed off until you started them again. An update restarts the input service, and gestures now wait up to a minute for it before saying it's missing.

## [9.1.0] - 2026-11-21

### Added

- New install script, which installs or updates Gestos with the package for your system: `curl -fsSL https://github.com/deters/gestos/releases/latest/download/install.sh | sh` (Ubuntu, Debian, Fedora, Alpine).
- With `sh -s -- --beta` instead of `sh`, the install script installs the beta, when there's one newer than the release.
- On other systems that install deb, rpm or apk packages, like openSUSE, the install script says Gestos wasn't tested there and asks before installing.
- New option "Use Variables" in the "Type" step of an "Advanced Action". With it, the text to type can include `{{clipboard}}`, `{{selection}}`, `{{files}}`, `{{app}}` and `{{gesture}}`, which hold the same values as the variables a command gets. It can also include `{{result}}`, which is what the command before the step printed. For example, the steps "Copy", "Command" and "Type" with `{{result}}` replace the selected text with what a command made of it. "Insert a Variable" next to the text lists them all.
- A "Type" step with variables types at most 4000 characters. When the text is longer, nothing is typed and Gestos says the text is too long.

### Fixed

- Gestos restarts its gestures when the mice change, when the mouse buttons are swapped, and after an update. Each restart left one finished process behind for each mouse, and these stayed until you logged out. Thanks to @jatenlee (#29).

## [9.0.1] - 2026-11-19

### Fixed

- Gestures could stop working after waking the computer from sleep, until Gestos was started again. Thanks to @jatenlee (#27).

## [9.0.0] - 2026-11-18

### Added

- New actions for the files selected in a file manager, on its page in "Apps", above its keyboard shortcuts: "Convert to PDF", "Merge PDFs", "Compress PDFs", "Split PDF into Pages", "Convert PDF to Images", "Make a Dated Copy", "Convert Images to PNG", "Convert Images to JPG", "Make Images Half the Size", "Compress to ZIP", "Extract ZIP Files" and "Show Checksums". Thanks to @Jopp-gh (#26).
- New actions for the selected text: "Search the Web", "Translate", "Open Link", "Read Aloud", "Calculate", "Count Words" and "Show QR Code", which shows the text or link as a QR code to scan with a phone. These actions copy the selected text, so it replaces what was in the clipboard.
- New action "Copy Text from Screen": you select an area of the screen, and Gestos copies the text in it, including text in images and videos.
- These actions show their result, like "Converted 3 images". When they can't do anything, they say why, like "The selected images are PNGs already" or "Select at least two PDFs to merge".
- An action that needs a program you don't have says which package to install.
- A gesture made with one of the new file or text actions runs the steps of the installed version, so updates to Gestos change what the gesture does. Clicking "Customize Action…" in the gesture editor gives the gesture its own copy of the steps to change. Updates no longer change that copy.
- New page "Actions" in Preferences: each action made from a file, like the file and text actions, can be turned off there, so it isn't offered for new gestures. Gestures already made with it keep working. Each can also be exported to a file, which then opens in the default text editor.
- "Import…" on the "Actions" page adds your own action from a file, after showing the commands it runs. Your actions are kept in your configuration with your gestures, so exporting the configuration includes them. Each action file has a short ID, like `id: convert-to-pdf`. One you import with the same ID as one of Gestos's takes its place until you remove it. An action file can name an icon from the icon theme with `icon:`, which the action and its gestures are shown with.
- New step "Copy" in an "Advanced Action": it copies the selected files or text for the command after it. It uses the copy shortcut the app lists, Ctrl+Shift+C in terminals, and Ctrl+C in other apps. In a window Gestos can't recognise, it copies nothing and the action stops.
- New step "Result" in an "Advanced Action": it shows up to the last 24 lines the command before it printed, for the number of seconds set in the step. What a command prints is shown only by this step.
- A result printed by a command, like the checksums from "Show Checksums", is shown in a fixed-width font, with its lines lined up on the left (GNOME, X11).

### Changed

- The main page now lists your gestures above the categories. The option "Show Gestures in the Main Page" in Preferences is now "Hide Gesture List in the Main Page", which shows only the categories.
- A category's page, like "Apps", and an app's page now open in the main window instead of a window of their own. Go back with the back button at the top left, Esc, Alt+Left or the mouse's back button.
- A new gesture is now drawn first, in a window of its own that says which mouse button draws gestures. The editor opens once the gesture is drawn. Press Esc in that window to cancel.
- To draw a gesture again, click its drawing in the editor. It no longer clears the drawing: it opens the window to draw in, and pressing Esc there keeps the old drawing.
- Gestures now work while the gesture editor is open. They stop only while you draw a gesture.
- Commands in "Run Command in Shell" and in the "Command" steps of an "Advanced Action" are now edited in a window of their own, with room for a script of several lines and the option "Use Variables". Open it with the edit button on the command's row. In that window, Enter starts a new line and Ctrl+Enter sets the command. Keywords, function names, quoted text, variables, comments and operators are shown in colors. A command inside the command, like `$(date)`, has colors of its own. The command's box grows with the window.
- A new "Command" step is now added only after you click "Set" in that window.
- A command in "Run Command in Shell" or in a "Command" step of an "Advanced Action" that starts with a line like `#!/usr/bin/python3` is now run by the program that line names, as a script file is. Such a command can be written in Python or another language. Other commands are still run by sh.
- The button "Insert a Variable" in the window a command is edited in is now hidden while the command's first line names a program that isn't a shell, like Python.
- When a command fails, its notification now shows the last lines of the command's error message, in place of its exit code.
- "Export…" in Preferences now suggests a file name with the date, like "Gestos 2026-11-18.yaml", and opens the exported file in the default text editor.
- The configuration file now starts with the version of Gestos that saved it, like "# Gestos 9.0.0 configuration".

## [8.1.1] - 2026-11-17

### Changed

- Gestos now runs two background processes instead of three. Thanks to @Jopp-gh (#23).

### Fixed

- After Gestos was updated, the gestures went on running the old version until you logged out. They now restart on their own when Gestos is updated.
- In an "Advanced Action", cancelling the keys of a new "Shortcut" step left a step that pressed nothing. Clearing the keys of a step now removes it.
- Plugging in or unplugging a monitor while windows were tiled could crash GNOME and log you out.
- A mouse plugged in or connected after Gestos started, like a Bluetooth mouse, didn't draw gestures until Gestos was started again. A mouse plugged back in could also stop drawing them.
- Gestos didn't start when no mouse was connected. It now starts and waits for one.
- The path of a gesture wasn't drawn on a monitor plugged in after Gestos started (X11).
- After plugging in or unplugging a monitor, the path of the next gesture could be drawn on the wrong screen, or not at all (KDE).
- The windows that "Export…", "Import…" and "Remove All…" open in Preferences belonged to the main window instead of Preferences. They could open on another monitor, apart from Preferences.

## [8.1.0] - 2026-11-16

### Added

- New option "Control Windows With the Middle Button" in Settings: drag a window with the middle button to move it, or scroll while holding the button to resize it (GNOME).
- The action "Advanced Action" can now run commands, so one gesture can open an app, press shortcuts and run a script.
- Each app's page can now create an "Advanced Action" that works only in that app.
- Gestures can now run a command on the files or text you copied, or the text you selected (option "Use Variables", in the actions "Run Command in Shell" and "Advanced Action"). Thanks to @Jopp-gh (#25).
- New option "Allow Middle Click Paste" in Settings: turns pasting with the middle button on or off (GNOME).
- New action "Toggle Middle Click Paste": turns pasting with the middle button on or off (GNOME).
- New action "Toggle Middle Button Window Control": turns moving and resizing windows with the middle button on or off (GNOME).

### Changed

- Dragging a tiled window now only moves the windows in its way, instead of all of them (GNOME).
- "Import Configuration" in Settings now shows the commands the gestures would run, before importing.

## [8.0.1] - 2026-11-15

### Fixed

- Gestures drawn while holding Ctrl, Alt, Shift or Super were saved without those keys on X11. Thanks to @Jopp-gh (#21).

## [8.0.0] - 2026-11-15

### Added

- Actions are now grouped in categories, such as Windows, Navigation and Sound and Media. All your gestures can still be shown in one list, with the option "Show Gestures in the Main Page" in Settings.
- New page for each app, with its gestures and its shortcuts. Any of those shortcuts can be made into a gesture there.
- New panel that shows what a gesture ran, or why nothing ran (GNOME, KDE, X11).
- Now Gestos will also show the gesture path on KDE and X11. Thanks to @Jopp-gh (#18).
- New action "Look Up Selection": opens the selected text as a link, works out a sum, or searches the web for it.
- Now Gestos will also detect the app in use on Sway and Hyprland, and allow gestures per app there.

### Changed

- Gestures are now matched by the directions drawn, instead of the exact shape, so quick or uneven drawings are recognized more often. Thanks to @Jopp-gh (#24).
- The gesture editor now shows an example of how to draw a gesture.
- The shortcut recorder now also records shortcuts the desktop uses itself, like Ctrl+Alt+Up, and modifier keys alone. Thanks to @Jopp-gh (#21).
- Deleting a gesture no longer asks for confirmation, and can be undone instead.
- Added keyboard shortcuts: Ctrl+W closes a window, Ctrl+Q quits, and Ctrl+? lists them all.

### Fixed

- GNOME's Extensions app showed the wrong version of the extension.
- Gestures for one app were slow with many apps installed.

## [7.1.0] - 2026-11-10

### Changed

- Windows snapped to half the screen now stay there, and tiling fits the other windows beside them (GNOME).

## [7.0.0] - 2026-11-10

### Added

- Tiling now also works on other monitors. Dialogs and videos stay on top instead of taking a tile (GNOME).
- New option "Show Overview With Both Buttons" in Settings: holding right and clicking left shows all windows (GNOME, KDE).

### Changed

- Windows now get back the size you gave them after tiling or maximizing, even after the app is reopened (GNOME).

### Fixed

- Tiling didn't start on Ubuntu.
- Closing some windows could crash GNOME and log you out.
- Gestures and shortcuts didn't work with swapped keys, such as Ctrl and Win. Thanks to @Jopp-gh (#18).

## [6.9.0] - 2026-11-09

### Added

- New actions "View Split on Left" and "View Split on Right": put a window on half the screen (GNOME).

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

- On systems without systemd, when gestures can't start, Gestos now shows the command that starts them.
- When no app can open the bug report, Gestos now shows where it was saved and can copy it.

## [6.7.0] - 2026-11-09

### Added

- Now Gestos will also detect the app in use on KDE and X11, and allow gestures per app there. Thanks to @Jopp-gh (#18).

### Fixed

- Apps with the same name couldn't be told apart. Nemo and Nautilus, both called Files, now show as Files (Nemo) and Files (Nautilus).
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
- New option "Show Gesture Path" in Preferences: draws each gesture on screen as you draw it (GNOME).
- New option "Draw With" in Preferences: draws gestures with the right, middle, back or forward button. Left-handed mice use the other side. Thanks to @Jopp-gh (#18).
- New option "Debugging Information" in Preferences: creates a report to attach to a bug report.

### Changed

- Gestos now only notifies when something goes wrong.
- Preferences is now grouped into sections.
- The gesture editor now names each step of a custom action.
- The drawing area in the gesture editor now keeps its size.

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

- New action "Custom Keyboard Action": presses shortcuts, types text and waits, in the order you set. The text can have several lines and accented letters. It replaces "Custom Keyboard Shortcut".
- New option "Tile Automatically" in Preferences: tiles every workspace, new ones too (GNOME).
- Gestos now says when its GNOME extension is out of date, and how to update it.

### Changed

- Tiling now keeps windows at their own sizes, moving them just enough not to overlap (GNOME).
- Tiling now stays on until turned off, also after logging out (GNOME).
- On GNOME, the card that asks which gesture was meant now appears at the pointer, and the window keeps its focus.
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

- Moving a tiled window puts all windows back at their own sizes again, as before 6.2.0 (GNOME).

### Fixed

- Big windows overlapped when tiled. They're now laid out in a grid when they can't all shrink enough.

## [6.2.0] - 2026-11-03

### Changed

- Moving a tiled window now keeps the workspace tiled: the window takes a tile where it's left (GNOME).

## [6.1.3] - 2026-11-03

### Fixed

- GNOME crashed when a window on a second monitor closed while tiling.
- The action "Close Window" now offers to force quit a window that doesn't respond.

## [6.1.2] - 2026-11-03

### Fixed

- On GNOME, the action "Close Window" now closes windows like their close button does.
- On GNOME, shortcuts pressed by gestures now start sooner.
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
- New Gestos GNOME extension, needed for tiling. It comes with Gestos: after installing, log out and back in, then turn it on in Preferences.
- Tiled windows get their own size back before they're closed, so apps don't remember the tile's size (GNOME).

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

[Unreleased]: https://github.com/deters/gestos/compare/v9.1.3...HEAD
[9.1.3]: https://github.com/deters/gestos/compare/v9.1.2...v9.1.3
[9.1.2]: https://github.com/deters/gestos/compare/v9.1.1...v9.1.2
[9.1.1]: https://github.com/deters/gestos/compare/v9.1.0...v9.1.1
[9.1.0]: https://github.com/deters/gestos/compare/v9.0.1...v9.1.0
[9.0.1]: https://github.com/deters/gestos/compare/v9.0.0...v9.0.1
[9.0.0]: https://github.com/deters/gestos/compare/v8.1.1...v9.0.0
[8.1.1]: https://github.com/deters/gestos/compare/v8.1.0...v8.1.1
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
