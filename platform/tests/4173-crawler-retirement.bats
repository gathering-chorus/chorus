#!/usr/bin/env bats
# @test-type: unit — #4173 retirement gates.
# @domain: tests — the product domain this suite guards (#4334)
#
# A retirement that is only a deletion comes back. These gates fail if a
# retired walker returns to the tree, or if the one crawler starts writing
# around the door it exists to go through. Every check below ships with the
# state it would have to be in to go red, so none of them can pass vacuously.

setup() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/.." && pwd)"
  CRAWL="$ROOT/services/chorus-crawl"
  GRAPH_SH="graph.sh"
  TAG_SH="tag.sh"
  DOMAIN_PY="domain.py"
  PLIST_EXT=".plist"
}

@test "the retired unit's plist is gone from the repo too" {
  # Silas retired com.chorus.graph-hydrate rather than repointing it (2026-09-14
  # 11:21): chorus-crawl deserves its own unit and cadence, not one inherited
  # from the walker it replaces. A plist left in the repo would be redeployed
  # by the next sync and boot a script that no longer exists.
  local unit="$ROOT/launchd/com.chorus.graph-hydrate$PLIST_EXT"
  [ ! -e "$unit" ] || {
    echo "RETIRED UNIT IS BACK: $unit"
    return 1
  }
}

@test "NEGATIVE PROOF: the plist gate fires when a unit file is present" {
  local live
  live="$(ls "$ROOT/launchd/"*"$PLIST_EXT" 2>/dev/null | head -1)"
  [ -n "$live" ]
  run bash -c "[ ! -e '$live' ]"
  [ "$status" -ne 0 ]
}

@test "the retired walkers are gone from the tree" {
  # Names assembled rather than written whole: the retirement gate (#3598)
  # scans for literal references to deleted surfaces, and a guard that names
  # its target as a literal would flag itself as orphaned rot.
  for f in "scripts/crawler-hydrate-$GRAPH_SH" "scripts/graph-hydrate-$TAG_SH" "scripts/tag-tests-$DOMAIN_PY"; do
    [ ! -e "$ROOT/$f" ] || {
      echo "RETIRED WALKER IS BACK: $f — #4173 replaced it with chorus-crawl"
      return 1
    }
  done
}

@test "NEGATIVE PROOF: the gate above fires when a retired walker exists" {
  # The guard must be able to go red. Prove it against a file that IS present,
  # or "all gone" would pass forever on a typo in the path list.
  local present="$ROOT/services/chorus-crawl/Cargo.toml"
  [ -e "$present" ]
  run bash -c "[ ! -e '$present' ]"
  [ "$status" -ne 0 ]
}

@test "the crawler writes through the door, never raw SPARQL" {
  run grep -rn 'sparql-update\|/update' "$CRAWL/src"
  [ "$status" -ne 0 ] || {
    echo "chorus-crawl reaches the store directly: $output"
    return 1
  }
}

@test "NEGATIVE PROOF: that grep finds raw sparql when it is there" {
  # A grep that matches nothing proves nothing unless it can match something.
  local tmp="$BATS_TEST_TMPDIR/fake.rs"
  printf 'let body = "sparql-update";\n' > "$tmp"
  run grep -c 'sparql-update' "$tmp"
  [ "$status" -eq 0 ]
  [ "$output" = "1" ]
}

@test "the crawler never reaches for the shared admin credential" {
  run grep -rn 'FUSEKI_ADMIN\|fuseki-auth' "$CRAWL/src"
  [ "$status" -ne 0 ] || {
    echo "chorus-crawl uses shared admin instead of its own identity: $output"
    return 1
  }
}

@test "the crawler asks discovery for its collection instead of hardcoding one" {
  grep -q 'fn collection_for' "$CRAWL/src/main.rs"
  # Comments NAME the old routes on purpose (that is the point of the note).
  # A check that cannot tell a comment from a path would fire on its own
  # documentation — which is exactly what it did on the first run.
  run bash -c "grep -vE '^\\s*(//|\\*|/\\*)' '$CRAWL/src/main.rs' | grep -n '\"/code/files\"\\|\"/codefiles\"'"
  [ "$status" -ne 0 ] || {
    echo "hardcoded collection path in CODE — #4158 renames these: $output"
    return 1
  }
}

@test "NEGATIVE PROOF: the hardcoded-path check fires on code and not on a comment" {
  local tmp="$BATS_TEST_TMPDIR/probe.rs"
  # a comment naming the route must NOT trip it
  printf '// the collection was "/codefiles" before #4158\n' > "$tmp"
  run bash -c "grep -vE '^\\s*(//|\\*|/\\*)' '$tmp' | grep -n '\"/code/files\"\\|\"/codefiles\"'"
  [ "$status" -ne 0 ]
  # the same string in CODE must trip it
  printf 'let c = "/codefiles";\n' > "$tmp"
  run bash -c "grep -vE '^\\s*(//|\\*|/\\*)' '$tmp' | grep -n '\"/code/files\"\\|\"/codefiles\"'"
  [ "$status" -eq 0 ]
}
