#!/usr/bin/env bats
# @test-type: unit — reads repo files and a fixture; nothing live
# @domain: builds
#
# #4880 — crawler-out-of-sync fired every 15 min from 10-26 on data, not code:
# 4 test files declared `@domain: werk` (no such domain) or no domain at all,
# and 12 endpoint rows outlived the media player's move out of this repo (#4748).

ROOT="${BATS_TEST_DIRNAME}/../.."
FILES=(
  platform/services/werk-push/src/lib.rs
  platform/services/werk-sync/src/lib.rs
  platform/services/werk-teardown/src/lib.rs
  platform/tests/werk-node-modules.bats
)

declared() { # the crawler's rule: the first `@domain:` header names the domain
  grep -m1 -oE '@[Dd]omain:[[:space:]]*[a-z][a-z0-9-]*' "$1" | sed -E 's/.*:[[:space:]]*//'
}

@test "AC2: the four files exist (a renamed or deleted target fails loudly)" {
  for f in "${FILES[@]}"; do [ -f "$ROOT/$f" ] || { echo "missing target: $f"; return 1; }; done
}

@test "AC2: each of the four declares builds, the werk verbs' domain" {
  for f in "${FILES[@]}"; do
    d=$(declared "$ROOT/$f")
    [ "$d" = "builds" ] || { echo "$f declares '${d:-nothing}'"; return 1; }
  done
}

@test "AC2: no file in the repo declares the nonexistent domain werk" {
  run git -C "$ROOT" grep -lE '@[Dd]omain:[[:space:]]*werk([[:space:]]|$)' -- ':!platform/tests/crawler-sync-data.bats'
  [ "$status" -ne 0 ] || { echo "still declared: $output"; return 1; }
}

@test "negative proof: the check reads a werk header and a headerless test as wrong" {
  fx="$BATS_TEST_TMPDIR/fx"; mkdir -p "$fx"
  printf '#!/usr/bin/env bats\n# @domain: werk\n' > "$fx/a.bats"
  printf '//! no header\n#[test]\nfn t() {}\n' > "$fx/b.rs"
  [ "$(declared "$fx/a.bats")" = "werk" ]
  [ "$(declared "$fx/a.bats")" != "builds" ] || return 1
  [ -z "$(declared "$fx/b.rs")" ] || return 1
}
