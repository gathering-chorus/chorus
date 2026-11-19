#!/usr/bin/env bats
# @test-type: unit — reads the repo's committed files (git index, sparse dirs included); no network.
# @domain: security — every reader of the store reads from this machine (#4582)
#
# #4582 AC3 — LAN reads of Fuseki close, so every legitimate reader must read
# through loopback (localhost, 127.0.0.1, [::1]). The readers, measured 11-19
# 10:05 (ESTABLISHED :3030 = 3 connections, all ::1):
#   door athena-make :3360, chorus-api, the crawler, cws, the nightly and the
#   scripts — all localhost:3030 (or a FUSEKI_* variable that defaults to it).
# The one LAN reader: Bedroom's NiFi photo flow, built by the scripts listed in
# EXCEPTIONS below; it reads and writes 192.168.86.36:3030. Its write leg has
# been refused from the LAN since #3579 (July). Decision for that flow is on
# the card (Kade, convergence owner); until it is taken, these files are named
# here so a NEW LAN reader fails this check, and a stale entry fails too.
#
# A LAN address is any 192.168.x.x, 10.x.x.x or *.local host before :3030 on a
# line that is not a comment. Searched with `git grep --cached` so sparse-
# checkout dirs (roles/*/scripts) are included.

ROOT="$BATS_TEST_DIRNAME/../.."
LAN_3030='(192\.168\.[0-9]+\.[0-9]+|10\.[0-9]+\.[0-9]+\.[0-9]+|[A-Za-z0-9-]+\.local):3030'
EXCEPTIONS=(
  roles/kade/scripts/build-nifi-photos-flow.py
  building/products/convergence/one-shots/build-nifi-iphone-native.py
  building/products/convergence/one-shots/configure-nifi-pipeline.sh
  building/products/convergence/one-shots/fix-all-processors.py
  building/products/convergence/one-shots/fix-fuseki-target.py
  building/products/convergence/one-shots/fix-pipeline-from-existing.py
  building/products/convergence/one-shots/rebuild-pipeline-clean.py
)

# lan_readers: path:line:text on stdin → the paths with a non-comment LAN :3030 line, unique
lan_readers() {
  grep -E "$LAN_3030" | awk -F: '{ t=$0; sub(/^[^:]*:[^:]*:/, "", t); if (t !~ /^[[:space:]]*(#|\/\/)/) print $1 }' | sort -u
}

committed() {
  git -C "$ROOT" grep --cached -nIE "$LAN_3030" -- . ':!*.md' ':!*.html' ':!*.nt' ':!*.backup' ':!*.svg' \
    ':!platform/tests/4582-fuseki-readers-localhost.bats' || true
}

is_exception() { local p; for p in "${EXCEPTIONS[@]}"; do [ "$p" = "$1" ] && return 0; done; return 1; }

@test "AC3: no committed reader of the store uses a LAN address, outside the named NiFi flow" {
  found="$(committed | lan_readers)"
  [ -n "$(git -C "$ROOT" ls-files --cached roles/kade/scripts)" ] || { echo "roles/*/scripts not in the index: the scan would miss the known LAN reader"; false; }
  new=""
  while IFS= read -r p; do
    [ -n "$p" ] && ! is_exception "$p" && new+="$p"$'\n'
  done <<< "$found"
  [ -z "$new" ] || { printf 'new LAN readers of Fuseki (they break when LAN reads close):\n%s' "$new"; false; }
  for p in "${EXCEPTIONS[@]}"; do
    grep -qxF "$p" <<< "$found" || { echo "stale exception: $p no longer reads over the LAN; remove it"; false; }
  done
}

@test "AC3 NEGATIVE PROOF: a new LAN reader is caught, a comment and a loopback reader are not" {
  # built, not written, so this file pins no LAN address (3370 ratchet)
  PLANTED="192.168.$((80 + 6)).36"
  out="$(printf '%s\n' \
    "platform/scripts/new-reader.sh:4:curl -s http://$PLANTED:3030/pods/query" \
    'platform/scripts/other.py:9:URL = "http://jeffs-mac-mini.local:3030/pods/sparql"' \
    'platform/scripts/doc.sh:2:# Library (Jeffs-Mac-Mini-M1-3.local:3030) is primary' \
    'platform/scripts/ok.sh:3:curl -s http://localhost:3030/pods/query' | lan_readers)"
  [ "$out" = "$(printf '%s\n' platform/scripts/new-reader.sh platform/scripts/other.py)" ] || { echo "got: $out"; false; }
  ! is_exception platform/scripts/new-reader.sh || return 1
}
