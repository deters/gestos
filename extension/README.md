# Gestos GNOME extension

Gives [Gestos](https://github.com/deters/gestos) window tiling, which GNOME has no shortcut for, and asks which gesture you meant when one you drew is close to several. GNOME on Wayland only lets its own Shell see and move other windows, so these have to run inside it.

## Tiling

**Toggle tile windows:** keeps the windows on the current workspace from overlapping, at their own sizes. Windows are moved just enough to fit beside each other, in the order they have across the screen, and only shrink, along the side they don't fit, when moving isn't enough. No window gets smaller than a quarter of the screen's width or a third of its height, unless it's smaller already, and a window that says it can't get smaller than half the screen is believed to get as small as the others, as some apps say that while they open. Windows never grow past their original size, and get it back once there's room again.

Moving or resizing a window makes the others make room for it as you go, and the size and place you leave it at become its own. The window you moved last keeps its size and place where there's room for the others. When even the smallest sizes don't fit, the windows touched longest ago are minimized. They come back on their own once you leave room for them again, and showing one yourself makes room for it.

Windows that are maximized when it starts are restored first. Windows that open, close, or move in or out while it's tiled join or leave, and a workspace stays tiled with no windows left. Maximizing a window, making it fullscreen, or snapping it to half the screen sets it aside, and the others take its room until it's back to normal, when it joins again. It stays tiled until you use it again, which puts them all back, even after you log out and back in: windows get their original sizes back as the session ends, and the workspace is tiled again in the next one. A workspace GNOME removes, as it does with empty ones you leave, isn't tiled anymore.

Turn on **Tile Automatically** in Gestos's Preferences to tile every workspace, also ones GNOME adds later, without using Toggle Tile Windows on each. Toggle Tile Windows still stops tiling one workspace, which stays that way until the next session. Turning it off puts all of them back.

### Original sizes

Apps remember the size their windows had while they weren't maximized, and save it when they close. So tiled windows get their original size back, and the app is given time to take it, before:

- tiling ends;
- they're closed from GNOME: the close shortcut, the window menu, or the overview;
- they're maximized or made fullscreen, so that's their size when they're restored;
- you confirm logging out, restarting, or shutting down, or the extension is turned off.

The window's picture is held while this happens, so it doesn't visibly change size before it closes or maximizes. A window snapped to half the screen gets its original size back when the snap is undone. While the screen is locked, everything stays as it is.

An app closing its own window, with its close button or a shortcut like Ctrl+Q, can't be caught in time, so it saves the size it was given. The next window that app opens at that size gets its original size instead, in that session or a later one, for up to 30 days. Windows the extension didn't resize are left as their apps open them.

## Choosing a gesture

When a drawn gesture is close to several of yours, Gestos asks which one you meant. On GNOME, the extension asks with a card at the pointer, styled like GNOME's own menus. The Shell draws it, so no window opens or takes focus from the one the gesture acts on. Click a gesture to pick it, or move between them with the arrow keys or Tab and press Enter. Any other click, Escape, or waiting 15 seconds cancels. Without the extension, Gestos asks in a window of its own instead.

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
