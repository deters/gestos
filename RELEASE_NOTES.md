# Gestos 6.8.0

## Added

- **Alpine for ARM.** An Alpine package for ARM computers, such as a virtual machine on an Apple silicon Mac.

## Changed

- **Only actions that work here.** GNOME's actions are offered on GNOME only, as KDE's are on KDE, and a gesture that uses another desktop's action says which desktop it works on.

## Fixed

- **Starting the input service on Alpine.** When it isn't running, Gestos now shows the command that starts it on systems without systemd.
- **Reports on systems without a text editor.** When no app can open the report, Gestos shows where it was saved and can copy it.
