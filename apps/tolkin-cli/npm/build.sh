#!/usr/bin/env bash
# Builds the Tolkin darwin binaries and stages them into the npm platform
# packages, then dry-runs npm pack for the darwin packages and the wrapper.
# Linux binaries are built and staged by CI (tolkin-publish.yml); their
# packages are packed here only when a binary is already present.
set -euo pipefail

cd "$(dirname "$0")/.."

echo "==> Building darwin-arm64 (native)"
cargo build --release

echo "==> Building darwin-x64 (cross)"
cargo build --release --target x86_64-apple-darwin

echo "==> Staging binaries into npm platform packages"
cp target/release/tolkin npm/tolkin-darwin-arm64/bin/tolkin
cp target/x86_64-apple-darwin/release/tolkin npm/tolkin-darwin-x64/bin/tolkin
chmod +x npm/tolkin-darwin-arm64/bin/tolkin npm/tolkin-darwin-x64/bin/tolkin

for pkg in tolkin-darwin-arm64 tolkin-darwin-x64 tolkin; do
  echo ""
  echo "==> npm pack --dry-run: ${pkg}"
  (cd "npm/${pkg}" && npm pack --dry-run)
done

# The linux and win32 packages are staged by CI (.github/workflows/tolkin-publish.yml),
# not by this script. Pack them only when a binary happens to be present.
for pkg in tolkin-linux-x64 tolkin-linux-arm64; do
  if [ -f "npm/${pkg}/bin/tolkin" ]; then
    echo ""
    echo "==> npm pack --dry-run: ${pkg}"
    (cd "npm/${pkg}" && npm pack --dry-run)
  else
    echo ""
    echo "==> skipping npm pack for ${pkg} (no binary staged; CI builds it)"
  fi
done
if [ -f "npm/tolkin-win32-x64/bin/tolkin.exe" ]; then
  echo ""
  echo "==> npm pack --dry-run: tolkin-win32-x64"
  (cd "npm/tolkin-win32-x64" && npm pack --dry-run)
else
  echo ""
  echo "==> skipping npm pack for tolkin-win32-x64 (no binary staged; CI builds it)"
fi

echo ""
echo "==> Publish order (run by CI or the coordinator, not this script):"
echo "    1. npm publish npm/tolkin-darwin-arm64"
echo "    2. npm publish npm/tolkin-darwin-x64"
echo "    3. npm publish npm/tolkin-linux-x64"
echo "    4. npm publish npm/tolkin-linux-arm64"
echo "    5. npm publish npm/tolkin-win32-x64"
echo "    6. npm publish npm/tolkin"
echo "    Platform packages must exist on the registry before the wrapper,"
echo "    so its optionalDependencies resolve at install time."
