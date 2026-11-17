#!/bin/sh
# Builds Gestos and installs it for your user, then restarts it so that the new version is what's
# running. Only the input service, which reads the input devices for everyone, is installed for
# the whole system, and only when it changed, so the password is asked for only then.
#
# Usage: ./reinstall.sh             build and install
#        ./reinstall.sh --remove    remove the copy installed for your user, so that the one
#                                   installed for the system, such as a package, runs again
set -e

cd "$(dirname "$0")"

local_root=${XDG_DATA_HOME:-$HOME/.local/share}
local_bin=$HOME/.local/bin
# Where your own action templates are, which Gestos reads before the system's. The ones from this
# checkout go there too, to test them, in place of any installed with the same names.
local_templates=$local_root/gestos/action-templates

# Where each file of the build is installed for the system, as "built file<TAB>installed file".
installed_files() {
    meson introspect build --installed |
        python3 -c 'import json, sys; [print(f"{s}\t{d}") for s, d in json.load(sys.stdin).items()]'
}

# Where a file installed for the system goes for your user instead, or nothing for one only the
# system can have. The rest of the system's copy is left as it is, behind your user's.
user_copy() {
    case $1 in
        */libexec/*|*/systemd/system/*|/etc/*|/lib/*|*/bin/mygestures|*/share/gestos/*) ;;
        */bin/*) echo "$local_bin/${1##*/}" ;;
        */lib/systemd/user/*) echo "$local_root/systemd/user/${1##*/}" ;;
        */share/*) echo "$local_root/${1#*/share/}" ;;
    esac
}

if [ "$1" = --remove ]; then
    installed_files | while IFS="$(printf '\t')" read -r built system; do
        copy=$(user_copy "$system")
        [ -n "$copy" ] && rm -f "$copy"
    done
    # Only the ones from this checkout, and not any you made.
    for template in action-templates/*.yaml; do
        rm -f "${local_templates:?}/${template##*/}"
    done
    rmdir "$local_templates" 2>/dev/null || true
    rmdir -p "$local_root/gnome-shell/extensions/gestos@deters.github.io/schemas" \
        "$local_root/gnome-shell/extensions/gestos@deters.github.io/lib" 2>/dev/null || true
    systemctl --user daemon-reload 2>/dev/null || true
    echo "Removed the copy installed for your user. Log out and back in to load the system's extension."
    exit 0
fi

# Built first, so that a failed build leaves the running version alone, and as your user, so that
# the build folder doesn't end up owned by root.
make

# The input service, its units and its device rules, installed for the system only when one of
# them differs from what's there.
system_changed=no
installed_files > build/installed-files.tsv
while IFS="$(printf '\t')" read -r built system; do
    [ -n "$(user_copy "$system")" ] && continue
    case $system in */share/gestos/*|*/bin/mygestures) continue ;; esac
    if ! cmp -s "$built" "$system"; then
        echo "Changed for the system: $system"
        system_changed=yes
    fi
done < build/installed-files.tsv
if [ "$system_changed" = yes ]; then
    sudo make install
fi

# Everything else, for your user. The files that name where the daemon is name your copy.
while IFS="$(printf '\t')" read -r built system; do
    copy=$(user_copy "$system")
    [ -z "$copy" ] && continue
    mkdir -p "$(dirname "$copy")"
    case $system in
        */dbus-1/services/*|*/systemd/user/*)
            sed "s|/[^ =]*/bin/gestosd|$local_bin/gestosd|" "$built" > "$copy.new" ;;
        *) cp "$built" "$copy.new" ;;
    esac
    [ -x "$built" ] && chmod 755 "$copy.new" || chmod 644 "$copy.new"
    mv -f "$copy.new" "$copy"
done < build/installed-files.tsv
# Gestos leaves out templates other users can change, so the folder's permissions are set too.
mkdir -p "$local_templates"
chmod 755 "$local_templates"
for template in action-templates/*.yaml; do
    install -m 644 "$template" "$local_templates/${template##*/}"
done
systemctl --user daemon-reload 2>/dev/null || true
update-desktop-database -q "$local_root/applications" 2>/dev/null || true
gtk-update-icon-cache -q -t -f "$local_root/icons/hicolor" 2>/dev/null || true
gnome-extensions enable gestos@deters.github.io 2>/dev/null || true

gestos=$local_bin/gestos

# Opening Gestos only brings an old window forward, so one that's open has to be stopped.
app_was_open=no
if pkill -x gestos; then
    app_was_open=yes
fi

"$gestos" --stop
"$gestos" --start

if [ "$app_was_open" = yes ]; then
    setsid "$gestos" >/dev/null 2>&1 &
fi

echo "Running version: $(busctl --user --auto-start=no get-property \
    io.github.deters.Gestos.Daemon /io/github/deters/Gestos/Daemon \
    io.github.deters.Gestos.Daemon Version 2>/dev/null || echo unknown)"
echo "Running: $(readlink "/proc/$(pgrep -x -u "$(id -u)" gestosd | head -n 1)/exe" 2>/dev/null || echo unknown)"
echo "If the GNOME extension changed, log out and back in to load it."
