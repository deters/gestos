# Unreleased

The first release under the name Gestos, formerly MyGestures. Your MyGestures config and autostart entry are carried over the first time it runs.

## Upgrading

**Gestos now uses a system service to reach your input devices.** `gestosd` is no longer installed as Set-Group-ID.

- **Packages:** the upgrade should take care of this.
- **From source:** run `sudo meson install -C build` or `sudo make install` again. Until then, gestures can't start.
- **Without systemd:** the service needs elogind. On Alpine, start it with `rc-service gestos-input start`, and add it with `rc-update add gestos-input` to start it at boot.

Some setups that worked before now stop with an error:

- **Running as root:** `gestosd` must run as your own user, so `sudo gestosd` no longer works.
- **Config permissions:** a config that other users can change is refused, as is one in a folder they can change or that belongs to them. The error says what to fix.

Being in the `input` group no longer has any effect on Gestos, so you can leave it if you only joined for this.

## New

- **Security:** see the [README](README.md#security) for what each part can access. In short, `gestosd` has no special rights and can't read what you type.
- **Modifier keys:** a gesture can require Ctrl, Alt, Shift or Super to be held.
- **Standard shortcuts:** the GNOME shortcuts, such as Copy (Ctrl+C) and Zoom In (Ctrl++), are now actions of their own.
- **Search first:** gestures are created from the search box, which suggests every action no gesture uses yet.
- **Similar gestures:** if a drawing matches several gestures, a chooser appears at the pointer.
- **Reduce Notifications:** a setting for hiding the notification that says which gesture ran.
- **Keyboard layouts:** shortcuts press the right keys on any layout.
- **Locked screen:** gestures are ignored while the screen is locked, on desktops that report it to logind.
- **A new look:** Start and Stop in the header bar, a smaller gesture editor following the GNOME guidelines, a new icon, and icons of their own for GNOME actions.

## Changed

- Gestos ships without any gestures, so you create the ones you want.
- Each gesture has exactly one action. An older config listing several still loads, using the first.
- Opening Gestos starts gestures, unless you stopped them yourself.
- GNOME actions with no shortcut assigned now run, and start sooner. If one sometimes does nothing, try setting `GESTOS_SHORTCUT_DELAY_MS` to `300`.
- Custom Keyboard Shortcut replaces Keypress Shortcut.
