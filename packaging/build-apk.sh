#!/bin/sh
# Builds the Alpine package from this checkout, as root in an Alpine container, run from the
# top of the source. The packages land in apk/, named with their architecture, since the
# packages for each architecture would otherwise share a name in the release.
set -eu

apk update
apk add alpine-sdk meson ninja cargo rust gtk4.0-dev libadwaita-dev libevdev-dev pkgconfig git sudo

# Create builder user (abuild cannot run as root)
adduser -D builder -G abuild
echo "builder ALL=(ALL) NOPASSWD: ALL" >> /etc/sudoers
chown -R builder:abuild .

# Build from this checkout. A source archive already in SRCDEST isn't downloaded,
# and the download would only exist once the release tag does.
VERSION=$(grep '^pkgver=' packaging/APKBUILD | cut -d= -f2)
mkdir -p /tmp/distfiles
tar --exclude-vcs --exclude='.cargo' --exclude='target-alpine' --exclude='apk' --transform "s,^\.,gestos-$VERSION," -czf /tmp/distfiles/gestos-$VERSION.tar.gz .
chown -R builder:abuild /tmp/distfiles

# Run abuild as builder user
su builder -c "abuild-keygen -i -a -n"
su builder -c "export SRCDEST=/tmp/distfiles && export CARGO_HOME=$PWD/.cargo && export OVERRIDE_CARGO_TARGET_DIR=$PWD/target-alpine && cd packaging && abuild checksum && abuild -r"

ARCH=$(apk --print-arch)
mkdir -p apk
for package in /home/builder/packages/*/"$ARCH"/*.apk; do
	cp "$package" "apk/$(basename "$package" .apk)-$ARCH.apk"
done
