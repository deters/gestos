#!/bin/sh
# Installs or updates Gestos with the package for this system, from its release on GitHub.
#
# Usage:
#   curl -fsSL https://github.com/deters/gestos/releases/latest/download/install.sh | sh
#   curl -fsSL https://github.com/deters/gestos/releases/latest/download/install.sh | sh -s -- --beta
#
# With --beta, it installs the beta when there's one of a version newer than the release.
# On a system it wasn't tested on, it asks in the terminal before installing.
# Each package is checked against the release's SHA256SUMS before it's installed.
# GESTOS_RELEASES can name another place with the same layout, to test the script.
set -eu

releases=${GESTOS_RELEASES:-https://github.com/deters/gestos/releases}

say() {
	echo "gestos: $*" >&2
}

fail() {
	say "$*"
	exit 1
}

# The package manager that installs Gestos here: apt-get, dnf, zypper or apk.
package_manager() {
	for manager in apt-get dnf zypper apk; do
		command -v "$manager" >/dev/null && echo "$manager" && return 0
	done
	fail "There's no package for this system. Gestos has deb, rpm and apk packages, installed with apt-get, dnf, zypper or apk."
}

# The kind of package a package manager installs: deb, rpm or apk.
package_kind() {
	manager=$1
	case "$manager" in
	apt-get) echo deb ;;
	dnf | zypper) echo rpm ;;
	apk) echo apk ;;
	esac
}

# The value of a field in /etc/os-release, like ID or PRETTY_NAME, or nothing.
system_field() {
	field=$1
	[ -r /etc/os-release ] || return 0
	# shellcheck disable=SC1091 # each system's own
	(. /etc/os-release && eval "echo \"\${$field:-}\"")
}

# Whether Gestos was tested on this system: Debian, Ubuntu and the systems made from them,
# Fedora and Alpine.
is_tested() {
	system_id=" $(system_field ID) "
	related=" $(system_field ID_LIKE) "
	case "$system_id$related" in
	*" debian "* | *" ubuntu "*) return 0 ;;
	esac
	case "$system_id" in
	" fedora " | " alpine ") return 0 ;;
	esac
	return 1
}

# Asks in the terminal whether to install on a system Gestos wasn't tested on, and stops unless
# the answer is yes. The script itself comes through the pipe, so the answer can't.
confirm_untested() {
	kind=$1
	system_name=$(system_field PRETTY_NAME)
	say "Gestos hasn't been tested on ${system_name:-this system}. The $kind package may need libraries or package names this system doesn't have."
	{ : </dev/tty; } 2>/dev/null || fail "Run this in a terminal to choose whether to install anyway."
	printf 'gestos: Install anyway? [y/N] ' >&2
	read -r answer </dev/tty || answer=
	case "$answer" in
	y | Y | yes | Yes) ;;
	*) fail "Nothing was installed." ;;
	esac
}

# The name a package for this kind and processor matches in SHA256SUMS.
package_pattern() {
	kind=$1
	processor=$(uname -m)
	case "$kind $processor" in
	"deb x86_64") echo 'gestos_[0-9]*_amd64.deb' ;;
	"rpm x86_64" | "rpm aarch64") echo "gestos-[0-9]*.$processor.rpm" ;;
	"apk x86_64" | "apk aarch64") echo "gestos-[0-9]*-$processor.apk" ;;
	*) fail "There's no $kind package for $processor processors." ;;
	esac
}

# Downloads SHA256SUMS of a release into the folder, and says the package in it that matches.
# Says nothing if the release or its SHA256SUMS doesn't exist.
find_package() {
	release_url=$1
	pattern=$2
	folder=$3
	curl -fsSL -o "$folder/SHA256SUMS" "$release_url/SHA256SUMS" 2>/dev/null || return 0
	while read -r _ name; do
		# shellcheck disable=SC2254 # the pattern is meant to match as a pattern
		case "$name" in
		$pattern) echo "$name" && return 0 ;;
		esac
	done < "$folder/SHA256SUMS"
}

# The checksum SHA256SUMS gives for a package, or nothing.
expected_checksum() {
	package=$1
	sums=$2
	while read -r checksum name; do
		[ "$name" = "$package" ] && echo "$checksum" && return 0
	done < "$sums"
	return 0
}

# The version a package is of, like 9.1.0 for gestos_9.1.0.beta1-1_amd64.deb.
version_of() {
	package=$1
	echo "$package" | sed 's/^gestos[_-]\([0-9]*\.[0-9]*\.[0-9]*\).*/\1/'
}

