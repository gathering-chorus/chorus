#!/bin/sh
# @domain: policies
# #4960 — a git tree in $1 holding one violation of policy-code-domain-api-over-raw-fuseki; the check run there must count it.
set -e
cd "$1"
git init -q .
mkdir -p app
echo "fetch(\"http://localhost:3030/pods/query\")" > app/q.ts
git add -A
