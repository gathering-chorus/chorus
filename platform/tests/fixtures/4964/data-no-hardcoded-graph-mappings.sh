#!/bin/sh
# @domain: policies
# #4964 — a git tree in $1 holding one violation of data-no-hardcoded-graph-mappings; the check run there must count it.
set -e
cd "$1"
git init -q .
mkdir -p platform/api/src
printf 'export const OWNERS = { domain: "wren" };\n' > platform/api/src/zz-owners.ts
git add -A
