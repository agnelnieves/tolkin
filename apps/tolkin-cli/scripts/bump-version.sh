#!/usr/bin/env bash
# Bump the Tolkin version (semver) across every file that carries it:
#   - apps/tolkin-cli/Cargo.toml            [package] version
#   - apps/tolkin-cli/package.json          Turbo workspace stub
#   - npm/tolkin/package.json               version + optionalDependencies pins
#   - npm/tolkin-darwin-arm64/package.json  version
#   - npm/tolkin-darwin-x64/package.json    version
#   - npm/tolkin-linux-x64/package.json     version
#   - npm/tolkin-linux-arm64/package.json   version
#   - npm/tolkin-win32-x64/package.json     version
#   - Cargo.lock                            refreshed via cargo metadata
#
# Usage: scripts/bump-version.sh patch|minor|major|<x.y.z>
set -euo pipefail

cd "$(dirname "$0")/.."

WRAPPER="npm/tolkin/package.json"
ARM64="npm/tolkin-darwin-arm64/package.json"
X64="npm/tolkin-darwin-x64/package.json"
LINUX_X64="npm/tolkin-linux-x64/package.json"
LINUX_ARM64="npm/tolkin-linux-arm64/package.json"
WIN32_X64="npm/tolkin-win32-x64/package.json"
SKILL_AUDIT="../../distribution/skills/tolkin-audit/SKILL.md"
SKILL_OPTIMIZE="../../distribution/skills/tolkin-optimize/SKILL.md"
SKILL_SLIM="../../distribution/skills/tolkin-slim/SKILL.md"
SKILL_CACHE="../../distribution/skills/tolkin-cache/SKILL.md"

current=$(sed -n 's/^  "version": "\(.*\)",$/\1/p' "$WRAPPER" | head -1)
if [[ ! "$current" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "error: could not read a semver version from $WRAPPER (got: '$current')" >&2
  exit 1
fi

IFS=. read -r major minor patch <<< "$current"
case "${1:-}" in
  patch) next="$major.$minor.$((patch + 1))" ;;
  minor) next="$major.$((minor + 1)).0" ;;
  major) next="$((major + 1)).0.0" ;;
  [0-9]*.[0-9]*.[0-9]*) next="$1" ;;
  *)
    echo "usage: $0 patch|minor|major|<x.y.z>   (current: $current)" >&2
    exit 1
    ;;
esac

echo "Bumping tolkin: $current -> $next"

# perl -pi -e is portable across macOS (BSD) and Linux (GNU); sed -i '' is BSD-only.
# Pass versions via environment to avoid quoting hazards with special regex chars.
CURRENT="$current" NEXT="$next" perl -pi -e \
  's/^version = "\Q$ENV{CURRENT}\E"$/version = "$ENV{NEXT}"/' Cargo.toml
for f in "$WRAPPER" "$ARM64" "$X64" "$LINUX_X64" "$LINUX_ARM64" "$WIN32_X64" "package.json"; do
  CURRENT="$current" NEXT="$next" perl -pi -e \
    's/^  "version": "\Q$ENV{CURRENT}\E",$/  "version": "$ENV{NEXT}",/' "$f"
done
for f in "$SKILL_AUDIT" "$SKILL_OPTIMIZE" "$SKILL_SLIM" "$SKILL_CACHE"; do
  CURRENT="$current" NEXT="$next" perl -pi -e \
    's/^  version: \Q$ENV{CURRENT}\E$/  version: $ENV{NEXT}/' "$f"
done
for pkg in tolkin-darwin-arm64 tolkin-darwin-x64 tolkin-linux-x64 tolkin-linux-arm64 tolkin-win32-x64; do
  CURRENT="$current" NEXT="$next" PKG="$pkg" perl -pi -e \
    's/"\Q$ENV{PKG}\E": "\Q$ENV{CURRENT}\E"/"$ENV{PKG}": "$ENV{NEXT}"/' "$WRAPPER"
done

# cargo metadata refreshes Cargo.lock for the new crate version without a build.
cargo metadata --format-version 1 > /dev/null

# Verify every carrier agrees before declaring success.
fail=0
grep -q "^version = \"$next\"" Cargo.toml || { echo "Cargo.toml did not update" >&2; fail=1; }
grep -q "\"version\": \"$next\"" "$WRAPPER" || { echo "$WRAPPER did not update" >&2; fail=1; }
grep -q "\"version\": \"$next\"" "$ARM64" || { echo "$ARM64 did not update" >&2; fail=1; }
grep -q "\"version\": \"$next\"" "$X64" || { echo "$X64 did not update" >&2; fail=1; }
grep -q "\"version\": \"$next\"" "$LINUX_X64" || { echo "$LINUX_X64 did not update" >&2; fail=1; }
grep -q "\"version\": \"$next\"" "$LINUX_ARM64" || { echo "$LINUX_ARM64 did not update" >&2; fail=1; }
grep -q "\"version\": \"$next\"" "$WIN32_X64" || { echo "$WIN32_X64 did not update" >&2; fail=1; }
for pkg in tolkin-darwin-arm64 tolkin-darwin-x64 tolkin-linux-x64 tolkin-linux-arm64 tolkin-win32-x64; do
  grep -q "\"$pkg\": \"$next\"" "$WRAPPER" || { echo "optionalDependencies $pkg pin did not update" >&2; fail=1; }
done
grep -q "^  version: $next$" "$SKILL_AUDIT" || { echo "$SKILL_AUDIT did not update" >&2; fail=1; }
grep -q "^  version: $next$" "$SKILL_OPTIMIZE" || { echo "$SKILL_OPTIMIZE did not update" >&2; fail=1; }
grep -q "^  version: $next$" "$SKILL_SLIM" || { echo "$SKILL_SLIM did not update" >&2; fail=1; }
grep -q "^  version: $next$" "$SKILL_CACHE" || { echo "$SKILL_CACHE did not update" >&2; fail=1; }
[ "$fail" -eq 0 ] || exit 1

echo "Done. All carriers at $next."
echo "Next: commit, push to main, and the tolkin-publish workflow handles the rest."
echo "For a local publish instead: bash npm/build.sh, then publish arm64, x64, wrapper in that order."
