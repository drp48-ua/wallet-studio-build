#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")"
python3 -m unittest discover -s tests -p 'test_*.py'
for sdk in iphonesimulator iphoneos; do
  app="build/$sdk/ShaderDiagnostics.app"
  mkdir -p "$app"
  target=arm64-apple-ios15.0
  if [[ "$sdk" == iphonesimulator ]]; then target="$(uname -m)-apple-ios15.0-simulator"; fi
  xcrun --sdk "$sdk" clang -fobjc-arc -fblocks -Wall -Wextra -Werror \
    -Wno-unused-parameter -Wno-deprecated-declarations -target "$target" \
    -isysroot "$(xcrun --sdk "$sdk" --show-sdk-path)" main.m \
    -framework Foundation -framework UIKit -framework CoreGraphics -o "$app/ShaderDiagnostics"
  cp Info.plist "$app/Info.plist"
  if [[ "$sdk" == iphonesimulator ]]; then codesign --force --sign - "$app"; fi
done
python3 - <<'PY'
import pathlib,zipfile
source=pathlib.Path('build/iphoneos/ShaderDiagnostics.app')
with zipfile.ZipFile('build/Wallet-Studio-Shaders-sin-firmar.ipa','w',zipfile.ZIP_DEFLATED) as z:
    for path in sorted(source.iterdir()):
        info=zipfile.ZipInfo('Payload/ShaderDiagnostics.app/'+path.name)
        info.external_attr=(0o100755 if path.name=='ShaderDiagnostics' else 0o100644)<<16
        z.writestr(info,path.read_bytes())
PY
