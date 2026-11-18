#!/usr/bin/env bash

# Smoke test the componentized-build cli, run by the CI workflow with the cli unpacked from its
# archive, and by `make test-cli` with the cli installed with the tools. Run in this repository, it
# lists the dependencies, checks the component pulled from componentized/logging is up to date, looked
# up in the registry, and that an id that isn't a dependency is an error.
#
#   scripts/smoke-test-cli.sh <cli> [<dir>]

set -euo pipefail

cli=$1

cd "$(dirname "$0")/.."

fail() {
  echo "::error::$1"
  exit 1
}

# without the carriage returns of windows
run() {
  "$cli" "$@" | tr -d '\r'
}

run --version

deps=$(run deps)
echo "${deps}"
grep -q '^componentized:logging  *0\.3\.1  *components to-stderr$' <<< "${deps}" \
  || fail "expected the dependencies to list componentized:logging 0.3.1"

dep=$(run dep componentized:logging --dry-run)
echo "${dep}"
grep -q '^Up to date' <<< "${dep}" || fail "expected componentized:logging to be up to date"

if error=$("$cli" dep componentized:loging 2>&1); then
  fail "expected componentized:loging, not a dependency, to be an error"
fi
echo "${error}"
grep -q 'did you mean componentized:logging?' <<< "${error}" \
  || fail "expected the error to suggest componentized:logging"
