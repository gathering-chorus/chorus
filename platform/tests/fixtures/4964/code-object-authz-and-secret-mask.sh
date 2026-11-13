#!/bin/sh
# @domain: policies
# #4964 — a git tree in $1 holding one violation of code-object-authz-and-secret-mask; the check run there must count it.
set -e
cd "$1"
git init -q .
mkdir -p platform/api/src
printf 'res.json({ name: row.name, password: row.password });\n' > platform/api/src/zz-leak.ts
git add -A
