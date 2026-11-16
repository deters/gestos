# Gestos

## No shortcuts through other programs

- Use a library Gestos already depends on before running a command-line program for the same
  thing: GSettings through `gio`, not `gsettings`; `/proc`, not `pgrep` or `pkill`; D-Bus through
  `zbus`, not `gdbus` or `busctl`.
- Never leave a helper program running beside the daemon or the input service. Anything needed
  for as long as Gestos runs happens in a thread of its own, with a GLib main loop if what it
  watches needs one.
- Don't copy a shortcut because the code around it takes one. Fix that code too, or say why not.
- When running a program really is the best choice, such as a user's own command or a tool that
  has no library, say so while discussing the change's edge cases, so the user decides.
