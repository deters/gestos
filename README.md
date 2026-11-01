Gestos - Simple mouse gestures for Linux
========================================

*Formerly MyGestures. See [Upgrading from MyGestures](#upgrading-from-mygestures).*

Mouse gestures - "draw" commands using your mouse.

Installing from source:
-----------------------

### Install dependencies:

#### Ubuntu / Debian:
```bash
sudo apt install pkg-config libgtk-4-dev libevdev-dev git meson ninja-build cargo
```

#### Fedora:
```bash
sudo dnf install pkgconf-pkg-config gtk4-devel libevdev-devel git meson ninja-build cargo
```


#### Alpine:
```bash
apk add git meson cargo sudo make
apk add glib-dev cairo-dev gdk4 gdk-pixbuf-dev pango-dev gtk4.0-dev
```

### Build and Install:

You can compile and install `gestosd` either using the compatibility wrapper `Makefile` or directly using `meson`.

#### Option A: Using the Makefile wrapper (Recommended)
```bash
git clone https://github.com/deters/gestos.git
cd gestos/
make
sudo make install
```

#### Option B: Using Meson directly
```bash
git clone https://github.com/deters/gestos.git
cd gestos/
meson setup build
meson compile -C build
sudo meson install -C build
```

The installation process automatically installs the `gestosd` daemon and `gestos` settings app, installs the udev rules, reloads the udev system, and sets `gestosd` to run with Set-Group-ID (SGID) access to the `input` group. Your user is only added to the `input` group if setting SGID fails.

### Faster development builds (optional):

If you rebuild often, `clang` and `mold` link much faster, and lighter debug info makes the binaries smaller. This is only for development, and nothing needs it to build or run the project.

Install them with your package manager:

```bash
# Fedora
sudo dnf install clang mold
# Ubuntu / Debian
sudo apt install clang mold
# Alpine
apk add clang mold
```

Then add this to `~/.cargo/config.toml`:

```toml
[target.x86_64-unknown-linux-gnu]
linker = "clang"
rustflags = ["-C", "link-arg=-fuse-ld=mold"]

[profile.dev]
debug = "line-tables-only"
```

Change `x86_64-unknown-linux-gnu` to match your system if it differs (`rustc -vV` shows it as `host`). The next build recompiles everything once. This applies to all your Rust projects, and you lose local variables in a debugger, though panics keep their file and line numbers.

Usage:
------

- `gestosd`: Start the daemon. Uses default button (button 3 / right-click) on the default mouse device.
- `gestos`: Start the GTK4 GUI to start and stop gestures and edit them.

**To use gestures:** Hold the trigger button (default: right-click / button 3) and move the mouse to draw a gesture. Release the button to execute.

Creating gestures:
------------------

Click **+** to create a gesture, or search for what you want it to do. If an action matches your search and no gesture uses it yet, it's listed under **Create a Gesture For**. Choose it, draw the gesture, and save.

Starting automatically:
-----------------------

The first time you click **Start** in `gestos`, gestures are also set to start when you log in, and a note lets you undo that. After that, it's your choice: click the **Preferences** button and change **Start When I Log In**. If you upgraded from MyGestures, your setting is kept. This is the supported method, and it works on GNOME, KDE Plasma, and XFCE on every distribution.

Sway and Hyprland don't run autostart entries by themselves, so start the daemon from their config instead:

- **Sway** (`~/.config/sway/config`): `exec gestosd`
- **Hyprland** (`~/.config/hypr/hyprland.conf`): `exec-once = gestosd`

On systemd distributions, Fedora and source installs also include an optional user service, for people who prefer `systemctl --user enable --now gestosd`. The Debian package doesn't include it, and Alpine doesn't use systemd. Use one method only: the switch in `gestos` doesn't know about the service.

Starting and stopping gestures:
-------------------------------

Use the **Start** or **Stop** button in the title bar of `gestos`, or right-click the app icon and choose **Start Gestures** or **Stop Gestures**. From a terminal or a key binding, run `gestos --start` or `gestos --stop`. Stopping also stops the gesture trail overlay, and your mouse works normally. Closing the `gestos` window doesn't stop gestures.

If **Start When I Log In** is on, gestures start again at your next login.

Troubleshooting:
----------------

### Gestures not working or no response:
1. **Wrong device:** The daemon auto-detects standard pointer devices. If yours is not captured, find its path in `/dev/input/by-path/` and run `gestosd -d '/dev/input/by-path/event-mouse'`.
2. **Permissions:** Ensure you have installed the udev rules (see above) and configured permissions. By default, the udev rules restrict access to the `input` group for security. You can configure `gestosd` with Set-Group-ID (SGID) privileges to run securely, or fallback to standard `uaccess`/`input` group setups. Running the binary will output detailed instructions on both options.
3. **Trigger Button:** If you are on a laptop with a touchpad, you might need to use button 1 (left-click) instead of 3. Try `gestosd -b 1`. Only mouse and digitizer buttons can be triggers, and the device must have that button.
5. **Shortcuts on non-US layouts:** `keypress` keys are physical key positions on a US layout, so a shortcut can land on a different character on other layouts. A shortcut naming a key Gestos doesn't know is ignored, and the daemon logs an error.
4. **Missing /dev/uinput (e.g. Alpine Linux):** On some distributions like Alpine Linux, the `uinput` kernel module is not loaded by default, and `/dev/uinput` will not exist. Load it manually using `sudo modprobe uinput`. To make this persistent on boot, add `uinput` to `/etc/modules` (or `/etc/modules-load.d/uinput.conf`). *(Note: If you install Gestos using the pre-built packages (.deb, .rpm, .apk), the package installation automatically sets up the modules-load rules to load the uinput module on boot.)*

Upgrading from MyGestures:
--------------------------

MyGestures was renamed to Gestos in version 5.0.0. The `mygestures` package and repository are deprecated.

Install Gestos like any new version. The new packages replace `mygestures`, and `make install` removes what a source install left behind. Then:

- **Your settings carry over.** The first time `gestos` or `gestosd` starts, your configuration is copied from `~/.config/mygestures/` to `~/.config/gestos/`. The old folder is kept, so you can go back to the old version, and can be deleted once you no longer need it.
- **Autostart keeps working.** The old entry is replaced with one for `gestosd`, including any custom arguments.
- **A running MyGestures daemon is stopped automatically** when `gestosd` starts. Only a daemon that owns the D-Bus name is found, so stop any other old instance by hand.
- **A `mygestures` command remains** as a small wrapper that starts `gestosd`, so that old scripts keep working.
- **systemd users:** if you enabled the user service, enable the new one with `systemctl --user enable --now gestosd`.

Gestures configuration:
-----------------------

On first execution, Gestos will create a configuration file for your user:

`~/.config/gestos/gestos.yaml`

Gestures are defined using coordinate paths matched using the Protractor gesture recognition algorithm.

Example of `gestos.yaml`:

```yaml
global:
  "Close window":
    move: "0,0 0,100 100,100"  # drawn as an 'L' shape
    do: kill

  "Maximize window":
    move: "50,100 50,0"        # drawn upwards
    do: toggle-maximized

  "Copy":
    move: "100,0 0,0 0,100 100,100"
    do: keypress Control_L+C
```

Supported Actions:
------------------

### Window Management
- `do: maximize`          # Maximizes focused window
- `do: restore`           # Restores maximized window
- `do: iconify`           # Minimizes/hides window
- `do: toggle-maximized`  # Toggles window maximization state
- `do: raise`             # Raises window
- `do: lower`             # Lowers window
- `do: toggle-fullscreen` # Toggles window fullscreen state

### Program Operations
- `do: kill`              # Closes the active window
- `do: exec <cmd>`        # Executes a custom shell command (e.g. `exec gedit`)

### Keyboard/Mouse Simulation
- `do: keypress <keys>`   # Simulates a keyboard shortcut (e.g. `keypress Control_L+c`)
- `do: click <button>`    # Emulates a mouse click (e.g. `click 3` for right-click)

### Desktop/Compositor-Agnostic Actions
(Supported on Sway, Hyprland, GNOME, and KDE)
- `do: workspace-left`
- `do: workspace-right`
- `do: workspace-up`
- `do: workspace-down`
- `do: show-overview`     # Toggles activities overview
- `do: show-app-grid`     # Shows application view
- `do: show-desktop`
- `do: lock-screen`
- `do: terminal`
- `do: volume-up`
- `do: volume-down`
- `do: volume-mute`
- `do: media-play`
- `do: media-next`
- `do: media-prev`

License
-------
GPL-2.0-or-later
