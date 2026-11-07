#!/usr/bin/env bats
# @test-type: unit — the architecture and quality review reads the diff, on every card, never the AC boxes
# @domain: pipelines
# @card: #4889 · owner: silas
# Reads skills/gate-arch/SKILL.md and skills/gate-quality/SKILL.md.
ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"

# prints why a review skill file breaks AC5, nothing when it holds
skill_problems() {
  grep -qiE 'type:fix.*SKIP|SKIP.*fix cards|not applicable for bug fix' "$1" && echo "skips fix cards"
  grep -qiE 'AC (items|boxes) (checked|ticked)|all AC|\[x\]' "$1" && echo "reads AC boxes"
  grep -qi 'diff' "$1" || echo "does not read the diff"
  return 0
}

@test "AC5: /gate-arch and /gate-quality review the diff on every card, and neither reads AC boxes" {
  for g in arch quality; do
    out="$(skill_problems "$ROOT/skills/gate-$g/SKILL.md")"
    [ -z "$out" ] || { echo "gate-$g: $out"; return 1; }
  done
}

@test "negative proof: a skill that skips fix cards, counts AC boxes, or never reads the diff is caught" {
  f="$BATS_TEST_TMPDIR/SKILL.md"
  printf 'Reviews the diff.\ntype:fix cards: SKIP this gate\n' > "$f"
  [[ "$(skill_problems "$f")" == *"skips fix cards"* ]] || return 1
  printf 'Reviews the diff.\nPass when all AC are ticked.\n' > "$f"
  [[ "$(skill_problems "$f")" == *"reads AC boxes"* ]] || return 1
  printf 'Reviews the card text only.\n' > "$f"
  [[ "$(skill_problems "$f")" == *"does not read the diff"* ]] || return 1
}

# the quality review's Semgrep step over one file: each finding as "<rule>:<severity>"
quality_scan() {
  [ -n "${CHORUS_TOOL_SEMGREP:-}" ] || { echo "no semgrep: ${CHORUS_TOOL_SEMGREP_ERROR:-CHORUS_TOOL_SEMGREP is not set (werk-test sets it from the tool.path.semgrep property)}" >&2; return 1; }
  SSL_CERT_FILE=/etc/ssl/cert.pem "$CHORUS_TOOL_SEMGREP" scan --config "$ROOT/platform/quality/semgrep/quality.yml" "$1" --json --metrics=off --disable-version-check 2>/dev/null \
    | python3 -c 'import json,sys; d=json.load(sys.stdin); print(" ".join(sorted(r["check_id"].split(".")[-1]+":"+r["extra"]["severity"] for r in d["results"])))'
}

@test "negative proof, run through the review: a planted hollow check is flagged as a failure, and the fixed check is not" {
  d="$BATS_TEST_TMPDIR/planted"; mkdir -p "$d"
  printf '@test "x" {\n  run true\n  ! false\n}\n' > "$d/hollow.bats"
  out="$(quality_scan "$d/hollow.bats")" || return 1
  [ "$out" = "bats-negation-cannot-fail:ERROR" ] || { echo "planted hollow check: $out"; return 1; }
  printf '@test "x" {\n  run true\n  ! false || return 1\n}\n' > "$d/fixed.bats"
  out="$(quality_scan "$d/fixed.bats")" || return 1
  [ -z "$out" ] || { echo "fixed check still flagged: $out"; return 1; }
}
