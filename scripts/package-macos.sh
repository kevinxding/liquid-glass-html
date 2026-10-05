#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
python3 scripts/gpui-compat.py --check vendor/gpui-ce
cargo build --release --bin gpui-glass
app="artifacts/Fieldnotes.app"
mkdir -p "$app/Contents/MacOS"
mkdir -p "$app/Contents/Resources/Licenses"
cp LICENSES/Lisse.txt "$app/Contents/Resources/Licenses/Lisse.txt"
cp LICENSES/GPUI-Kit-Apache-2.0.txt "$app/Contents/Resources/Licenses/GPUI-Kit.txt"
cp crates/gpui-glass/LICENSE "$app/Contents/Resources/Licenses/gpui-glass.txt"
# Replace atomically: a running instance retains its executable, the next launch gets this build.
cp target/release/gpui-glass "$app/Contents/MacOS/Fieldnotes.next"
mv "$app/Contents/MacOS/Fieldnotes.next" "$app/Contents/MacOS/Fieldnotes"
cat > "$app/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>Fieldnotes</string>
<key>CFBundleDisplayName</key><string>Fieldnotes</string>
<key>CFBundleExecutable</key><string>Fieldnotes</string>
<key>CFBundleIdentifier</key><string>dev.gpui.glass.fieldnotes</string>
<key>CFBundleVersion</key><string>1</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
printf 'Built %s\n' "$app"
