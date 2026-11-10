project('gestos',
  version: '7.1.0',
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

# Cargo knows which sources changed, so it's always asked, rather than meson keeping a list of
# them that falls behind as files are added.
cargo_build = custom_target(
  'cargo-build',
  build_by_default: true,
  build_always_stale: true,
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
# Drawn at the usual sizes too, for where the SVG isn't shown, as on Ubuntu.
foreach size : ['16', '24', '32', '48', '64', '128', '256', '512']
  install_data('icons/gestos-' + size + '.png', rename: 'gestos.png',
    install_dir: datadir / 'icons/hicolor' / (size + 'x' + size) / 'apps')
endforeach
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
  install_data('extension/extension.js', 'extension/prefs.js', install_dir: extension_dir)
  # The installed metadata carries a fingerprint of the extension's code, which Gestos compares
  # with its own to tell whether the copy GNOME runs is the one it came with.
  custom_target(
    'extension-metadata',
    input: 'extension/metadata.json',
    output: 'metadata.json',
    command: [find_program('sh'), meson.project_source_root() / 'extension/fingerprint.sh', '@OUTPUT@'],
    depend_files: files(
      'extension/extension.js',
      'extension/prefs.js',
      'extension/lib/tile.js',
      'extension/lib/chooser.js',
      'extension/lib/trail.js',
      'extension/lib/sizes.js',
      'extension/schemas/org.gnome.shell.extensions.gestos.gschema.xml',
    ),
    build_by_default: true,
    install: true,
    install_dir: extension_dir,
  )
  install_data('extension/lib/tile.js', 'extension/lib/chooser.js', 'extension/lib/trail.js',
    'extension/lib/sizes.js', install_dir: extension_dir / 'lib')
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
    test('gnome-extension-sizes', gjs, args: ['-m', files('extension/tests/sizes.test.js')])
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

