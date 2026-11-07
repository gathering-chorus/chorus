#!/usr/bin/env bats
# @test-type: integration — every tracked test file against the store's Domain list (the live door).
# @domain: tests
# @card: #4915
# @needs: machine
# #4915 round 2 (Jeff's go 11-07 08:47): every test file's @domain names a domain the
# store lists. On 11-07, 22 did not (proving ×15, werk ×3, chorus ×3, autonomy ×1),
# and crawler-out-of-sync fired for hours as test-declared-domain-missing.

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"

listed() { curl -sf --max-time 10 "${ATHENA_MAKE_URL:-http://localhost:3360}/v1/domains/domains?limit=500" | jq -r '.data[].name'; }

# test files (the crawler's kinds) in a git tree whose header names a domain not in $1
unlisted_in() {
  local tree="$1" names="$2"
  git -C "$tree" ls-files -z -- '*.bats' '*.test.*' '*.spec.*' '*/tests/*.rs' '*.test.sh' '*.feature' \
    | xargs -0 -I{} sh -c 'f="$1"; case "$f" in */fixtures/*) exit 0;; esac; head -c 2000 "$2/$f" | grep -m1 -iE "^[[:space:]]*(#|//|\*)[[:space:]]*@domain:[[:space:]]*[a-z]" | sed -E "s/.*@[Dd][Oo][Mm][Aa][Ii][Nn]:[[:space:]]*([A-Za-z0-9-]+).*/\1/" | tr "A-Z" "a-z" | sed "s|^|$f |"' _ {} "$tree" \
    | while read -r f d; do printf '%s\n' "$names" | grep -qx "$d" || echo "$f $d"; done
}

@test "every tracked test file names a listed domain" {
  names="$(listed)"
  [ -n "$names" ] || { echo "UNMEASURED: the store did not list its Domains"; return 1; }
  run unlisted_in "$ROOT" "$names"
  [ "$status" -eq 0 ]
  [ -z "$output" ] || { echo "unlisted: $output"; return 1; }
}

@test "negative proof: a planted unlisted domain is named" {
  T="$(mktemp -d "${BATS_TMPDIR:-/tmp}/t4915.XXXXXX")"
  git -C "$T" init -q && mkdir -p "$T/platform/tests"
  printf '# @domain: nowhere\n' > "$T/platform/tests/a.bats"
  printf '# @domain: cicd\n' > "$T/platform/tests/b.bats"
  git -C "$T" add -A
  run unlisted_in "$T" "$(printf 'cicd\ntests\n')"
  rm -rf "$T"
  [ "$output" = "platform/tests/a.bats nowhere" ]
}
