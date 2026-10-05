#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
python3 scripts/gpui-compat.py --check vendor/gpui-ce
cargo build --release --bin market-demo
app="artifacts/Prism.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources/Licenses"
cp LICENSES/Lisse.txt "$app/Contents/Resources/Licenses/Lisse.txt"
cp crates/gpui-glass/LICENSE "$app/Contents/Resources/Licenses/gpui-glass.txt"
cp target/release/market-demo "$app/Contents/MacOS/Prism.next"
mv "$app/Contents/MacOS/Prism.next" "$app/Contents/MacOS/Prism"
cat > "$app/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>Prism</string>
<key>CFBundleDisplayName</key><string>Prism Stress Test</string>
<key>CFBundleExecutable</key><string>Prism</string>
<key>CFBundleIdentifier</key><string>dev.gpui.glass.prism</string>
<key>CFBundleVersion</key><string>1</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
printf 'Built %s\n' "$app"
