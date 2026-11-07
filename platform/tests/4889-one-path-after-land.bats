#!/usr/bin/env bats
# @test-type: unit — AC2's check run against main's workflows, now that #4889 is on main
# @domain: pipelines
# @card: #4889 · owner: silas
# Land 11-05 (Kade 20:32): AC2's negative proof read origin/main for the old layers, so it
# went red on every run once #4889 landed. platform/tests/4889-one-path.bats now plants the
# old layers instead; this check runs that file against origin/main's workflows.
ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"

# origin/main's five workflows into $1
main_pipelines() {
  mkdir -p "$1"
  for f in pair cicd demo cicd-go cicd-land; do git -C "$ROOT" show "origin/main:platform/pipelines/$f.yaml" > "$1/$f.yaml" || return 1; done
}

@test "AC2's check stays green on main's workflows after the land" {
  d="$BATS_TEST_TMPDIR/main"; main_pipelines "$d" || return 1
  run env PIPELINES="$d" bats "$ROOT/platform/tests/4889-one-path.bats"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
}

@test "negative proof: main's workflows with an old layer planted back fail AC2's check" {
  d="$BATS_TEST_TMPDIR/planted"; main_pipelines "$d" || return 1
  printf '  - name: skill-werk-demo-gate-ops\n' >> "$d/demo.yaml"
  run env PIPELINES="$d" bats "$ROOT/platform/tests/4889-one-path.bats"
  [ "$status" -ne 0 ] || { echo "$output"; return 1; }
}
