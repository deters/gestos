#!/bin/sh
# Builds and installs Gestos, then restarts it so that the new version is what's running.
set -e

cd "$(dirname "$0")"

# Built first, so that a failed build leaves the running version alone, and as your user, so that
# the build folder doesn't end up owned by root.
make
sudo make install

# The installed copy, in case an older one is first in PATH.
gestos=$(meson introspect build --installed 2>/dev/null |
    sed -n 's/.*"\([^"]*\/bin\/gestos\)".*/\1/p' | head -n 1)
gestos=${gestos:-gestos}

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
