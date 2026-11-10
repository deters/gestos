# Gestos 7.0.0

## Added

- **Show the overview with the mouse.** When turned on, clicking left while holding right shows GNOME's overview.
- **Betas.** Gestos now has beta releases to try changes early. About and the debug report say which beta is installed.

## Changed

- **Tiling fits the windows you really have.**
  - Windows on other monitors are tiled too.
  - Dialogs, picture-in-picture videos, windows kept on top and drop-down terminals float over the tiles.
  - Windows that can't be resized keep their size, and the others make room for them.
  - Windows that open maximized or snapped stay that way.
  - The other windows are tiled beside a snapped one.
- **Smoother maximizing.** A tiled window maximizes straight from its tile, and gets its own size back when it's back to normal.
- **Tiling works alongside Ubuntu's Tiling Assistant.** Tiling no longer waits while another tiling extension is on, which on Ubuntu meant it never happened.

## Fixed

- **GNOME Shell crashing.** Closing a window that was maximized, or shown on every workspace, no longer crashes GNOME Shell.
- **Windows keep their own size.** Each window gets its own size back after tiling, even after GNOME Shell restarts, or when an app closes all its windows at once or quits while maximized.
- **Keyboard remaps.** Gestures held with a modifier, and shortcuts Gestos presses, follow layouts that move keys, such as Ctrl and Win swapped.
- **Modifier gestures after updating.** Gestures held with a modifier keep working until you log out after an update.
- **Saving before closing.** A window kept open to ask whether to save shows at once, and goes back to its tile or stays maximized.
- **Terminals** that only take whole lines and columns aren't resized over and over.
- **The gesture path** stays under the pointer.

Log out and back in after updating, so GNOME loads the new extension.
