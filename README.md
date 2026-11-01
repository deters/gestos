# Gestos

## Get started

Search some action and create a gesture:

![Searching for "screen" lists matching actions to create a gesture for](docs/screenshots/search-suggestions.jpg)

Draw the gesture and save it. Gestos warns you if your drawing is close to one you already have:

![The gesture editor with a drawn path and the Take Screenshot action](docs/screenshots/gesture-editor.jpg)

That's it. Hold the right mouse button anywhere, draw your gesture, and let go. A short click still works as a normal right click.

## Install

Packages for Fedora, Debian, and Alpine are on the [releases page](https://github.com/deters/gestos/releases).

### From source

Install the dependencies:

```bash
# Fedora
sudo dnf install pkgconf-pkg-config gtk4-devel libevdev-devel git meson ninja-build cargo

# Ubuntu / Debian
sudo apt install pkg-config libgtk-4-dev libevdev-dev git meson ninja-build cargo

# Alpine
apk add git meson cargo sudo make glib-dev cairo-dev gdk4 gdk-pixbuf-dev pango-dev gtk4.0-dev
```

Then build and install:

```bash
git clone https://github.com/deters/gestos.git
cd gestos
meson setup build --buildtype=release
meson compile -C build
sudo meson install -C build
```

## Using Gestos

### Choose between similar gestures

When a drawing matches several gestures almost equally, they appear at the pointer. Click the one you meant. Clicking anywhere else, pressing `Esc`, or waiting 15 seconds does nothing.

### Start and stop

Gestures start when you open Gestos, and the first time, they're also set to start when you log in, with a note that lets you undo that. If you stop them, opening Gestos won't start them again, though **Start When I Log In** still does at your next login.

Use the **Stop** or **Start** button in the title bar, or right-click the app icon and choose **Start Gestures** or **Stop Gestures**. In a terminal or a key binding, use:

```bash
gestos --start
gestos --stop
```

Closing the window doesn't stop gestures. While they're stopped, your mouse works normally.

### Start when you log in

Click the **Preferences** button and change **Start When I Log In**. This works on GNOME, KDE Plasma, and XFCE.

Sway and Hyprland don't run autostart entries by themselves, so add a line to their config instead:

| Desktop | File | Line |
|---|---|---|
| Sway | `~/.config/sway/config` | `exec gestosd` |
| Hyprland | `~/.config/hypr/hyprland.conf` | `exec-once = gestosd` |

Fedora and source installs also include an optional systemd user service, which you can use with `systemctl --user enable --now gestosd`. Use only one of these methods, because the setting in Gestos doesn't know about the service.

### Keyboard shortcuts in the app

| Keys | Action |
|---|---|
| Just start typing, or `Ctrl+F` | Search |
| `Esc` | Clear the search |
| `Ctrl+N` | New gesture |
| `Ctrl+,` | Preferences |
| `Ctrl+Q` | Close the window |

## Configuration

Gestures are saved in `~/.config/gestos/`, in a file named after your desktop, such as `gestos_gnome.yaml`. On a desktop Gestos doesn't recognize, the file is `gestos.yaml`. You can edit it by hand, and **Preferences** can export and import it.

```yaml
global:
  "Close window":
    move: "0,0 0,100 100,100"  # An L shape, drawn down and then right
    do: kill

  "Copy":
    move: "100,0 0,0 0,100 100,100"
    do: keypress Control+C

  "Open browser and maximize":
    move: "0,100 50,0 100,100"
    do:
      - www
      - maximize
```

`move` is the path as `x,y` points, and `do` is one action or a list of them. The `gestos` app edits one action per gesture, so saving a change there to a gesture with several actions keeps only the action you chose.

### Actions

| Action | What it does |
|---|---|
| `keypress <keys>` | Sends a keyboard shortcut, such as `keypress Control+C` |
| `exec <command>` | Runs a shell command, such as `exec gedit` |
| `click <button>` | Clicks a mouse button, such as `click 3` |
| `kill` | Closes the window |
| `maximize`, `restore`, `toggle-maximized` | Maximizes or restores the window |
| `iconify` | Minimizes the window |
| `raise`, `lower` | Moves the window to the front or back |
| `toggle-fullscreen` | Toggles fullscreen |
| `show-desktop` | Shows the desktop |
| `workspace-left`, `workspace-right`, `workspace-up`, `workspace-down` | Switches workspace |
| `show-overview`, `show-app-grid` | Opens the overview or the app grid |
| `volume-up`, `volume-down`, `volume-mute` | Changes the volume |
| `media-play`, `media-next`, `media-prev` | Controls media playback |
| `www`, `home`, `email`, `search`, `calculator`, `terminal`, `control-center` | Opens that app |
| `screenshot`, `screenshot-window`, `screenshot-area` | Takes a screenshot |
| `lock-screen`, `logout` | Locks the screen or logs out |
| `gnome <key>` | Runs a GNOME shortcut, such as `gnome maximize-vertically` |
| `kde <component> <shortcut>` | Runs a KDE shortcut |
| `abort` | Disables the gesture |

`keypress` uses physical key positions on a US layout, so on other layouts a shortcut can land on a different character. A shortcut naming a key Gestos doesn't know is ignored, and the daemon logs an error.

### Daemon options

`gestosd` is the background program that recognizes gestures. It normally needs no options.

| Option | Meaning |
|---|---|
| `-b`, `--button <button>` | The button to draw with. The default is `3`, the right button |
| `-d`, `--device <path>` | The device to use. Repeat it for several devices |
| `-s`, `--sensitivity <value>` | Distance between recorded points, in tenths of a pixel. The default is `30` |
| `<file>` | A configuration file to use instead of the default |

## Troubleshooting

**Nothing happens when I draw**
- Check that Gestos shows **Running** in the title bar.
- If your mouse isn't detected, find it in `/dev/input/by-path/` and run `gestosd -d /dev/input/by-path/<your-mouse>`.
- On a touchpad, you might need the left button: `gestosd -b 1`. A trigger must be a mouse or digitizer button that the device has.

**Gestos can't access my mouse**

The installer sets up access for you, using udev rules and a Set-Group-ID `gestosd` that can use the `input` group. Reinstalling usually fixes this. If not, **Details** in the error lists the steps. Adding your user to the `input` group also works, but it lets every program you run read your keyboard.

**`/dev/uinput` is missing**

The `uinput` kernel module isn't loaded, which is common on Alpine when installing from source. Load it with `sudo modprobe uinput`, and add `uinput` to `/etc/modules` to keep it after a restart. The packages set this up for you.

## Upgrading from MyGestures

MyGestures was renamed to Gestos in version 5.0.0, and the `mygestures` package and repository are deprecated. Install Gestos as usual: the packages replace `mygestures`, and installing from source removes what a source install left behind. Then:

- **Your gestures carry over.** They're copied from `~/.config/mygestures/` the first time Gestos runs. The old folder is kept so that you can go back, and you can delete it when you no longer need it.
- **Autostart keeps working.** The old entry is replaced with one for `gestosd`, including any custom arguments.
- **A running MyGestures daemon is stopped automatically** when `gestosd` starts. Only the one that owns the D-Bus name is found, so stop any other instance by hand.
- **A `mygestures` command remains** as a small wrapper that starts `gestosd`, so that old scripts keep working.
- **If you enabled the systemd service,** enable the new one with `systemctl --user enable --now gestosd`.

## Development

`make` builds an unoptimized version in `build/`, and `sudo make install` installs it.

```bash
make
dbus-run-session -- cargo test
```

The tests use a private D-Bus session, and some are skipped without one.

### Faster rebuilds

`clang` and `mold` link much faster, and lighter debug info makes the binaries smaller. Nothing needs them to build or run the project. Install them with your package manager, then add this to `~/.cargo/config.toml`:

```toml
[target.x86_64-unknown-linux-gnu]
linker = "clang"
rustflags = ["-C", "link-arg=-fuse-ld=mold"]

[profile.dev]
debug = "line-tables-only"
```

Change `x86_64-unknown-linux-gnu` to match your system if it differs (`rustc -vV` shows it as `host`). The next build recompiles everything once. This applies to all your Rust projects, and you lose local variables in a debugger, though panics keep their file and line numbers.

## License

GPL-2.0-or-later. See [COPYING](COPYING).
