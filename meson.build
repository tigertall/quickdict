project('quickdict',
  version: '0.1.0',
  meson_version: '>= 1.3',
  license: 'MIT',
  default_options: ['warning_level=2']
)

application_id = 'io.github.tigertall.QuickDict'

# Dependencies
glib_dep = dependency('glib-2.0', version: '>= 2.80')
gtk_dep = dependency('gtk4', version: '>= 4.14')
adw_dep = dependency('libadwaita-1', version: '>= 1.5')

# i18n
i18n = import('i18n')
gnome = import('gnome')
fs = import('fs')

# Desktop file
desktop_file = i18n.merge_file(
  type: 'desktop',
  input: 'data' / application_id + '.desktop.in',
  output: application_id + '.desktop',
  po_dir: meson.project_source_root() / 'po',
  install: true,
  install_dir: get_option('datadir') / 'applications',
)

# Metainfo
metainfo_file = i18n.merge_file(
  type: 'xml',
  input: 'data' / application_id + '.metainfo.xml.in',
  output: application_id + '.metainfo.xml',
  po_dir: meson.project_source_root() / 'po',
  install: true,
  install_dir: get_option('datadir') / 'metainfo',
)

# Validate metainfo
appstream_util = find_program('appstreamcli', required: false)
if appstream_util.found()
  test('validate-metainfo', appstream_util,
    args: ['validate', '--no-net', metainfo_file],
    depends: metainfo_file,
  )
endif

# Icons (hicolor, all sizes)
install_subdir(
  'data/icons/hicolor',
  install_dir: get_option('datadir') / 'icons',
)

# Screenshots (installed alongside metainfo for Flathub)
install_subdir(
  'screenshots',
  install_dir: get_option('datadir') / 'metainfo',
)

subdir('po')

# Rust binary
cargo = find_program('cargo', required: true)
cargo_env = environment()
# cargo_env.set('CARGO_HOME', meson.project_build_root() / 'cargo-home')

cargo_release = get_option('buildtype').startswith('release')
cargo_target_dir = meson.project_build_root() / 'target'

if cargo_release
  cargo_flags = ['--release']
  rust_target = 'release'
else
  cargo_flags = []
  rust_target = 'debug'
endif

cargo_args = ['build'] + cargo_flags + ['--target-dir', cargo_target_dir]

custom_target('cargo-build',
  build_by_default: true,
  build_always_stale: true,
  output: ['quickdict'],
  console: true,
  command: [cargo.full_path(), cargo_args],
  env: cargo_env,
)

# Install binary — cargo outputs to builddir/target/, copy to bindir
meson.add_install_script('sh', '-c',
  'install -D ' + cargo_target_dir / rust_target / 'quickdict' +
  ' "' + get_option('prefix') + '/' + get_option('bindir') + '/quickdict"'
)

# GSettings schema
install_data(
  'schemas/io.github.tigertall.QuickDict.gschema.xml',
  install_dir: get_option('datadir') / 'glib-2.0' / 'schemas',
)

# GNOME Shell extension — see ext/meson.build for standalone zip packaging

gnome.post_install(
  glib_compile_schemas: true,
  gtk_update_icon_cache: true,
  update_desktop_database: true,
)
