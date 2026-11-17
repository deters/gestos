# Gestos

## No shortcuts through other programs

- Use a library Gestos already depends on before running a command-line program for the same
  thing: GSettings through `gio`, not `gsettings`; `/proc`, not `pgrep` or `pkill`; D-Bus through
  `zbus`, not `gdbus` or `busctl`.
- Never leave a helper program running beside the daemon or the input service. Anything needed
  for as long as Gestos runs happens in a thread of its own, with a GLib main loop if what it
  watches needs one.
- Don't copy a shortcut because the code around it takes one. Fix that code too, or say why not.
- When running a program really is the best choice, such as a user's own command or a tool that
  has no library, say so while discussing the change's edge cases, so the user decides.

## Changelog

[CHANGELOG.md](CHANGELOG.md) follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Each release's page on GitHub is its section there.

- Every change a user would notice adds an entry under `## [Unreleased]`, in the same commit as
  the change. A fix to something not released yet changes that entry instead of adding one.
- Entries go under `### Added`, `### Changed`, `### Deprecated`, `### Removed`, `### Fixed` or
  `### Security`, in that order.
- Each entry is a plain bold name and a short sentence on what the user gets, with a concrete
  example where it helps. Nothing on how it's built: no extensions, D-Bus or threads.
- To release, run `packaging/bump-version.sh <major|minor|patch>`. It moves what's under
  Unreleased to the new version, dated, and refuses to release with nothing there. Pushing the
  tag publishes that section.
- A beta's page shows what's under Unreleased.
