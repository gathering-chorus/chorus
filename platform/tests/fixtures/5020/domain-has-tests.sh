#!/bin/sh
# @domain: policies
# #5020 — a git tree in $1 holding one violation of domain-has-tests: domain x has no test file
# naming @domain: x (y has one; a non-test file naming x does not count).
set -e
cd "$1"
git init -q .
mkdir -p platform/tests platform/scripts
printf '#!/usr/bin/env bats\n# @domain: y\n@test "y" { true; }\n' > platform/tests/y.bats
printf '#!/bin/sh\n# @domain: x\n' > platform/scripts/x-tool
printf '#!/usr/bin/env bats\n# @domain: xx\n@test "xx" { true; }\n' > platform/tests/xx.bats
git add -A
