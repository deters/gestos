#!/bin/sh
# Numbers this build as beta N of the version given, in each package format's own way, so it
# installs over the current version, and that version's release installs over it.
# Usage: mark-beta.sh VERSION N, from the top of the source.
set -eu

version=$1
n=$2

# Debian and Fedora: 6.10.0~beta1 comes after 6.9.0 and before 6.10.0.
sed -i "1s/^gestos ([^)]*)/gestos ($version~beta$n-1)/" debian/changelog
sed -i "s/^Version:\( *\).*/Version:\1$version~beta$n/" packaging/gestos.spec
# Alpine: 6.10.0_beta1, the same way.
sed -i "s/^pkgver=.*/pkgver=${version}_beta$n/" packaging/APKBUILD
# What About and the debug report show.
echo "$version beta $n" > packaging/beta-version

grep -q "^gestos ($version~beta$n-1)" debian/changelog
grep -q "^Version: *$version~beta$n\$" packaging/gestos.spec
grep -q "^pkgver=${version}_beta$n\$" packaging/APKBUILD
