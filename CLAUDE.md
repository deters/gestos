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
- Each entry says what changed, compared with before, in one plain line: "Added …",
  "… now also works on X11", "Renamed X to Y", and under Fixed, what was wrong: "Closing some
  windows could crash GNOME." No bold headings, no selling: it's a record of changes, not an
  announcement. Nothing on how it's built: no extensions, D-Bus or threads.
- Use what a user would want to do only as a test: would they understand the entry, and see
  why it's useful? If not, add what's missing, like what a new step makes possible. Still write
  the entry as what changed, not as "You can now …".
- An entry about an action or an option names it in quotes, as the app shows it: 'New action
  "Toggle Middle Click Paste": turns middle click paste on or off.', 'The action "Close Window"
  now …', 'New option "Allow Middle Click Paste" in Settings …'. When a change hides something,
  say how to get it back: 'Actions are now grouped in categories. You can still see all your
  gestures together by turning on "Show Gestures in the Main Page" in Settings.'
- When something doesn't work everywhere, the entry ends by saying where it does, in
  parentheses: "(GNOME)", "(GNOME, KDE, X11)". A change to how it works on one desktop starts
  with it: "On GNOME, …".
- Before committing any text for users, reread it for what makes text read as AI-written:
  claims and slogans, vague comparatives like "smoother" or "better" with nothing behind them,
  marketing phrases like "in one click" or "no more", and fragments used for punch.
- To release, run `packaging/bump-version.sh <major|minor|patch>`. It moves what's under
  Unreleased to the new version, dated, and refuses to release with nothing there. Pushing the
  tag publishes that section.
- A beta's page shows what's under Unreleased.
