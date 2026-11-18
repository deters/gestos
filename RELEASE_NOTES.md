# Gestos 8.1.1

## Changed

- Gestos now runs two background processes instead of three.

## Fixed

- After Gestos was updated, the gestures went on running the old version until you logged out. They now restart on their own when Gestos is updated.
- In an "Advanced Action", cancelling the keys of a new "Shortcut" step left a step that pressed nothing. Clearing the keys of a step now removes it.
- Plugging in or unplugging a monitor while windows were tiled could crash GNOME and log you out.
- A mouse plugged in or connected after Gestos started, like a Bluetooth mouse, didn't draw gestures until Gestos was started again. A mouse plugged back in could also stop drawing them.
- Gestos didn't start when no mouse was connected. It now starts and waits for one.
- The path of a gesture wasn't drawn on a monitor plugged in after Gestos started (X11).
- After plugging in or unplugging a monitor, the path of the next gesture could be drawn on the wrong screen, or not at all (KDE).
- The windows that "Export…", "Import…" and "Remove All…" open in Preferences belonged to the main window instead of Preferences. They could open on another monitor, apart from Preferences.

Update today, then log out and back in so GNOME loads the new version.
