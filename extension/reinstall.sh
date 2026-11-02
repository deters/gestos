#!/bin/sh
# Tests, installs, and enables the extension for your user.
set -e

cd "$(dirname "$0")"

uuid=gestos@deters.github.io

# Tested first, so that a failure leaves the installed version alone.
make test
make install

# GNOME only finds a new extension when you log in, and on Wayland it can't reload one either, so
# the running Shell keeps the old code until then.
if gnome-extensions enable "$uuid" 2>/dev/null; then
    gnome-extensions info "$uuid" | grep -E 'Enabled|State'
    echo "Installed. If you changed the code, log out and back in to load it."
else
    echo "Installed. Log out and back in, then run this script again to enable it."
fi
