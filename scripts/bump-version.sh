#!/usr/bin/env bash
# Bumps one component's semver, independently of the others.
#
#   scripts/bump-version.sh <core|ios|android> <major|minor|patch|X.Y.Z> [--tag]
#
#   core      core/Cargo.toml [workspace.package] version (both crates inherit)
#   ios       apps/ios/project.yml MARKETING_VERSION; CURRENT_PROJECT_VERSION +1
#   android   apps/android/app/build.gradle.kts versionName; versionCode +1
#
# The shells' build number (CFBundleVersion / versionCode) increments on EVERY
# bump because stores require it to be monotonic per upload.
#
# --tag additionally commits the bump and creates the matching release tag
# (ios-vX.Y.Z+N / android-vX.Y.Z+N) that triggers the release workflow when
# pushed. The core has no release tag — it ships inside the shells.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
COMPONENT="${1:-}"
BUMP="${2:-}"
TAG_FLAG="${3:-}"

usage() {
  grep '^#' "$0" | sed 's/^# \{0,1\}//' | head -14
  exit 1
}

[ -n "$COMPONENT" ] && [ -n "$BUMP" ] || usage
if [ -n "$TAG_FLAG" ] && [ "$TAG_FLAG" != "--tag" ]; then usage; fi

bump_semver() { # current bump-kind-or-explicit
  local cur="$1" kind="$2"
  if [[ "$kind" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "$kind"
    return
  fi
  local major minor patch
  IFS=. read -r major minor patch <<<"$cur"
  case "$kind" in
    major) echo "$((major + 1)).0.0" ;;
    minor) echo "${major}.$((minor + 1)).0" ;;
    patch) echo "${major}.${minor}.$((patch + 1))" ;;
    *) echo "error: bump must be major|minor|patch|X.Y.Z, got '$kind'" >&2; exit 1 ;;
  esac
}

# BSD (macOS) and GNU sed disagree on -i; write via temp file instead.
sed_inplace() { # file sed-expr...
  local file="$1"; shift
  local tmp
  tmp="$(mktemp)"
  sed "$@" "$file" > "$tmp"
  mv "$tmp" "$file"
}

TAG=""
SCOPE=""
case "$COMPONENT" in
  core)
    FILE="$ROOT/core/Cargo.toml"
    CUR=$(sed -n '/\[workspace\.package\]/,/^\[/{s/^version = "\(.*\)"/\1/p;}' "$FILE")
    NEW=$(bump_semver "$CUR" "$BUMP")
    sed_inplace "$FILE" "s/^version = \"$CUR\"/version = \"$NEW\"/"
    # Keep Cargo.lock's own-crate versions in sync so the bump commit is clean.
    (cd "$ROOT/core" && rustup run stable cargo update --workspace --quiet 2>/dev/null || true)
    SCOPE="core"
    echo "core: $CUR -> $NEW"
    if [ "$TAG_FLAG" = "--tag" ]; then
      echo "error: core has no release tag; it ships inside the shells" >&2
      exit 1
    fi
    ;;
  ios)
    FILE="$ROOT/apps/ios/project.yml"
    CUR=$(sed -n 's/.*MARKETING_VERSION: "\(.*\)"/\1/p' "$FILE")
    BUILD=$(sed -n 's/.*CURRENT_PROJECT_VERSION: "\(.*\)"/\1/p' "$FILE")
    NEW=$(bump_semver "$CUR" "$BUMP")
    NEW_BUILD=$((BUILD + 1))
    sed_inplace "$FILE" \
      -e "s/MARKETING_VERSION: \"$CUR\"/MARKETING_VERSION: \"$NEW\"/" \
      -e "s/CURRENT_PROJECT_VERSION: \"$BUILD\"/CURRENT_PROJECT_VERSION: \"$NEW_BUILD\"/"
    SCOPE="ios"
    TAG="ios-v$NEW+$NEW_BUILD"
    echo "ios: $CUR ($BUILD) -> $NEW ($NEW_BUILD)"
    ;;
  android)
    FILE="$ROOT/apps/android/app/build.gradle.kts"
    CUR=$(sed -n 's/.*versionName = "\(.*\)"/\1/p' "$FILE")
    BUILD=$(sed -n 's/.*versionCode = \([0-9]*\)/\1/p' "$FILE")
    NEW=$(bump_semver "$CUR" "$BUMP")
    NEW_BUILD=$((BUILD + 1))
    sed_inplace "$FILE" \
      -e "s/versionName = \"$CUR\"/versionName = \"$NEW\"/" \
      -e "s/versionCode = $BUILD/versionCode = $NEW_BUILD/"
    SCOPE="android"
    TAG="android-v$NEW+$NEW_BUILD"
    echo "android: $CUR ($BUILD) -> $NEW ($NEW_BUILD)"
    ;;
  *)
    usage
    ;;
esac

if [ "$TAG_FLAG" = "--tag" ]; then
  cd "$ROOT"
  git add -A
  git commit -m "chore($SCOPE): bump version to $NEW"
  git tag "$TAG"
  echo "Committed and tagged $TAG. Push with: git push origin main $TAG"
elif [ -n "$TAG" ]; then
  echo "Files updated. Commit, then tag with: git tag $TAG"
fi
