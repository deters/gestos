project('gestos',
  version: '6.1.1',
  meson_version: '>= 0.56.0'
)

# Install directories
prefix = get_option('prefix')
bindir = prefix / get_option('bindir')
libexecdir = prefix / get_option('libexecdir')

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
  'src/input_client.rs',
  'src/input_protocol.rs',
  'src/ipc.rs',
  'src/keyboards.rs',
  'src/keymap.rs',
  'src/keys.rs',
  'src/modifiers.rs',
  'src/protractor.rs',
  'src/uinput.rs',
  'src/wayland.rs',
  'src/bin/gestosd.rs',
  'src/bin/gestos-input.rs',
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
  'src/bin/gestos/extension.rs',
  'src/bin/gestos/state.rs',
  'src/bin/gestos/suggestions.rs',
  'src/bin/gestos/main.rs',
)

# Custom target for Cargo build
cargo_build = custom_target(
  'cargo-build',
  build_by_default: true,
  input: cargo_sources,
  output: ['gestosd', 'gestos', 'gestos-input'],
  console: true,
  install: true,
  install_dir: [bindir, bindir, libexecdir],
  command: [
    cargo_script,
    cargo_target_dir,
    cargo_manifest,
    build_type,
    meson.project_build_root(),
    'gestosd',
    'gestos',
    'gestos-input'
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
conf_data.set('libexecdir', libexecdir)

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

# The input service: the only part with access to input devices
systemd_system_dir = get_option('systemdsystemunitdir')
if systemd_system_dir == ''
  systemd_system_dir = prefix / 'lib' / 'systemd' / 'system'
endif
install_data('build-aux/gestos-input.socket', install_dir: systemd_system_dir)
configure_file(
  input: 'build-aux/gestos-input@.service.in',
  output: 'gestos-input@.service',
  configuration: conf_data,
  install_dir: systemd_system_dir
)

# The GNOME Shell extension, for window actions that only the Shell can do
if get_option('gnome_extension')
  extension_dir = datadir / 'gnome-shell' / 'extensions' / 'gestos@deters.github.io'
  install_data('extension/extension.js', 'extension/prefs.js', 'extension/metadata.json', install_dir: extension_dir)
  install_data('extension/lib/tile.js', install_dir: extension_dir / 'lib')
  install_data('extension/schemas/org.gnome.shell.extensions.gestos.gschema.xml', install_dir: extension_dir / 'schemas')
  # Compiled here, as extensions keep their own settings schema instead of sharing the system's.
  custom_target(
    'extension-schemas',
    input: 'extension/schemas/org.gnome.shell.extensions.gestos.gschema.xml',
    output: 'gschemas.compiled',
    command: [find_program('glib-compile-schemas'), '--targetdir=@OUTDIR@', meson.project_source_root() / 'extension/schemas'],
    build_by_default: true,
    install: true,
    install_dir: extension_dir / 'schemas',
  )

  gjs = find_program('gjs', required: false)
  if gjs.found()
    test('gnome-extension', gjs, args: ['-m', files('extension/tests/tile.test.js')])
  endif
endif

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

