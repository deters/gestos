#!/bin/sh
# Numbers this build as beta N after the current version, in each package format's own way, so it
# installs over the current version, and the next release installs over it.
# Usage: mark-beta.sh N, from the top of the source.
set -eu

n=$1
version=$(grep '^pkgver=' packaging/APKBUILD | cut -d= -f2)

# Debian: 6.9.0+beta5 comes after 6.9.0 and before 6.9.1.
sed -i "1s/^gestos ($version-/gestos ($version+beta$n-/" debian/changelog
# Fedora: release 1.beta5 comes after release 1 of the same version.
sed -i "s/^Release:\( *\)1%{?dist}/Release:\11.beta$n%{?dist}/" packaging/gestos.spec
# Alpine only takes a few suffixes: 6.9.0_p5 comes after 6.9.0 and before 6.9.1.
sed -i "s/^pkgver=$version\$/pkgver=${version}_p$n/" packaging/APKBUILD

grep -q "^gestos ($version+beta$n-" debian/changelog
grep -q "^Release: *1.beta$n%{?dist}" packaging/gestos.spec
grep -q "^pkgver=${version}_p$n\$" packaging/APKBUILD
