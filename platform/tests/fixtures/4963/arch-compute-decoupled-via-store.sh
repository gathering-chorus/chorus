#!/bin/sh
# @domain: policies
# #4963 — a git tree in $1 holding one violation of policy-arch-compute-decoupled-via-store; the check run there must count it.
set -e
cd "$1"
git init -q .
mkdir -p platform/api/src
echo 'const board = new Database(process.env.VIKUNJA_DB_PATH);' > platform/api/src/board.ts
git add -A
