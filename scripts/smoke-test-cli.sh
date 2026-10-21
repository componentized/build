#!/usr/bin/env bash

# Smoke test the hello cli, run by the CI workflow with the cli unpacked from its archive.
#
#   scripts/smoke-test-cli.sh <cli> <dir>

set -euo pipefail

cli=$1

actual=$("$cli" build | tr -d '\r')
echo "hello build = ${actual}"
if [ "${actual}" != "Hello, build!" ] ; then
  echo "::error::expected 'Hello, build!', got '${actual}'"
  exit 1
fi
