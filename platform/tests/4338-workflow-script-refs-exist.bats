#!/usr/bin/env bats
# @test-type: contract
# @domain: pipelines — the land workflows this suite guards
# #4338 — a workflow step that calls a script which no longer exists must fail the
# commit, not the land. athena.yml's prove step called platform/scripts/athena-
# validate.sh for 12 days after #4167 deleted it: bash said "No such file", the
# count grep found nothing, pipefail ended the step with exit 1 and no message,
# and every land's model run read as failed with nobody told why.

ROOT="${BATS_TEST_DIRNAME}/../.."

missing_refs() {  # missing_refs <root> <workflow files...> → each referenced platform/scripts path that is absent
  local root="$1"; shift
  grep -ohE 'platform/scripts/[A-Za-z0-9._/-]+' "$@" | sort -u | while read -r p; do
    [ -e "$root/$p" ] || echo "$p"
  done
}

@test "every platform/scripts path the workflows call exists" {
  run missing_refs "$ROOT" "$ROOT"/.github/workflows/*.yml
  [ "$status" -eq 0 ] || false
  [ -z "$output" ] || { echo "workflows call scripts that do not exist: $output"; false; }
  # a guard whose input vanished must fail, never pass on nothing
  n=$(grep -ohE 'platform/scripts/[A-Za-z0-9._/-]+' "$ROOT"/.github/workflows/*.yml | sort -u | wc -l)
  [ "$n" -ge 5 ] || { echo "only $n script references found"; false; }
}

@test "NEGATIVE PROOF — the 09-20 shape (a deleted athena-validate.sh) is named" {
  wf="$BATS_TEST_TMPDIR/athena.yml"
  printf '%s\n' '      - run: bash "${CHORUS_HOME}/platform/scripts/athena-validate.sh"' > "$wf"
  run missing_refs "$ROOT" "$wf"
  [ "$output" = "platform/scripts/athena-validate.sh" ] || false
}
