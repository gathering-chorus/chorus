#!/usr/bin/env bats
# @test-type: unit — hermetic: fixture pin + fixture werk repo; no services.
#
# #3956 — a flake costs the flake, not the pipeline. werk-resume-check decides
# per leg: CARRY (prior pass, same tree), RUN (no proof / not resuming), or
# REFUSE (tree changed / pin missing on an attempted resume — both loud).
# Exit codes: 0=carry, 1=run, 2=refuse. Carried legs emit werk.leg.carried.

setup() {
  REPO="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  SCRIPT="$REPO/platform/scripts/werk-resume-check"
  T="$(mktemp -d)"; export HOME="$T/home"; mkdir -p "$HOME/.chorus/werk-runs"
  export CHORUS_HOME="$T/chorus_home"; mkdir -p "$CHORUS_HOME/ops/logs"
  export CHORUS_WERK_BASE="$T/werks"; mkdir -p "$CHORUS_WERK_BASE/kade-42"
  git -C "$CHORUS_WERK_BASE/kade-42" init -q -b kade/42 .
  ( cd "$CHORUS_WERK_BASE/kade-42" && echo x > f && git add . \
    && git -c user.email=t@t -c user.name=t commit -q -m "kade: #42" )
  export ROLE=kade CARD_ID=42
  TREE=$(bash "$SCRIPT" --tree-hash 42 kade)
}

teardown() { rm -rf "$T"; }

pin_legs() { # pin_legs <tree_hash> <leg:verdict>...
  local th="$1"; shift
  local legs=""
  for lv in "$@"; do
    legs="${legs:+$legs,}{\"leg\":\"${lv%%:*}\",\"verdict\":\"${lv##*:}\",\"tree_hash\":\"$th\"}"
  done
  printf '{"runId":"42-kade-1-9","phase":"failed","legs":[%s]}' "$legs" \
    > "$HOME/.chorus/werk-runs/42.json"
}

@test "tree-hash is stable and sees uncommitted edits" {
  h1=$(bash "$SCRIPT" --tree-hash 42 kade)
  [ "$h1" = "$TREE" ]
  echo dirty >> "$CHORUS_WERK_BASE/kade-42/f"
  h2=$(bash "$SCRIPT" --tree-hash 42 kade)
  [ "$h2" != "$TREE" ]
}

@test "resume carries a leg that passed against the same tree, citing the prior run" {
  pin_legs "$TREE" build:pass test:pass
  WERK_RESUME=1 run bash "$SCRIPT" 42 kade build
  [ "$status" -eq 0 ]
  echo "$output" | grep -q "carried"
  echo "$output" | grep -q "42-kade-1-9"   # prior run cited
}

@test "resume runs the leg that failed" {
  pin_legs "$TREE" build:pass test:fail
  WERK_RESUME=1 run bash "$SCRIPT" 42 kade test
  [ "$status" -eq 1 ]
}

@test "resume runs a leg with no recorded verdict" {
  pin_legs "$TREE" build:pass
  WERK_RESUME=1 run bash "$SCRIPT" 42 kade demo
  [ "$status" -eq 1 ]
}

@test "no WERK_RESUME flag → always run (resume is opt-in by the launcher)" {
  pin_legs "$TREE" build:pass
  run bash "$SCRIPT" 42 kade build
  [ "$status" -eq 1 ]
}

@test "NEGATIVE PROOF (#3734): tree changed since the failed run — REFUSE naming both hashes" {
  pin_legs "$TREE" build:pass
  echo drift >> "$CHORUS_WERK_BASE/kade-42/f"
  NOW=$(bash "$SCRIPT" --tree-hash 42 kade)
  WERK_RESUME=1 run bash "$SCRIPT" 42 kade build
  [ "$status" -eq 2 ]
  echo "$output" | grep -q "$TREE"
  echo "$output" | grep -q "$NOW"
}

@test "NEGATIVE PROOF (#3734): resume with NO pin refuses loudly, never silently full-runs" {
  rm -f "$HOME/.chorus/werk-runs/42.json"
  WERK_RESUME=1 run bash "$SCRIPT" 42 kade build
  [ "$status" -eq 2 ]
  echo "$output" | grep -qi "no run pin"
}

@test "carried leg emits werk.leg.carried to the witness log" {
  pin_legs "$TREE" build:pass
  WERK_RESUME=1 bash "$SCRIPT" 42 kade build >/dev/null
  grep -q '"event":"werk.leg.carried"' "$CHORUS_HOME/ops/logs/werk.jsonl"
  grep -q '"leg":"build"' "$CHORUS_HOME/ops/logs/werk.jsonl"
}
