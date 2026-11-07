# Gestos 6.3.0

## Added

- **Custom Keyboard Action** replaces Custom Keyboard Shortcut. It presses shortcuts, types text and pauses, in any order you set. Text can have several lines, and accented letters are typed with your keyboard layout's dead keys. A single shortcut works as before.
- **Tile Automatically**, in Preferences, tiles every workspace, also new ones.
- **Gestos tells you when its GNOME extension is out of date**, and how to update it: by logging out and back in, or by removing an old copy in your home folder.

## Changed

- **Tiling keeps windows at their own sizes.** Windows move just enough not to overlap, and only shrink when moving isn't enough. Moving or resizing a window makes the others make room for it, and windows minimized for lack of room come back once there's room again.
- **Tiling stays on** until you turn it off, also after logging out and back in. Maximizing a window, making it fullscreen, or snapping it sets it aside until it's back to normal.
- **On GNOME, the card that asks which gesture you meant** appears at the pointer, styled like GNOME's menus, and your window keeps its focus.
- **Run Command and Custom Keyboard Action are always offered** when you search for an action, as they can do what the others can't.
- **The shortcut recorder records any key**, Escape, Backspace, Tab and Enter included.

## Fixed

- **Gestos's own windows aren't tiled.**
- **The daemon can be started through D-Bus.**

Log out and back in after updating, so GNOME loads the new extension.
