#!/bin/sh
# @domain: policies
# #4964 — a git tree in $1 holding one violation of data-single-graph-multi-lens; the check run there must count it.
set -e
cd "$1"
git init -q .
mkdir -p platform/api/src
printf 'const db = new Database(path.join(home, "zz-view.db"));\n' > platform/api/src/zz-view.ts
git add -A
