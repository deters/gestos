# Gestos 6.1.2

## Fixed

- **Close Window, on GNOME:** closes the focused window the way its close button does, through the “Gestos” GNOME extension. Without the extension it still presses GNOME's close shortcut.
- Shortcuts that gestures press on GNOME start sooner, as the keyboard layout is no longer looked up with `gsettings` each time.
- The cursor stays smooth on a busy desktop, as the input service now runs at a slightly higher priority.

Log out and back in after updating, so GNOME loads the new extension.