# Whether version $1 comes after version $2, both written like 9.1.0.
is_newer() {
	old_ifs=$IFS
	IFS=.
	# shellcheck disable=SC2086 # split into its numbers on purpose
	set -- $1 $2
	IFS=$old_ifs
	[ "$1" -ne "$4" ] && { [ "$1" -gt "$4" ]; return; }
	[ "$2" -ne "$5" ] && { [ "$2" -gt "$5" ]; return; }
	[ "$3" -gt "$6" ]
}

# Runs a command as root: directly when already root, otherwise with sudo or doas.
as_root() {
	if [ "$(id -u)" -eq 0 ]; then
		"$@"
	elif command -v sudo >/dev/null; then
		sudo "$@"
	elif command -v doas >/dev/null; then
		doas "$@"
	else
		fail "Installing needs root, and neither sudo nor doas is installed."
	fi
}

main() {
	want_beta=false
	for option in "$@"; do
		case "$option" in
		--beta) want_beta=true ;;
		*) fail "Unknown option: $option. The only option is --beta." ;;
		esac
	done

	command -v curl >/dev/null || fail "Install curl first."
	command -v sha256sum >/dev/null || fail "Install sha256sum first (coreutils)."

	manager=$(package_manager)
	kind=$(package_kind "$manager")
	pattern=$(package_pattern "$kind")
	tested=true
	is_tested || tested=false
	$tested || confirm_untested "$kind"

	folder=$(mktemp -d)
	trap 'rm -rf "$folder"' EXIT
	mkdir "$folder/release" "$folder/beta"

	release_url="$releases/latest/download"
	release_package=$(find_package "$release_url" "$pattern" "$folder/release")
	[ -n "$release_package" ] || fail "The latest release has no $kind package for $(uname -m)."
	release_version=$(version_of "$release_package")

	chosen_url=$release_url
	chosen_package=$release_package
	chosen_folder="$folder/release"
	if $want_beta; then
		beta_url="$releases/download/beta"
		beta_package=$(find_package "$beta_url" "$pattern" "$folder/beta")
		beta_version=$(version_of "${beta_package:-gestos_0.0.0}")
		if [ -n "$beta_package" ] && is_newer "$beta_version" "$release_version"; then
			chosen_url=$beta_url
			chosen_package=$beta_package
			chosen_folder="$folder/beta"
		else
			say "There's no beta newer than $release_version, so this installs $release_version."
		fi
	fi

	say "Downloading $chosen_package"
	curl -fSL --progress-bar -o "$chosen_folder/$chosen_package" "$chosen_url/$chosen_package"
	expected=$(expected_checksum "$chosen_package" "$chosen_folder/SHA256SUMS")
	checksum_line=$(sha256sum "$chosen_folder/$chosen_package")
	actual=${checksum_line%% *}
	[ "$expected" = "$actual" ] || fail "$chosen_package doesn't match its checksum, so it wasn't installed."

	say "Installing $chosen_package"
	package_path="$chosen_folder/$chosen_package"
	# apt reads the package as a user of its own, and mktemp made the folder for this one.
	chmod 755 "$chosen_folder" "$folder" && chmod 644 "$package_path"
	if $tested; then
		yes=-y
		answers=/dev/null
	else
		# So the package manager shows what it would do, and asks.
		yes=
		answers=/dev/tty
	fi
	case "$manager" in
	apt-get) as_root apt-get install $yes "$package_path" <"$answers" ;;
	dnf) as_root dnf install $yes "$package_path" <"$answers" ;;
	zypper) as_root zypper install --allow-unsigned-rpm "$package_path" <"$answers" ;;
	apk) as_root apk add --allow-untrusted "$package_path" <"$answers" ;;
	esac

	if [ "$chosen_url" = "$release_url" ]; then
		say "Gestos $(version_of "$chosen_package") is installed."
	else
		say "The beta of Gestos $(version_of "$chosen_package") is installed."
	fi
	# A copy installed for this user from the source, by reinstall.sh, comes first: on PATH, and
	# for GNOME, which loads a user's extension in place of the system's. It would go on running
	# in place of the one just installed.
	user_extension="$HOME/.local/share/gnome-shell/extensions/gestos@deters.github.io"
	if [ -e "$HOME/.local/bin/gestos" ] || [ -e "$HOME/.local/bin/gestosd" ] || [ -e "$user_extension" ]; then
		say "Another copy of Gestos is installed for your user in $HOME/.local, and runs in place of this one."
		say "Remove it with reinstall.sh --remove in the Gestos source folder, or delete $HOME/.local/bin/gestos, $HOME/.local/bin/gestosd and $user_extension."
	fi
	say "On GNOME, log out and back in after installing, then turn on the Gestos extension in Preferences."
}

# Everything runs from here, once the whole script is read, so nothing it runs can read the
# rest of the script from the pipe.
main "$@"
