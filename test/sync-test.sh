#!/usr/bin/env bash

# Sync the build from this checkout into a new repository, and check the sync and its drift check.
#
#   test/sync-test.sh

set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
repo=$(mktemp -d)
fresh=$(mktemp -d)
trap 'rm -rf "$repo" "$fresh"' EXIT

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
# the workflow of the build before it moved into build/, with the repository's current version
mkdir -p "$repo/.github/workflows"
cat > "$repo/.github/workflows/bump-version.yaml" <<'YAML'
        default: "9.9.9-dev" # the current version, kept current by scripts/bump-version.sh
YAML
cat > "$repo/tools/Cargo.toml" <<'TOML'
[dependencies]
wac-cli = "=0.11.0"
wkg = "=0.16.1"
TOML

echo "--- sync"
BUILD_SOURCE="$root" "$repo/build/scripts/update-build.sh" v0.0.0-test
[[ "$(cat "$repo/build/VERSION")" == v0.0.0-test ]] || fail "build/VERSION is not the version synced"
while read -r path; do
    # compared but for the repository's current version below
    [[ -z "$path" || "$path" == \#* || "$path" == .github/workflows/bump-version.yaml ]] && continue
    diff -r -x VERSION -x MANIFEST "$root/$path" "$repo/$path" > /dev/null || fail "$path differs from the build"
done < "$root/SYNC"
[[ -x "$repo/build/scripts/check-build.sh" ]] || fail "the scripts are not executable"
grep -qxF 'wac-cli = "=0.12.0"' "$repo/tools/Cargo.toml" || fail "wac-cli was not raised to the version the build requires"
grep -qxF 'wkg = "=0.16.1"' "$repo/tools/Cargo.toml" || fail "wkg was changed"

workflow=.github/workflows/bump-version.yaml
default_re='^ *default: "%s" # the current version, kept current by build/scripts/bump-version.sh$'
# shellcheck disable=SC2059
grep -qE "$(printf "$default_re" '9\.9\.9-dev')" "$repo/$workflow" || fail "the repository's current version was not carried forward"
diff <(sed -E 's/default: "[^"]*"/default: ""/' "$root/$workflow") <(sed -E 's/default: "[^"]*"/default: ""/' "$repo/$workflow") > /dev/null || fail "$workflow differs from the build, but for the current version"

echo "--- check"
"$repo/build/scripts/check-build.sh" || fail "the check failed after a sync"

echo "--- check a bumped current version"
sed -i.bak 's/default: "9.9.9-dev"/default: "9.9.9"/' "$repo/$workflow" && rm "$repo/$workflow.bak"
"$repo/build/scripts/check-build.sh" || fail "the check failed after bumping the current version"
BUILD_SOURCE="$root" "$repo/build/scripts/update-build.sh" v0.0.0-test > /dev/null
# shellcheck disable=SC2059
grep -qE "$(printf "$default_re" '9\.9\.9')" "$repo/$workflow" || fail "the bumped version was not carried forward"
"$repo/build/scripts/check-build.sh" || fail "the check failed after syncing a bumped version"

echo "--- check a change to the workflow other than its current version"
echo "# changed" >> "$repo/$workflow"
if "$repo/build/scripts/check-build.sh" > /dev/null; then fail "the check passed with a changed workflow"; fi
BUILD_SOURCE="$root" "$repo/build/scripts/update-build.sh" v0.0.0-test > /dev/null

echo "--- sync a repository without the workflow, its interface version is the current version"
mkdir -p "$fresh/build/scripts" "$fresh/wit"
cp "$root/build/scripts/update-build.sh" "$fresh/build/scripts/"
echo "package componentized:fresh@2.0.0-dev;" > "$fresh/wit/worlds.wit"
BUILD_SOURCE="$root" "$fresh/build/scripts/update-build.sh" v0.0.0-test > /dev/null
# shellcheck disable=SC2059
grep -qE "$(printf "$default_re" '2\.0\.0-dev')" "$fresh/$workflow" || fail "the interface version is not the current version"
"$fresh/build/scripts/check-build.sh" || fail "the check failed in a repository without the workflow before"

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
