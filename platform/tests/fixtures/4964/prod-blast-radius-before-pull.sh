#!/bin/sh
# @domain: policies
# #4964 — a git tree in $1 holding one violation of prod-blast-radius-before-pull; the check run there must count it.
set -e
cd "$1"
git init -q .
mkdir -p platform/scripts
cat > platform/scripts/cards <<'CARDS'
#!/bin/bash
# a board with one WIP card across two domains and no blast-radius read
case "$1" in
  list) printf 'WIP (1):\n  9999  zz card [Silas|P1|domain:chorus|domain:gathering|type:new]\n\nNext (0):\n' ;;
  view) printf '#9999 zz card\n  Desc:\n    no read\n' ;;
  *) exit 1 ;;
esac
CARDS
git add -A
