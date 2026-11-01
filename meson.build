project('gestos',
  version: '5.0.0',
  meson_version: '>= 0.56.0'
)

# Install directories
prefix = get_option('prefix')
bindir = prefix / get_option('bindir')

# Dependencies & Programs
cargo = find_program('cargo', required: true)
cargo_script = find_program('build-aux/cargo.sh')

cargo_target_dir = meson.project_build_root() / 'cargo'
cargo_manifest = meson.project_source_root() / 'Cargo.toml'

# Only an explicit debug build skips optimization. Distributions build with 'plain'.
if get_option('buildtype') == 'debug'
  build_type = 'debug'
else
  build_type = 'release'
endif

# Source files to track for rebuilds
cargo_sources = files(
  'Cargo.toml',
  'Cargo.lock',
  'src/lib.rs',
  'src/chooser.rs',
  'src/config.rs',
  'src/gesture.rs',
  'src/ipc.rs',
  'src/protractor.rs',
  'src/uinput.rs',
  'src/wayland.rs',
  'src/bin/gestosd.rs',
  'src/bin/gestos/actions.rs',
  'src/bin/gestos/ui/shortcut_recorder.rs',
  'src/bin/gestos/ui/overlay.rs',
  'src/bin/gestos/ui/main_window.rs',
  'src/bin/gestos/ui/mod.rs',
  'src/bin/gestos/ui/components.rs',
  'src/bin/gestos/ui/editor.rs',
  'src/bin/gestos/ui/gesture_chooser.rs',
  'src/bin/gestos/ui/settings.rs',
  'src/bin/gestos/ui/gesture_status.rs',
  'src/bin/gestos/system.rs',
  'src/bin/gestos/daemon.rs',
  'src/bin/gestos/state.rs',
  'src/bin/gestos/suggestions.rs',
  'src/bin/gestos/main.rs',
)

# Custom target for Cargo build
cargo_build = custom_target(
  'cargo-build',
  build_by_default: true,
  input: cargo_sources,
  output: ['gestosd', 'gestos'],
  console: true,
  install: true,
  install_dir: bindir,
  command: [
    cargo_script,
    cargo_target_dir,
    cargo_manifest,
    build_type,
    meson.project_build_root(),
    'gestosd',
    'gestos'
  ]
)

# Install the udev rules
udevdir = get_option('udevdir')
if udevdir == ''
  udevdir = '/etc/udev/rules.d'
endif
install_data('packaging/99-gestos.rules', install_dir: udevdir)

# Install systemd modules-load configuration
modulesloaddir = get_option('modulesloaddir')
if modulesloaddir == ''
  modulesloaddir = '/lib/modules-load.d'
endif
install_data('packaging/gestos.conf', install_dir: modulesloaddir)

# Install desktop entry, icon and default configuration
datadir = prefix / get_option('datadir')
install_data('gestos.desktop', install_dir: datadir / 'applications')
install_data('gestos.svg', install_dir: datadir / 'icons/hicolor/scalable/apps')
install_data('gestos.yaml', install_dir: datadir / 'gestos')

# D-Bus Activation Service
dbus_service_dir = get_option('dbusservicedir')
if dbus_service_dir == ''
  dbus_service_dir = datadir / 'dbus-1' / 'services'
endif

conf_data = configuration_data()
conf_data.set('bindir', bindir)

configure_file(
  input: 'build-aux/io.github.deters.Gestos.Daemon.service.in',
  output: 'io.github.deters.Gestos.Daemon.service',
  configuration: conf_data,
  install_dir: dbus_service_dir
)

# systemd User Service
systemd_user_dir = get_option('systemduserunitdir')
if systemd_user_dir == ''
  systemd_user_dir = prefix / 'lib' / 'systemd' / 'user'
endif
configure_file(
  input: 'build-aux/gestosd.service.in',
  output: 'gestosd.service',
  configuration: conf_data,
  install_dir: systemd_user_dir
)

# Keeps old autostart entries and scripts working after the rename from MyGestures
# It's generated under another name: in a build folder that once built the old binary,
# a file called mygestures would be deleted as a stale build output.
mygestures_wrapper = configure_file(
  input: 'build-aux/mygestures-compat.sh.in',
  output: 'mygestures-compat.sh',
  configuration: conf_data
)
install_data(
  mygestures_wrapper,
  rename: 'mygestures',
  install_dir: bindir,
  install_mode: 'rwxr-xr-x'
)

# Run post-install script
meson.add_install_script('build-aux/post-install.sh')

