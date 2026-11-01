#!/bin/sh

# Post-installation script for gestosd

if [ -z "$DESTDIR" ]; then
    # Remove what a source install of MyGestures left behind. Its binary is now a wrapper,
    # which must not keep the old binary's Set-Group-ID permission.
    PREFIX="${MESON_INSTALL_PREFIX:-/usr/local}"
    chmod 755 "$PREFIX/bin/mygestures" 2>/dev/null || true
    rm -f /etc/udev/rules.d/99-mygestures.rules \
          /lib/modules-load.d/mygestures.conf \
          "$PREFIX/share/dbus-1/services/org.mygestures.Daemon.service" \
          "$PREFIX/lib/systemd/user/mygestures.service" \
          "$PREFIX/share/mygestures/mygestures.yaml"
    rmdir "$PREFIX/share/mygestures" 2>/dev/null || true

    echo "Reloading udev rules..."
    udevadm control --reload-rules 2>/dev/null || true
    udevadm trigger 2>/dev/null || true

    # Earlier versions installed gestosd as Set-Group-ID. The input service does this job now.
    GESTOSD_BIN="$PREFIX/bin/gestosd"
    chown root:root "$GESTOSD_BIN" 2>/dev/null || true
    chmod 755 "$GESTOSD_BIN" 2>/dev/null || true

    # The input service reaches input devices through this group
    if ! grep -q "^input:" /etc/group 2>/dev/null; then
        echo "Creating 'input' group..."
        if command -v groupadd >/dev/null 2>&1; then
            groupadd -r input 2>/dev/null || true
        elif command -v addgroup >/dev/null 2>&1; then
            addgroup -S input 2>/dev/null || addgroup input 2>/dev/null || true
        fi
    fi

    if command -v systemctl >/dev/null 2>&1; then
        echo "Starting the Gestos input service..."
        systemctl daemon-reload 2>/dev/null || true
        if ! systemctl enable --now gestos-input.socket 2>/dev/null; then
            echo "Warning: Could not start it. Run: sudo systemctl enable --now gestos-input.socket"
        fi
    fi

    # Update desktop database and GTK icon cache
    echo "Updating desktop database..."
    if command -v update-desktop-database >/dev/null 2>&1; then
        update-desktop-database -q "${MESON_INSTALL_PREFIX:-/usr/local}/share/applications" 2>/dev/null || true
    fi

    echo "Updating icon cache..."
    if command -v gtk-update-icon-cache >/dev/null 2>&1; then
        gtk-update-icon-cache -q -t -f "${MESON_INSTALL_PREFIX:-/usr/local}/share/icons/hicolor" 2>/dev/null || true
    fi
fi

