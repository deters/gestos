# Gestos

Mouse gestures for Linux.

![A GNOME desktop with tiled windows, Gestos's list of gestures, and a gesture's path drawn over Files](docs/screenshots/desktop.png)

## Features

- Different gestures for each app, on GNOME, KDE and X11 desktops
- Ctrl, Alt, Shift and Super turn one gesture into several
- Draw with the right, middle, back or forward button
- Wayland and X11
- GNOME, KDE, Sway, Hyprland and XFCE

![Preferences: start at login, the button to draw with, the gesture path and window tiling](docs/screenshots/preferences.png)

## Actions Supported

- The actions GNOME and KDE expose, such as moving windows, switching workspaces and media controls
- Opening an app, or switching to it if it's already open
- Keyboard shortcuts
- Typing text, line breaks and accents included
- Shell commands
- Custom actions that chain apps, shortcuts, text and pauses

![Searching for "maximize" lists GNOME's window actions to create a gesture for](docs/screenshots/search-actions.png)

![A custom action that opens Camera and presses two shortcuts, drawn as an R with Super held](docs/screenshots/custom-action.png)

## GNOME Gestos Extension

- The path shows on screen while you draw
- Advanced window tiling that keeps windows from overlapping

![A Next Track gesture limited to Audio Player](docs/screenshots/gesture-per-app.png)

## Install

Packages for Fedora, Debian and Alpine are on the [releases page](https://github.com/deters/gestos/releases).

On GNOME, log out and back in after installing, then turn on the Gestos extension in Preferences.

### From source

Install the dependencies:

```bash
# Fedora
sudo dnf install gtk4-devel libadwaita-devel meson ninja-build cargo gcc git

# Debian 13
sudo apt install libgtk-4-dev libadwaita-1-dev meson ninja-build cargo build-essential pkg-config git

# Alpine
apk add gtk4.0-dev libadwaita-dev meson cargo gcc musl-dev git
```

Ubuntu 24.04's Rust 1.75 is too old. Get 1.85 or newer from [rustup](https://rustup.rs).

Then build and install:

```bash
git clone https://github.com/deters/gestos.git
cd gestos
meson setup build --buildtype=release
meson compile -C build
sudo meson install -C build
```

To build without the GNOME extension, add `-Dgnome_extension=false` to `meson setup`.

Updating an older install? Run the install step again. It drops the Set-Group-ID bit `gestosd` used to have and starts the input service.

## Troubleshooting

When you [report a bug](https://github.com/deters/gestos/issues), attach the report from Preferences → Troubleshooting → Create Report, or the output of `gestos --debug-info`.

## Security

Reading the mouse takes access to your input devices. Only a small, sandboxed system service has it. The app itself runs as you, with no extra rights.

The service never passes on your keystrokes and only answers the user at the screen. It ignores gestures while the screen is locked, on desktops that report it.

Programs you run can still ask it to press keys. It guards against other users and keyloggers, not against software you already trust.

## Development

`make` builds an unoptimized version in `build/`, and `sudo make install` installs it.

```bash
make
dbus-run-session -- cargo test
```

The tests use a private D-Bus session, and some are skipped without one.

### Faster rebuilds

Optional: `clang` and `mold` link much faster, and lighter debug info makes smaller binaries. Install both, then add this to `~/.cargo/config.toml`:

```toml
[target.x86_64-unknown-linux-gnu]
linker = "clang"
rustflags = ["-C", "link-arg=-fuse-ld=mold"]

[profile.dev]
debug = "line-tables-only"
```

Use your own target if it isn't `x86_64-unknown-linux-gnu` (`rustc -vV` prints it as `host`). The next build starts from scratch. The settings apply to all your Rust projects, and debuggers no longer see local variables, but panics still show file and line.

## History

Gestos used to be called MyGestures.

## License

GPL-2.0-or-later. See [COPYING](COPYING).
