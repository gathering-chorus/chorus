#!/usr/bin/env bats
# @test-type: unit — the 30-day proof-layer inventory is in the repo and carries what each layer caught
# @domain: pipelines
# @card: #4889 · owner: silas
ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
INV="$ROOT/designing/docs/proof-layers-4889.md"

@test "AC1: the inventory names every layer with its owner, runs and what it caught alone, and ends on Jeff's path" {
  [ -f "$INV" ] || { echo "no inventory at $INV"; return 1; }
  [ "$(grep -cE '^\| [0-9]+ \| ' "$INV")" -ge 11 ] || return 1
  grep -q 'Alone, real defect' "$INV" || return 1
  grep -q 'one test run · one code review · one architecture and quality review · prove in prod' "$INV" || return 1
}

@test "AC1 negative proof: an inventory missing its per-layer numbers is refused by the same check" {
  printf '# Proof layers\n| 1 | Pair green | navigator |\n' > "$BATS_TEST_TMPDIR/inv.md"
  ! grep -q 'Alone, real defect' "$BATS_TEST_TMPDIR/inv.md" || return 1
}
