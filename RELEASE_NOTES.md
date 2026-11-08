# Gestos 6.4.0

## Added

- **Gestures for some apps.** A gesture can be limited to the apps you pick, in its trigger. It then only works in them, and when a drawing matches both, the app's own gesture wins, so the same shape can do something else in some apps. This needs the GNOME extension.
- **Custom Action can open an app** before its other steps, or bring its window to the front if it's open, and the steps after it go to that app.
- **Custom actions have names.** A new one is named after what you searched for, offered as Create New Action in the results, and you can change it.

## Changed

- **Custom Keyboard Action is now Custom Action**, and **Run Command is now Run Command in Shell**.
- **The gesture editor** shows what the action does first, then what triggers it: the modifier keys and the drawing together, and the apps it works in. Its parts are as wide as the main window's lists, it opens as tall as what's in it, and Delete Gesture… comes last.
- **Starting and stopping gestures** from the app icon's menu no longer shows a notification. Only a failure to start does.

Log out and back in after updating, so GNOME loads the new extension.
