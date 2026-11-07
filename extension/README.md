# Gestos GNOME extension

Gives [Gestos](https://github.com/deters/gestos) window tiling, which GNOME has no shortcut for. GNOME on Wayland only lets its own Shell see and move other windows, so these have to run inside it.

## Tiling

**Toggle tile windows:** fits the windows on the current workspace together without overlaps, each near where it was and sized in proportion to the others' original sizes. No tile gets smaller than a quarter of the screen's width or a third of its height, so up to 12 windows fit, 4 across and 3 down. A window that says it can't get smaller than half the screen gets a tile like the others, as some apps say that while they open. Windows that are maximized or snapped to half the screen are restored first. Windows that open, close, or move in or out while it's tiled join or leave, and a workspace stays tiled with no windows left. Resizing a tiled window keeps it tiled: the edges you drag stay where you leave them, and the tiles beyond them make room. Opening or closing windows places the tiles again from the original sizes. A second use, or moving, maximizing, or snapping any of them, puts them all back.

**Toggle arrange windows:** keeps the windows on the current workspace from overlapping, at their own sizes. Windows are moved just enough to fit beside each other, in the order they have across the screen, and only shrink, along the side they don't fit, when moving isn't enough. They never grow past their original size, and get it back once there's room again. Moving or resizing a window makes the others make room for it as you go, and the size and place you leave it at become its own. The window you moved last keeps its size and place where there's room for the others. When even the smallest sizes don't fit, the windows touched longest ago are minimized, and showing one again makes room for it. Windows that open join where they open. Using Toggle Tile Windows on an arranged workspace tiles it instead, and the other way around. A second use, or maximizing or snapping any of them, puts them all back.

### Original sizes

Apps remember the size their windows had while they weren't maximized, and save it when they close. So tiled windows get their original size back, and the app is given time to take it, before:

- tiling ends;
- they're closed from GNOME: the close shortcut, the window menu, or the overview;
- they're maximized or made fullscreen, so that's their size when they're restored;
- you confirm logging out, restarting, or shutting down, or the extension is turned off.

The window's picture is held while this happens, so it doesn't visibly change size before it closes or maximizes. A window snapped to half the screen gets its original size back when the snap is undone. While the screen is locked, everything stays as it is.

An app closing its own window, with its close button or a shortcut like Ctrl+Q, can't be caught in time, so it saves the size it was given. The next window that app opens at that size gets its original size instead, in that session or a later one, for up to 30 days. Windows the extension didn't resize are left as their apps open them.

## Settings

**Window Gap:** the pixels between tiled windows and at the edges of the screen. Change it in Gestos's Preferences or the Extensions app, or run:

```bash
gnome-extensions prefs gestos@deters.github.io
```

## Install

Gestos installs this for all users, unless it's built with `-Dgnome_extension=false`. To try changes, `./reinstall.sh` installs this folder for your user, which takes priority over that copy.

GNOME only finds extensions when you log in, so log out and back in, then enable it:

```bash
gnome-extensions enable gestos@deters.github.io
```

## D-Bus

Tiling is on GNOME Shell's bus name, and returns whether any window changed.

```bash
gdbus call --session --dest org.gnome.Shell \
  --object-path /io/github/deters/Gestos/Extension \
  --method io.github.deters.Gestos.Extension.ToggleTileWindows
```

Arranging works the same way, with `ToggleArrangeWindows`.

## Development

```bash
make test
```

To try changes without touching your session, run a nested Shell:

```bash
dbus-run-session -- gnome-shell --devkit --wayland
```

## License

GPL-2.0-or-later. See [COPYING](../COPYING).
