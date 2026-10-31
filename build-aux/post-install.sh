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

    # Set Set-Group-ID (SGID) on gestosd to securely access input devices
    GESTOSD_BIN="${MESON_INSTALL_PREFIX:-/usr/local}/bin/gestosd"
    if [ -f "$GESTOSD_BIN" ]; then
        # Create 'input' group if it doesn't exist
        if ! grep -q "^input:" /etc/group 2>/dev/null; then
            echo "Creating 'input' group..."
            if command -v groupadd >/dev/null 2>&1; then
                groupadd -r input 2>/dev/null || true
            elif command -v addgroup >/dev/null 2>&1; then
                addgroup -S input 2>/dev/null || addgroup input 2>/dev/null || true
            fi
        fi

        echo "Setting Set-Group-ID (SGID) to 'input' group on $GESTOSD_BIN..."
        if chown root:input "$GESTOSD_BIN" 2>/dev/null && chmod 2755 "$GESTOSD_BIN" 2>/dev/null; then
            echo "Successfully configured secure SGID permissions for gestosd."
        else
            echo "Warning: Could not set SGID permissions on $GESTOSD_BIN."
            echo "Falling back to standard user group configuration..."
            # Determine user to add to input group
            TARGET_USER="${SUDO_USER:-$USER}"
            if [ -n "$TARGET_USER" ] && [ "$TARGET_USER" != "root" ]; then
                if ! groups "$TARGET_USER" 2>/dev/null | grep -q "\binput\b"; then
                    echo "Adding user '$TARGET_USER' to 'input' group..."
                    usermod -aG input "$TARGET_USER" 2>/dev/null || echo "Please add your user to the 'input' group manually: sudo usermod -aG input $TARGET_USER"
                fi
            fi
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

