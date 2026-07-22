project('quickdict-extension',
  version: '0.1.0',
  meson_version: '>= 1.3',
)

extension_id = 'quickdict-focus@tigertall.github.com'

custom_target('extension-zip',
  build_by_default: true,
  output: extension_id + '.zip',
  command: [
    'sh', '-c',
    'rm -f "$1" && cd "$0" && zip -r "$1" . -x "*~" "*.un~"',
    meson.project_source_root() / extension_id,
    meson.current_build_dir() / extension_id + '.zip'
  ],
)
