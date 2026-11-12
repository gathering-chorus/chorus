#!/bin/sh
# @domain: policies
# #4963 — a git tree in $1 holding one violation of policy-arch-hook-single-responsibility; the check run there must count it.
set -e
cd "$1"
git init -q .
mkdir -p platform/services/chorus-hooks/src/hooks
printf 'pub fn run() {\n    emit_spine("zz.seen");\n    deny("no");\n}\n' > platform/services/chorus-hooks/src/hooks/zz_mixed.rs
git add -A
