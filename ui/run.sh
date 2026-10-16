#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
if [[ ! -f src/device/abi.rs || ! -f src/device/defaults.json ]]; then
  python3 ../tools/bootstrap.py
fi
cargo build --release
if [[ "${1:-}" == "--bundle" ]]; then
  if [[ "$(uname -s)" != "Darwin" ]]; then
    echo 'The app bundle is for macOS. Use ./run.sh on Linux.' >&2
    exit 1
  fi
  bundle_dir="$PWD/target/GR-1 UI.app"
  mkdir -p "$bundle_dir/Contents/MacOS"
  cp target/release/gr1-ui "$bundle_dir/Contents/MacOS/gr1-ui"
  cat > "$bundle_dir/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>gr1-ui</string>
<key>CFBundleIdentifier</key><string>org.gr1.ui</string>
<key>CFBundleName</key><string>GR-1 UI</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
  echo "$bundle_dir"
else
  exec target/release/gr1-ui "$@"
fi
