#!/bin/bash
# ChatTerm remote installer for macOS and Linux (Debian/Ubuntu).
#
# Install:
#   curl -fsSL https://raw.githubusercontent.com/chatterm/chatterm/main/scripts/install-remote.sh | bash
#
# Pin a version:
#   VERSION=v0.1.0 curl -fsSL ... | bash
#
# - macOS: downloads the universal DMG and copies ChatTerm.app to /Applications.
#          Using curl instead of a browser avoids the com.apple.quarantine
#          attribute, so the unsigned app launches without Gatekeeper warnings.
# - Linux: downloads the .deb matching the machine architecture and installs
#          via dpkg + apt-get. Debian/Ubuntu only; for other distros, grab the
#          .AppImage from https://github.com/chatterm/chatterm/releases.

set -euo pipefail

REPO="chatterm/chatterm"
VERSION="${VERSION:-latest}"

err() { echo "Error: $*" >&2; exit 1; }

if [[ "$VERSION" == "latest" ]]; then
  api_url="https://api.github.com/repos/${REPO}/releases/latest"
else
  api_url="https://api.github.com/repos/${REPO}/releases/tags/${VERSION}"
fi

# Grep the first release asset URL matching an extended regex.
find_asset() {
  local pattern="$1"
  curl -fsSL "$api_url" \
    | grep '"browser_download_url"' \
    | grep -oE "https://[^\"]+" \
    | grep -E "$pattern" \
    | head -1
}

install_macos() {
  local DEST="/Applications/ChatTerm.app"
  local dmg_url
  dmg_url=$(find_asset '\.dmg$')
  [[ -n "$dmg_url" ]] || err "no .dmg asset found in ${VERSION} release — check https://github.com/${REPO}/releases"

  if pgrep -f "${DEST}/Contents/MacOS/" >/dev/null 2>&1; then
    err "ChatTerm is running — quit it and retry."
  fi

  local tmpdir mount_point dmg_file src_app
  tmpdir=$(mktemp -d)
  mount_point="${tmpdir}/mnt"
  mkdir -p "$mount_point"
  trap "hdiutil detach '${mount_point}' -quiet 2>/dev/null || true; rm -rf '${tmpdir}'" EXIT

  dmg_file="${tmpdir}/ChatTerm.dmg"
  echo "Downloading ${dmg_url##*/}..."
  curl -fL --progress-bar -o "$dmg_file" "$dmg_url"

  echo "Mounting..."
  hdiutil attach -nobrowse -readonly -noverify \
    -mountpoint "$mount_point" "$dmg_file" >/dev/null

  src_app="${mount_point}/ChatTerm.app"
  [[ -d "$src_app" ]] || err "ChatTerm.app not found in DMG."

  echo "Installing to ${DEST}..."
  rm -rf "$DEST"
  cp -R "$src_app" "$DEST"
  xattr -cr "$DEST" 2>/dev/null || true

  echo "✅ Installed: ${DEST}"
  echo "Launch: open \"${DEST}\""
}

install_linux() {
  command -v dpkg >/dev/null 2>&1 \
    || err "dpkg not found. This installer supports Debian/Ubuntu. For other distros, use the .AppImage from https://github.com/${REPO}/releases"

  local arch
  case "$(uname -m)" in
    x86_64)         arch="amd64" ;;
    aarch64|arm64)  arch="arm64" ;;
    *) err "Unsupported Linux architecture: $(uname -m)" ;;
  esac

  # Prefer the arch-specific .deb; fall back to any .deb in the release.
  local deb_url
  deb_url=$(find_asset "_${arch}\\.deb$")
  [[ -n "$deb_url" ]] || deb_url=$(find_asset '\.deb$')
  [[ -n "$deb_url" ]] || err "no .deb asset found in ${VERSION} release — check https://github.com/${REPO}/releases"

  local tmpdir deb_file
  tmpdir=$(mktemp -d)
  trap "rm -rf '${tmpdir}'" EXIT

  deb_file="${tmpdir}/${deb_url##*/}"
  echo "Downloading ${deb_file##*/}..."
  curl -fL --progress-bar -o "$deb_file" "$deb_url"

  echo "Installing (sudo required)..."
  if ! sudo dpkg -i "$deb_file"; then
    echo "Resolving missing dependencies..."
    sudo apt-get install -f -y
  fi

  echo "✅ Installed. Launch: chatterm"
}

echo "Fetching release info (${VERSION})..."
case "$(uname -s)" in
  Darwin) install_macos ;;
  Linux)  install_linux ;;
  *)      err "Unsupported OS: $(uname -s) (supported: macOS, Linux)" ;;
esac
