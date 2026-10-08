#!/usr/bin/env bats
# @domain: tests
# @test-type: unit
# #4459 — claudemd-gen leaves a generated file alone when the only change would
# be the header timestamp. On 2026-10-08 that one-line rewrite dirtied a werk
# mid-run (roles/abby-normal/AGENTS.md) and Jeff's go on #4454 was refused.
# Covers: platform/scripts/claudemd-gen.py

setup() {
  ROOT="${BATS_TEST_DIRNAME}/../.."
  GEN="$ROOT/platform/scripts/claudemd-gen.py"
  CM="$BATS_TEST_TMPDIR/cm"
  mkdir -p "$CM/out"
  printf '# Hello {{NAME}}\n' > "$CM/frag.md"
  printf '{"_build":"7","variables":{"r":{"NAME":"r"}},"roles":{"r":{"output":"out/R.md","sections":["frag.md"]}}}\n' > "$CM/manifest.json"
  OUT="$CM/out/R.md"
  OLD_STAMP='2001-01-01 00:00'
}

gen() {
  python3 "${1:-$GEN}" "$CM/manifest.json" "$CM" generate >/dev/null 2>&1
}

# Put yesterday's stamp on the header, as a file generated earlier would carry.
age_header() {
  sed -i '' "1s/| [0-9-]* [0-9:]* |/| $OLD_STAMP |/" "$OUT"
  grep -q "$OLD_STAMP" "$OUT" || return 1
}

@test "an unchanged generate leaves the file byte-for-byte as it was" {
  gen
  age_header
  before=$(cat "$OUT")
  gen
  [ "$(cat "$OUT")" = "$before" ]
}

@test "a real fragment change still rewrites the file with a new stamp" {
  gen
  age_header
  printf '# Hello again {{NAME}}\n' > "$CM/frag.md"
  gen
  grep -q 'Hello again r' "$OUT" || return 1
  run grep -c "$OLD_STAMP" "$OUT"
  [ "$output" = "0" ]
}

@test "NEGATIVE PROOF: the old unconditional write rewrites the stamp, so the first test can fail" {
  old="$BATS_TEST_TMPDIR/claudemd-gen-old.py"
  # The write path before #4459: every generate rewrote the file.
  sed -e 's/^\( *\)write_generated(\([a-z_]*\), \([a-z_]*\))$/\1open(\2, "w").write(\3)/' "$GEN" > "$old"
  run grep -c 'write_generated(output_path' "$old"
  [ "$output" = "0" ]
  gen "$old"
  age_header
  before=$(cat "$OUT")
  gen "$old"
  [ "$(cat "$OUT")" != "$before" ]
}
