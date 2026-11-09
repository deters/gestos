#!/bin/sh
# Writes a release page: the release notes, then which package to install on each system.
# Usage: release-body.sh RELEASE_NOTES.md <folder with the packages>
set -eu

notes=$1
packages=$2

# The package in the folder whose name matches, or nothing if there's none.
find_package() {
	find "$packages" -name "$1" -exec basename {} \; | head -n 1
}

row() {
	system=$1
	file=$2
	command=$3
	[ -n "$file" ] && printf '| %s | `%s` | `%s ./%s` |\n' "$system" "$file" "$command" "$file"
	return 0
}

# The release has its own title, so the notes' title is left out.
sed '1{/^# /d}' "$notes" | sed '/./,$!d'
echo
echo "## Install"
echo
echo "| System | Package | Install with |"
echo "|---|---|---|"
row "Ubuntu, Debian" "$(find_package 'gestos_*_amd64.deb')" "sudo apt install"
row "Fedora" "$(find_package 'gestos-*.x86_64.rpm')" "sudo dnf install"
row "Fedora on ARM" "$(find_package 'gestos-*.aarch64.rpm')" "sudo dnf install"
row "Alpine" "$(find_package 'gestos-*-x86_64.apk')" "doas apk add --allow-untrusted"
row "Alpine on ARM" "$(find_package 'gestos-*-aarch64.apk')" "doas apk add --allow-untrusted"
echo
echo "On GNOME, log out and back in after installing, then turn on the Gestos extension in Preferences."
