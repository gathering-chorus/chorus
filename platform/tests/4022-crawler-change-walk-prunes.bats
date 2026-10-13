#!/usr/bin/env bats
# @test-type: unit
# @domain: search — the product domain this suite guards (#4334)
# #4022 — the index crawler asks "did any file change since my last run?" with
# find -newer over platform/. `-not -path */node_modules/*` filtered the answer
# but find still walked every node_modules and target tree: 73.5s per crawler
# run on 2026-10-13, on every prod tick and 6 times in crawler-error-tracking.bats.
# changed_since must never open a pruned directory.

LIB="$BATS_TEST_DIRNAME/../scripts/lib/changed-since.sh"

setup() {
  T="$BATS_TEST_TMPDIR"
  mkdir -p "$T/tree/src" "$T/tree/node_modules/pkg" "$T/tree/svc/target/debug" "$T/tree/web/dist"
  touch "$T/tree/src/old.ts"
  touch -t 202001010000 "$T/stamp"
  touch -t 201901010000 "$T/tree/src/old.ts"
  . "$LIB"
}

teardown() {
  chmod -R u+rwx "$T/tree" 2>/dev/null || true
}

@test "a changed source file is found" {
  touch "$T/tree/src/new.ts"
  run changed_since "$T/stamp" "$T/tree"
  [ "$status" -eq 0 ]
  [ "$output" = "$T/tree/src/new.ts" ]
}

@test "nothing changed answers empty" {
  run changed_since "$T/stamp" "$T/tree"
  [ "$status" -eq 0 ]
  [ -z "$output" ]
}

@test "a change only inside node_modules, target or dist is not a change" {
  touch "$T/tree/node_modules/pkg/a.js" "$T/tree/svc/target/debug/b" "$T/tree/web/dist/c.js"
  touch -t 201901010000 "$T/tree/src/old.ts"
  run changed_since "$T/stamp" "$T/tree"
  [ "$status" -eq 0 ]
  [ -z "$output" ]
}

# NEGATIVE PROOF: a walk that enters node_modules or target hits these
# unreadable directories and find reports the error (exit 1). A pruned walk
# never opens them. The old `-not -path` expression fails this case.
@test "pruned directories are never opened" {
  chmod 000 "$T/tree/node_modules" "$T/tree/svc/target"
  run changed_since "$T/stamp" "$T/tree"
  [ "$status" -eq 0 ] || { echo "walked into a pruned dir: status $status, $output"; return 1; }
}
