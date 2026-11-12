#!/bin/sh
# @domain: policies
# #4963 — a git tree in $1 holding one violation of policy-arch-one-execution-path; the check run there must count it.
set -e
cd "$1"
git init -q .
mkdir -p platform/services/zz-verb platform/scripts
printf '[package]\nname = "zz-verb"\n' > platform/services/zz-verb/Cargo.toml
printf '#!/bin/sh\necho "the old bash path still does the work"\n' > platform/scripts/zz-verb
git add -A
