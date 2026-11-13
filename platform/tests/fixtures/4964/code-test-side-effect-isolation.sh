#!/bin/sh
# @domain: policies
# #4964 — a git tree in $1 holding one violation of code-test-side-effect-isolation; the check run there must count it.
set -e
cd "$1"
git init -q .
mkdir -p platform/tests
printf '@test "zz" {\n  ENDPOINT=http://127.0.0.1:3030/pods/update run-the-writer\n}\n' > platform/tests/zz.bats
git add -A
