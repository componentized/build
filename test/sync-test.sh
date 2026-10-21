#!/usr/bin/env bash

# Sync the build from this checkout into a new repository, and check the sync and its drift check.
#
#   test/sync-test.sh

set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
repo=$(mktemp -d)
trap 'rm -rf "$repo"' EXIT

fail() {
    echo "FAIL: $*" >&2
    exit 1
}

# a repository with the build's files and a tool pinned below the version the build requires
mkdir -p "$repo/build/scripts" "$repo/tools"
# an older copy of the script, the sync replaces it while it runs
# shellcheck disable=SC2016
sed '2i\
# an older version, longer than the version synced, bash would continue reading it at the wrong place
' "$root/build/scripts/update-build.sh" > "$repo/build/scripts/update-build.sh"
chmod +x "$repo/build/scripts/update-build.sh"
cat > "$repo/tools/Cargo.toml" <<'TOML'
[dependencies]
wac-cli = "=0.11.0"
wkg = "=0.16.1"
TOML

echo "--- sync"
BUILD_SOURCE="$root" "$repo/build/scripts/update-build.sh" v0.0.0-test
[[ "$(cat "$repo/build/VERSION")" == v0.0.0-test ]] || fail "build/VERSION is not the version synced"
while read -r path; do
    [[ -z "$path" || "$path" == \#* ]] && continue
    diff -r -x VERSION -x MANIFEST "$root/$path" "$repo/$path" > /dev/null || fail "$path differs from the build"
done < "$root/SYNC"
[[ -x "$repo/build/scripts/check-build.sh" ]] || fail "the scripts are not executable"
grep -qxF 'wac-cli = "=0.12.0"' "$repo/tools/Cargo.toml" || fail "wac-cli was not raised to the version the build requires"
grep -qxF 'wkg = "=0.16.1"' "$repo/tools/Cargo.toml" || fail "wkg was changed"

echo "--- check"
"$repo/build/scripts/check-build.sh" || fail "the check failed after a sync"

echo "--- check a changed file"
echo "# changed" >> "$repo/build/build.mk"
if "$repo/build/scripts/check-build.sh"; then fail "the check passed with a changed file"; fi

echo "--- check an added file"
BUILD_SOURCE="$root" "$repo/build/scripts/update-build.sh" v0.0.0-test > /dev/null
"$repo/build/scripts/check-build.sh" || fail "the check failed after syncing again"
touch "$repo/build/local.mk"
if "$repo/build/scripts/check-build.sh"; then fail "the check passed with an added file"; fi
rm "$repo/build/local.mk"

echo "--- sync removes the files the build no longer has"
echo "0000000000000000000000000000000000000000000000000000000000000000  build/removed.mk" >> "$repo/build/MANIFEST"
touch "$repo/build/removed.mk"
BUILD_SOURCE="$root" "$repo/build/scripts/update-build.sh" v0.0.0-test > /dev/null
[[ ! -e "$repo/build/removed.mk" ]] || fail "a file the build no longer has was kept"
"$repo/build/scripts/check-build.sh" || fail "the check failed after removing a file"

echo "--- sync refuses paths outside the repository"
echo "0000000000000000000000000000000000000000000000000000000000000000  ../outside" >> "$repo/build/MANIFEST"
if BUILD_SOURCE="$root" "$repo/build/scripts/update-build.sh" v0.0.0-test > /dev/null 2>&1; then fail "a path outside the repository was synced"; fi

echo "PASS"
