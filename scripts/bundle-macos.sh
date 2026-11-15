#!/usr/bin/env bash
# Usage: bundle-macos.sh <binary> <version> <out.tar.gz> [out.dmg]
# The .tar.gz feeds auto-update; the optional .dmg is the drag-to-install image.
set -euo pipefail
bin="$1"; version="$2"; out="$3"; dmg="${4:-}"
work="$(mktemp -d)"
app="$work/kubernet-wizard.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$bin" "$app/Contents/MacOS/kubernet-wizard"
cp "$(dirname "$0")/../assets/icon/kubernet-wizard.icns" "$app/Contents/Resources/kubernet-wizard.icns"
chmod 755 "$app/Contents/MacOS/kubernet-wizard"
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>kubernet-wizard</string>
  <key>CFBundleDisplayName</key><string>kubernet-wizard</string>
  <key>CFBundleIdentifier</key><string>dev.kubernet-wizard.app</string>
  <key>CFBundleExecutable</key><string>kubernet-wizard</string>
  <key>CFBundleIconFile</key><string>kubernet-wizard</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>${version}</string>
  <key>CFBundleVersion</key><string>${version}</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST
tar -C "$work" -czf "$out" kubernet-wizard.app
if [ -n "$dmg" ]; then
  stage="$work/dmg"
  mkdir -p "$stage"
  cp -R "$app" "$stage/"
  ln -s /Applications "$stage/Applications"
  rm -f "$dmg"
  hdiutil create -volname "kubernet-wizard $version" -srcfolder "$stage" -fs HFS+ -format UDZO -ov "$dmg" >/dev/null
fi
rm -rf "$work"
