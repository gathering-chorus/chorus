#!/bin/sh
<<<<<<< HEAD
# @domain: policies
=======
>>>>>>> b6319870c (silas: #4960 — AC1 review (BQ): baseline as string and derivedFrom as one full IRI (PolicyCreate contract, with a test); the cross-graph rule row re-read whole (a | in its text had split it); a 16th runnable check, arch-css-sole-identity-issuer (passkey/sudoers in source, baseline 11, the #4710 class), with its fixture. 8/8.)
# #4960 — a git tree in $1 holding one violation of policy-arch-css-sole-identity-issuer (the #4710 shape); the check run there must count it.
set -e
cd "$1"
git init -q .
mkdir -p svc/src
echo "fn verify_passkey() {}" > svc/src/go.rs
git add -A
