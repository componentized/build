#!/usr/bin/env bash

# Check the files synced from componentized/build are unchanged since build/scripts/update-build.sh
# synced them, the version in build/VERSION.
#
#   build/scripts/check-build.sh
#
# Fails when a synced file was changed or removed, or a file was added to build/. A change to the
# build belongs in componentized/build, released and synced to each repository, rather than here.

set -euo pipefail

cd "$(dirname "$0")/../.."

# componentized/build itself, the source of the files rather than a copy of them
if [[ -f SYNC ]]; then
    echo "the build repository, nothing synced to check"
    exit 0
fi

if [[ ! -f build/MANIFEST || ! -f build/VERSION ]]; then
    echo "unable to find build/MANIFEST and build/VERSION, run \`make update-build\` to sync the build" >&2
    exit 1
fi
version=$(cat build/VERSION)

status=0
if ! shasum -a 256 --check --quiet build/MANIFEST; then
    status=1
fi

# build/ only holds synced files
while read -r path; do
    case "$path" in
        build/VERSION | build/MANIFEST) ;;
        *)
            if ! grep -qE "^[0-9a-f]{64}  ${path//./\\.}$" build/MANIFEST; then
                echo "${path}: not synced from componentized/build ${version}"
                status=1
            fi
            ;;
    esac
done < <(find build -type f | sort)

if [[ "$status" != 0 ]]; then
    echo "the build differs from componentized/build ${version}. Make the change in componentized/build and update to its release, or run \`make update-build BUILD_VERSION=${version}\` to undo it." >&2
fi
exit "$status"
