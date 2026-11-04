#!/usr/bin/env bats
# @test-type: unit — the generated pipelines' handlers, and run-finished against a caged nudge
# @domain: pipelines
# @card: #4888 · owner: silas
# What Jeff sees: he is never the one who notices a run finished. A run sends one
# wake when it finishes, pass or fail, to the owner and whoever waits on it (the
# navigator); a step in the middle that passes sends nothing. #4887's land at
# 14:35 woke nobody — Jeff found it at 14:37.

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
PIPES="$ROOT/platform/pipelines"
RF="$ROOT/platform/scripts/run-finished"

# A top-level pipeline is one a role starts (no TRACE param: a child's parent
# owns its finish). The nightly keeps its own exit (werk-test notify).
toplevel() {
  for f in "$1"/*.yaml; do
    grep -q '^  - TRACE: ' "$f" && continue
    case "$(basename "$f")" in nightly.yaml) continue ;; esac
    echo "$f"
  done
}
handlers() { sed -n '/^handler_on:/,/^steps:/p' "$1"; }

# prints a problem line per pipeline whose finish wake is wrong
wake_problems() {
  for f in $(toplevel "$1"); do
    h="$(handlers "$f")"
    n="$(printf '%s\n' "$h" | grep -c 'platform/scripts/run-finished')"
    [ "$n" -eq 1 ] || echo "$(basename "$f"): $n run-finished calls"
    printf '%s\n' "$h" | grep -q '^  exit:' || echo "$(basename "$f"): no exit handler"
    printf '%s\n' "$h" | sed -n '/^  failure:/,/^  [a-z]*:/p' | grep -q 'ops-nudge' && echo "$(basename "$f"): failure handler still nudges"
  done
  return 0
}

@test "AC1/AC2: every top-level pipeline wakes once from its exit handler, through run-finished; its failure handler only routes" {
  out="$(wake_problems "$PIPES")"
  [ -z "$out" ] || { echo "$out"; return 1; }
  # the card's five (and the go) by name, so a misread heuristic can never drop one silently
  for p in pair cicd cicd-go cicd-land athena domain-flow; do
    toplevel "$PIPES" | grep -q "/$p.yaml$" || { echo "$p.yaml is not checked as a top-level pipeline"; return 1; }
  done
}

@test "children stay silent: demo and cicd-test have no wake of their own (the parent's exit is the one)" {
  for c in demo cicd-test; do
    h="$(handlers "$PIPES/$c.yaml")"
    if printf '%s\n' "$h" | grep -qE 'run-finished|ops-nudge'; then echo "$c: $h"; return 1; fi
  done
}

@test "a failing child fails its parent, so the parent's exit wakes once with failed (dagu fixture)" {
  d="$BATS_TEST_TMPDIR/dg"; mkdir -p "$d/home"
  printf 'params:\n  - TRACE: ""\nsteps:\n  - name: boom\n    command: "false"\n' > "$d/child.yaml"
  printf 'handler_on:\n  exit:\n    command: sh -c "echo exit:$DAG_RUN_STATUS >> %s/wakes"\nsteps:\n  - name: call-child\n    command: env DAGU_HOME=%s/home dagu start %s/child.yaml -- TRACE=t\n  - name: after\n    depends: [call-child]\n    command: "true"\n' "$d" "$d" "$d" > "$d/parent.yaml"
  (cd "$d" && DAGU_HOME="$d/home" dagu start parent.yaml >/dev/null 2>&1) || true
  [ "$(cat "$d/wakes")" = "exit:failed" ] || { cat "$d/wakes"; return 1; }
}

@test "AC4 negative proof: a pipeline with no exit wake, or one that still nudges on failure, is named" {
  d="$BATS_TEST_TMPDIR/pipes"; mkdir -p "$d"
  printf 'handler_on:\n  failure:\n    command: ops-nudge ${ROLE} x\nsteps:\n' > "$d/old.yaml"
  printf 'handler_on:\n  exit:\n    command: "${CHORUS_HOME}/platform/scripts/run-finished ${CARD} ${ROLE} ${DAG_NAME} ${DAG_RUN_ID}"\nsteps:\n' > "$d/good.yaml"
  printf 'params:\n  - TRACE: ""\nhandler_on:\nsteps:\n' > "$d/child.yaml"
  out="$(wake_problems "$d")"
  [[ "$out" == *"old.yaml: 0 run-finished calls"* ]] || { echo "$out"; return 1; }
  [[ "$out" == *"old.yaml: failure handler still nudges"* ]] || { echo "$out"; return 1; }
  [[ "$out" != *"good.yaml"* && "$out" != *"child.yaml"* ]] || { echo "$out"; return 1; }
}

# --- run-finished, with chorus-hook-shim and chorus-werk-status caged ---
cage() {
  H="$BATS_TEST_TMPDIR/home"; mkdir -p "$H/ops/active-pairs"
  SENT="$BATS_TEST_TMPDIR/sent"; : > "$SENT"
  printf '#!/bin/sh\n[ "$1" = nudge-send ] && printf "%%s|%%s\\n" "$3" "$4" >> %s\n' "$SENT" > "$BATS_TEST_TMPDIR/shim"
  printf '#!/bin/sh\necho "failed at test-pipeline: werk-test: RED (exit 1)"\n' > "$BATS_TEST_TMPDIR/why"
  chmod +x "$BATS_TEST_TMPDIR/shim" "$BATS_TEST_TMPDIR/why"
}
finish() { # STATUS DAG [STEP]
  env -u CHORUS_ROLE CHORUS_HOME="$H" CHORUS_HOOK_SHIM="$BATS_TEST_TMPDIR/shim" RUN_FINISHED_WHY="$BATS_TEST_TMPDIR/why" \
    DAG_RUN_STATUS="$1" DAG_RUN_STEP_NAME="${3:-onExit}" bash "$RF" 4888 silas "$2" run-1
}

@test "AC1: a failed run wakes the owner once, naming the step and the reason" {
  cage; run finish failed cicd
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [ "$(wc -l < "$SENT" | tr -d ' ')" -eq 1 ] || { cat "$SENT"; return 1; }
  grep -q '^silas|.*#4888 cicd failed at test-pipeline: werk-test: RED' "$SENT" || { cat "$SENT"; return 1; }
}

@test "AC1: a paired card's finish wakes the driver and the navigator, each once" {
  cage; printf '{"card":"4888","driver":"silas","navigator":"kade"}' > "$H/ops/active-pairs/4888.json"
  run finish failed pair
  [ "$(cut -d'|' -f1 "$SENT" | sort | tr '\n' ' ')" = "kade silas " ] || { cat "$SENT"; return 1; }
}

@test "AC3: a land that succeeds wakes with 'landed' and names the post-land step" {
  cage; run finish succeeded cicd-land
  [ "$(wc -l < "$SENT" | tr -d ' ')" -eq 1 ] || { cat "$SENT"; return 1; }
  grep -q '^silas|#4888 landed, post-land: ' "$SENT" || { cat "$SENT"; return 1; }
}

@test "AC1: a passing athena, pair or domain-flow run wakes once with its result" {
  for dag in athena pair domain-flow; do
    cage; run finish succeeded "$dag"
    [ "$(wc -l < "$SENT" | tr -d ' ')" -eq 1 ] || { echo "$dag:"; cat "$SENT"; return 1; }
    grep -q "#4888 $dag passed" "$SENT" || { echo "$dag:"; cat "$SENT"; return 1; }
  done
}

@test "cicd parks at the go: its pass is the demo brief, so run-finished sends nothing for it" {
  cage; run finish succeeded cicd
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [[ "$output" == *"demo brief"* ]] || { echo "$output"; return 1; }
  [ ! -s "$SENT" ] || { cat "$SENT"; return 1; }
}

@test "AC4 negative proof: called from a step in the middle, run-finished sends nothing and says so" {
  cage; run finish succeeded cicd-land skill-werk-merge
  [ "$status" -ne 0 ] || return 1
  [ ! -s "$SENT" ] || { cat "$SENT"; return 1; }
  [[ "$output" == *"not an exit handler"* ]] || { echo "$output"; return 1; }
}

@test "AC2: run-finished sends through ops-nudge and nothing else" {
  out="$(grep -nE 'nudge-send|curl|clearing-nudge|/api/nudge|osascript' "$RF" || true)"
  [ -z "$out" ] || { echo "$out"; return 1; }
  [ "$(grep -c 'S/ops-nudge' "$RF")" -eq 1 ] || { grep -n ops-nudge "$RF"; return 1; }
}

@test "a passed go is silent (its land wakes); a failed go wakes" {
  cage; run finish succeeded cicd-go
  { [ "$status" -eq 0 ] && [ ! -s "$SENT" ]; } || { cat "$SENT"; return 1; }
  cage; run finish failed cicd-go
  grep -q '^silas|#4888 cicd-go failed at' "$SENT" || { cat "$SENT"; return 1; }
}

@test "AC6: pair red routes back to slice without its own nudge (the exit handler is the one wake)" {
  out="$(sed -n '/^red)/,/^  ;;/p' "$ROOT/platform/scripts/pair" | grep -nE '^\s*(nudge|.*ops-nudge) ' || true)"
  [ -z "$out" ] || { echo "pair red still nudges: $out"; return 1; }
}
