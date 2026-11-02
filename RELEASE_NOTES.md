# Gestos 6.1.0

## New

- **Toggle Tile Windows, on GNOME:** fits the windows on a workspace together without overlaps, each near where it was and sized in proportion to the others. Resizing a tile keeps the edge where you leave it. Using it again, or moving, maximizing, or snapping a window, puts them all back.
- **The “Gestos” GNOME extension:** tiling runs in it, and it comes with Gestos. After installing, log out and back in, then turn it on in Gestos's Preferences, which also shows whether it's running and has a Window Gap setting. Gestos works without it, and other desktops don't use it.
- **Original sizes kept:** tiled windows get their original size back before they're closed from GNOME, maximized, or left at logout, so apps don't save the tiled size. If an app closes one itself, the next window it opens at that size gets the original size instead.

## Fixed

- Gestos shows with its own icon in GNOME's app list and dock.

## Packaging

- The extension installs to `share/gnome-shell/extensions/gestos@deters.github.io`. Build with `-Dgnome_extension=false` to leave it out.
- Building it needs `glib-compile-schemas`, and its tests run with `gjs` when it's installed.
