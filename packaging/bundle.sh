#!/bin/sh
# Builds a release binary and wraps it as PocketAnon.app (unsigned) in target/bundle/.
# A bare Mach-O binary cannot carry an icon; macOS reads it from the bundle's Info.plist.
set -eu
cd "$(dirname "$0")/.."
cargo build -p pocketanon --release
app=target/bundle/PocketAnon.app
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp target/release/pocketanon "$app/Contents/MacOS/PocketAnon"
cp assets/icon.icns "$app/Contents/Resources/PocketAnon.icns"
version=$(grep -m1 '^version' Cargo.toml | sed 's/.*"\(.*\)".*/\1/')
# Author from [workspace.package] authors (first entry, email stripped) for the copyright line.
author=$(cargo metadata --no-deps --format-version 1 \
  | python3 -c 'import json,sys,re; m=json.load(sys.stdin); p=[x for x in m["packages"] if x["name"]=="pocketanon"][0]; a=(p.get("authors") or ["Unknown"])[0]; print(re.sub(r"\s*<[^>]*>","",a))')
year=$(date +%Y)
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>PocketAnon</string>
  <key>CFBundleDisplayName</key><string>PocketAnon</string>
  <key>CFBundleIdentifier</key><string>de.pocketanon.PocketAnon</string>
  <key>CFBundleVersion</key><string>$version</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>CFBundleExecutable</key><string>PocketAnon</string>
  <key>CFBundleIconFile</key><string>PocketAnon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>LSMinimumSystemVersion</key><string>12.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSHumanReadableCopyright</key><string>© $year $author. MIT OR Apache-2.0.</string>
  <key>CFBundleDocumentTypes</key>
  <array>
    <dict>
      <key>CFBundleTypeName</key><string>Document</string>
      <key>CFBundleTypeRole</key><string>Viewer</string>
      <key>LSItemContentTypes</key>
      <array>
        <string>public.plain-text</string>
        <string>org.openxmlformats.wordprocessingml.document</string>
        <string>public.rtf</string>
      </array>
    </dict>
  </array>
</dict>
</plist>
PLIST
touch "$app"
echo "built $app  (open it with: open $app)"
