Name:           gestos
Version:        6.1.3
Release:        1%{?dist}
Summary:        Pure Wayland/Evdev mouse gestures for Linux
License:        GPL-2.0-or-later
URL:            https://github.com/deters/gestos
Source0:        %{url}/archive/v%{version}/%{name}-%{version}.tar.gz

# Renamed from MyGestures in 5.0.0
Obsoletes:      mygestures < 5.0.0
Provides:       mygestures = %{version}-%{release}

BuildRequires:  meson
BuildRequires:  ninja-build
BuildRequires:  cargo
BuildRequires:  gtk4-devel
BuildRequires:  libevdev-devel
BuildRequires:  gcc
BuildRequires:  systemd-rpm-macros

Requires:       gtk4
Requires:       libevdev
Requires:       systemd

%description
Mouse gestures - "draw" commands using your mouse/touchscreen/touchpad.
Now written in Rust and completely independent of X11 and legacy drivers.

%prep
%autosetup

%build
%meson -Dudevdir=%{_udevrulesdir}
%meson_build

%install
%meson_install

%pre
# Ensure the 'input' group exists before installing the files
getent group input >/dev/null || groupadd -r input

%post
%udev_rules_update
%systemd_post gestos-input.socket

%preun
%systemd_preun gestos-input.socket

%postun
%udev_rules_update
%systemd_postun_with_restart gestos-input.socket

%files
%license COPYING
%doc README.md
%{_bindir}/gestosd
%{_libexecdir}/gestos-input
%{_bindir}/mygestures
%{_bindir}/gestos
%{_udevrulesdir}/99-gestos.rules
/lib/modules-load.d/gestos.conf
%{_datadir}/applications/gestos.desktop
%{_datadir}/icons/hicolor/scalable/apps/gestos.svg
%{_datadir}/gestos/gestos.yaml
%{_datadir}/gnome-shell/extensions/gestos@deters.github.io/
/usr/lib/systemd/user/gestosd.service
%{_unitdir}/gestos-input.socket
%{_unitdir}/gestos-input@.service
/usr/share/dbus-1/services/io.github.deters.Gestos.Daemon.service

%changelog
* Tue Nov 03 2026 Lucas Augusto Deters <lucasdeters@gmail.com> - 6.1.3-1
- New release 6.1.3.
* Tue Nov 03 2026 Lucas Augusto Deters <lucasdeters@gmail.com> - 6.1.2-1
- New release 6.1.2.
* Mon Nov 02 2026 Lucas Augusto Deters <lucasdeters@gmail.com> - 6.1.1-1
- New release 6.1.1.
* Mon Nov 02 2026 Lucas Augusto Deters <lucasdeters@gmail.com> - 6.1.0-1
- New release 6.1.0.
* Sun Nov 01 2026 Lucas Augusto Deters <lucasdeters@gmail.com> - 6.0.0-1
- New release 6.0.0.
* Sat Oct 31 2026 Lucas Augusto Deters <lucasdeters@gmail.com> - 5.0.0-1
- New release 5.0.0.
* Sat Oct 31 2026 Lucas Augusto Deters <lucasdeters@gmail.com> - 4.2.2-1
- New release 4.2.2.
* Thu Jun 11 2026 Lucas Augusto Deters <lucasdeters@gmail.com> - 4.2.1-1
- New release 4.2.1.
* Thu Jun 11 2026 Lucas Augusto Deters <lucasdeters@gmail.com> - 4.2.0-1
- New release 4.2.0.
* Thu Jun 11 2026 Lucas Augusto Deters <lucasdeters@gmail.com> - 4.1.11-1
- New release 4.1.11.
* Thu Jun 11 2026 Lucas Augusto Deters <lucasdeters@gmail.com> - 4.1.10-1
- New release 4.1.10.
* Wed Jun 10 2026 Lucas Augusto Deters <lucasdeters@gmail.com> - 4.1.9-1
- New release 4.1.9.
* Wed Jun 10 2026 Lucas Augusto Deters <lucasdeters@gmail.com> - 4.1.8-1
- New release 4.1.8.
* Wed Jun 10 2026 Lucas Augusto Deters <lucasdeters@gmail.com> - 4.1.7-1
- New release 4.1.7.
* Wed Jun 10 2026 Lucas Augusto Deters <lucasdeters@gmail.com> - 4.1.6-1
- New release 4.1.6.
* Wed Jun 10 2026 Lucas Augusto Deters <lucasdeters@gmail.com> - 4.1.5-1
- New release 4.1.5.
* Wed Jun 10 2026 Lucas Augusto Deters <lucasdeters@gmail.com> - 4.1.4-1
- New release 4.1.4.
* Wed Jun 10 2026 Lucas Augusto Deters <lucasdeters@gmail.com> - 4.1.3-1
- New release 4.1.3.
* Wed Jun 10 2026 Lucas Augusto Deters <lucasdeters@gmail.com> - 4.1.2-1
- New release 4.1.2.
* Wed Jun 10 2026 Lucas Augusto Deters <lucasdeters@gmail.com> - 4.1.1-1
- New release 4.1.1.
* Wed Jun 10 2026 Lucas Augusto Deters <lucasdeters@gmail.com> - 4.1.0-1
- New release 4.1.0.
* Wed Jun 10 2026 Lucas Augusto Deters <lucasdeters@gmail.com> - 4.0.0-1
- Initial Fedora packaging release with secure SGID permissions configuration.
