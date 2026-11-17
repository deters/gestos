#!/bin/sh
# Reads and releases CHANGELOG.md, kept as https://keepachangelog.com has it.
#
# Usage:
#   changelog.sh notes <version|Unreleased> [CHANGELOG.md]
#       Writes what changed in that version, without its heading. Fails if it has nothing.
#   changelog.sh release <version> [CHANGELOG.md]
#       Moves what's under Unreleased to the version, dated today, and links it to the
#       comparison with the version before. Fails if Unreleased has nothing.
set -eu

command=${1:-}
version=${2:-}
file=${3:-CHANGELOG.md}
repository=https://github.com/deters/gestos

if [ -z "$command" ] || [ -z "$version" ]; then
	sed -n '3,9s/^# \{0,1\}//p' "$0" >&2
	exit 2
fi

# The lines between the version's heading and the next one, or the links at the end, with the
# blank lines around them left out.
section() {
	awk -v heading="## [$1]" '
		index($0, heading) == 1 { inside = 1; next }
		inside && (/^## \[/ || /^\[[^]]+\]: /) { exit }
		inside { print }
	' "$file" | sed '/./,$!d' | sed -e :a -e '/^\n*$/{$d;N;ba' -e '}'
}

case "$command" in
notes)
	notes=$(section "$version")
	if [ -z "$notes" ]; then
		echo "$file says nothing under $version." >&2
		exit 1
	fi
	printf '%s\n' "$notes"
	;;
release)
	if [ -z "$(section Unreleased)" ]; then
		echo "Write what changed under Unreleased in $file first." >&2
		exit 1
	fi
	if grep -q "^## \[$version\]" "$file"; then
		echo "$file already has $version." >&2
		exit 1
	fi
	# The version Unreleased is compared with is the one before this.
	previous=$(sed -n 's|^\[Unreleased\]: .*/compare/\(v[^.]*\.[^.]*\.[^.]*\)\.\.\.HEAD$|\1|p' "$file")
	today=$(date +%Y-%m-%d)
	temporary=$(mktemp)
	awk -v version="$version" -v today="$today" -v previous="$previous" -v repository="$repository" '
		/^## \[Unreleased\]/ { print; print ""; print "## [" version "] - " today; next }
		/^\[Unreleased\]: / {
			print "[Unreleased]: " repository "/compare/v" version "...HEAD"
			if (previous != "")
				print "[" version "]: " repository "/compare/" previous "...v" version
			else
				print "[" version "]: " repository "/releases/tag/v" version
			next
		}
		{ print }
	' "$file" > "$temporary"
	mv "$temporary" "$file"
	;;
*)
	echo "Unknown command: $command" >&2
	exit 2
	;;
esac
