#!/bin/sh
# Runs Gestos through GTK's Broadway backend, for recording the README's GIFs in a browser. It has
# its own home folder and D-Bus session, and a stand-in for the daemon, so it can't touch the real
# config, the mouse, or a running daemon. See README.md in this folder.
set -e

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
# Under the runtime folder because it's short: Broadway's socket path has to fit in 108 bytes.
dir="${XDG_RUNTIME_DIR:-/tmp}/gestos-record"
display=9
isolated="env -i PATH=/usr/bin HOME=$dir/home XDG_CONFIG_HOME=$dir/home/.config XDG_RUNTIME_DIR=$dir/run"

case "$1" in
server)
    # Runs until "stop". The browser must attach to it before "app" runs, because it can't catch
    # up on what was drawn before it connected.
    rm -rf "$dir"
    mkdir -p "$dir/home/.config/gestos" "$dir/run" "$dir/bin" "$dir/frames"
    chmod 700 "$dir/run"
    exec $isolated gtk4-broadwayd ":$display"
    ;;
app)
    # A copy on its own, so that it can't find and start the real daemon beside it.
    cp "$root/target/debug/gestos" "$dir/bin/gestos"
    cat > "$dir/run.sh" <<SESSION
#!/bin/sh
python3 -I "$here/fake-daemon.py" &
sleep 0.7
exec "$dir/bin/gestos"
SESSION
    chmod +x "$dir/run.sh"
    $isolated XDG_CURRENT_DESKTOP=GNOME GDK_BACKEND=broadway BROADWAY_DISPLAY=":$display" \
        LANG=en_US.UTF-8 setsid dbus-run-session -- "$dir/run.sh" >"$dir/app.log" 2>&1 &
    ;;
build)
    # Arguments are the GIF's name, then frame:delay, with the delay in hundredths of a second.
    name=$2
    shift 2
    list=""
    for frame in "$@"; do
        list="$list ( ${frame%:*} -crop 549x549+10+9 +repage -set delay ${frame##*:} )"
    done
    # shellcheck disable=SC2086
    magick $list -loop 0 -colors 96 -dither None -layers Optimize \
        "$root/docs/screenshots/$name.gif"
    ;;
stop)
    pkill -f "$dir/bin/gestos" || true
    pkill -f "$here/fake-daemon.py" || true
    pkill -f "gtk4-broadwayd :$display" || true
    rm -rf "$dir"
    ;;
*)
    echo "Usage: $0 {server|app|build NAME FRAME:DELAY...|stop}" >&2
    exit 1
    ;;
esac
