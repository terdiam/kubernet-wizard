#!/usr/bin/env sh
# Installs the kubernet-wizard launcher entry and icons for the current user.
# Usage: ./install.sh [path/to/kubernet-wizard-binary]
set -eu
here="$(cd "$(dirname "$0")" && pwd)"
data="${XDG_DATA_HOME:-$HOME/.local/share}"
bin_dir="$HOME/.local/bin"
if [ "${1:-}" ]; then
  mkdir -p "$bin_dir"
  install -m 755 "$1" "$bin_dir/kubernet-wizard"
fi
for png in "$here"/hicolor/*.png; do
  size="$(basename "$png" .png)"
  mkdir -p "$data/icons/hicolor/$size/apps"
  install -m 644 "$png" "$data/icons/hicolor/$size/apps/kubernet-wizard.png"
done
mkdir -p "$data/applications"
install -m 644 "$here/kubernet-wizard.desktop" "$data/applications/kubernet-wizard.desktop"
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$data/applications" || true
command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -q "$data/icons/hicolor" || true
echo "kubernet-wizard launcher installed (binary expected on PATH as 'kubernet-wizard')."
