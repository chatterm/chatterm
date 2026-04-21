#!/bin/bash
# ChatTerm remote installer for macOS
#
# Install:
#   curl -fsSL https://raw.githubusercontent.com/chatterm/chatterm/main/scripts/install-remote.sh | bash
#
# Pin a version:
#   VERSION=v0.1.0 curl -fsSL ... | bash
#
# Downloading with curl avoids the com.apple.quarantine attribute that browsers add,
# so the unsigned app launches without Gatekeeper warnings.

set -euo pipefail

REPO="chatterm/chatterm"
APP_NAME="ChatTerm.app"
DEST="/Applications/${APP_NAME}"
VERSION="${VERSION:-latest}"

err() { echo "Error: $*" >&2; exit 1; }

[[ "$(uname -s)" == "Darwin" ]] || err "ChatTerm is macOS only."

if [[ "$VERSION" == "latest" ]]; then
  api_url="https://api.github.com/repos/${REPO}/releases/latest"
else
  api_url="https://api.github.com/repos/${REPO}/releases/tags/${VERSION}"
fi

echo "Fetching release info (${VERSION})..."
dmg_url=$(curl -fsSL "$api_url" \
  | grep '"browser_download_url"' \
  | grep -oE 'https://[^"]+\.dmg' \
  | head -1)

[[ -n "$dmg_url" ]] || err "no .dmg asset found in ${VERSION} release — check https://github.com/${REPO}/releases"

if pgrep -f "${DEST}/Contents/MacOS/" >/dev/null 2>&1; then
  err "ChatTerm is running — quit it and retry."
fi

tmpdir=$(mktemp -d)
mount_point="${tmpdir}/mnt"
mkdir -p "$mount_point"

cleanup() {
  hdiutil detach "$mount_point" -quiet 2>/dev/null || true
  rm -rf "$tmpdir"
}
trap cleanup EXIT

dmg_file="${tmpdir}/ChatTerm.dmg"
echo "Downloading ${dmg_url##*/}..."
curl -fL --progress-bar -o "$dmg_file" "$dmg_url"

echo "Mounting..."
hdiutil attach -nobrowse -readonly -noverify \
  -mountpoint "$mount_point" "$dmg_file" >/dev/null

src_app="${mount_point}/${APP_NAME}"
[[ -d "$src_app" ]] || err "${APP_NAME} not found in DMG."

echo "Installing to ${DEST}..."
rm -rf "$DEST"
cp -R "$src_app" "$DEST"
xattr -cr "$DEST" 2>/dev/null || true

echo "✅ Installed: ${DEST}"
echo "Launch: open \"${DEST}\""
