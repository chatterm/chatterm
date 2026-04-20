#!/bin/bash
# Install ChatTerm to /Applications
set -e
APP="src-tauri/target/release/bundle/macos/ChatTerm.app"
DEST="/Applications/ChatTerm.app"

if [ ! -d "$APP" ]; then
  echo "Build first: npm run tauri build"
  exit 1
fi

echo "Installing ChatTerm..."
rm -rf "$DEST"
cp -R "$APP" "$DEST"
# Remove quarantine flag so macOS doesn't block it
xattr -cr "$DEST" 2>/dev/null || true
echo "✅ Installed to $DEST"
echo "Run: open /Applications/ChatTerm.app"
