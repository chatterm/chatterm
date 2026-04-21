#!/usr/bin/env bash
# Install a locally built ChatTerm Linux package.
#
# Build first:
#   npm run tauri -- build --bundles deb,appimage
set -euo pipefail

err() {
  echo "Error: $*" >&2
  exit 1
}

[[ "$(uname -s)" == "Linux" ]] || err "This installer is for Linux."

deb=$(find src-tauri/target/release/bundle/deb -maxdepth 1 -name '*.deb' 2>/dev/null | head -1 || true)
appimage=$(find src-tauri/target/release/bundle/appimage -maxdepth 1 -name '*.AppImage' 2>/dev/null | head -1 || true)

if [[ -n "$deb" ]]; then
  echo "Installing DEB package: $deb"
  sudo dpkg -i "$deb" || {
    echo "Resolving missing package dependencies..."
    sudo apt-get install -f -y
  }
  echo "Installed ChatTerm from DEB."
  exit 0
fi

if [[ -n "$appimage" ]]; then
  dest_dir="$HOME/.local/bin"
  dest="$dest_dir/chatterm"
  mkdir -p "$dest_dir"
  cp "$appimage" "$dest"
  chmod +x "$dest"
  echo "Installed AppImage launcher: $dest"
  echo "Make sure $dest_dir is on PATH, then run: chatterm"
  exit 0
fi

err "No Linux bundle found. Run: npm run tauri -- build --bundles deb,appimage"
