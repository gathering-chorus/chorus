#!/bin/sh
# @domain: policies
# #4960 — a git tree in $1 holding one violation of policy-arch-css-sole-identity-issuer (the #4710 shape); the check run there must count it.
set -e
cd "$1"
git init -q .
mkdir -p svc/src
echo "fn verify_passkey() {}" > svc/src/go.rs
git add -A
