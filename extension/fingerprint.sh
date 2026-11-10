#!/bin/sh
# Prints a fingerprint of the extension's code, which only changes when the extension does: its
# version is Gestos's, which changes with every release. Gestos is built with the same fingerprint,
# so it can tell when the copy GNOME runs is a different one.
#
# Given a file, writes the extension's metadata there instead, with the fingerprint and Gestos's
# version in it, for an installed copy. The metadata in the source tree has neither, so the version
# is only ever bumped in Cargo.toml.
set -e

# Where to write, as given, since the rest runs in the extension's folder.
case "$1" in
    "" | /*) output=$1 ;;
    *) output=$PWD/$1 ;;
esac
cd "$(dirname "$0")"

# The shell sorts each pattern's files the same way everywhere with this locale. The metadata is
# left out, as its version changes with every release.
export LC_ALL=C
fingerprint=$(for file in *.js lib/*.js schemas/*.gschema.xml; do
    printf '%s\n' "$file"
    cat "$file"
done | sha256sum | cut -c 1-16)

if [ -z "$output" ]; then
    echo "$fingerprint"
    exit
fi
if [ "$output" -ef metadata.json ]; then
    echo "fingerprint.sh: won't write over the metadata it reads" >&2
    exit 1
fi

version=$(sed -n 's/^version = "\(.*\)"$/\1/p' ../Cargo.toml | head -n 1)
if [ -z "$version" ]; then
    echo "fingerprint.sh: no version in Cargo.toml" >&2
    exit 1
fi

awk -v fingerprint="$fingerprint" -v version="$version" '
    { print }
    /"uuid":/ {
        printf "  \"version-name\": \"%s\",\n", version
        printf "  \"gestos-fingerprint\": \"%s\",\n", fingerprint
    }
' metadata.json > "$output"
