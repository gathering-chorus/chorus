#!/usr/bin/env bats
# @test-type: unit — reads the card's diff and a covers list; no live service
# @domain: tests
# @card: #4925 · owner: kade
# AC3: every test this round deletes names the test that still covers what it covered
# (Jeff 11-22, via Wren: "every DELETE names what the test covered"). A deleted test
# file must be listed in fixtures/4925-deleted-covers.tsv, and its cover must be a
# test that exists in the tree, or "none: <why>".

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
COVERS="$ROOT/platform/tests/fixtures/4925-deleted-covers.tsv"

# uncovered <deleted-list-file> <covers-tsv> <root>: one line per deleted test file
# with no entry, and per entry whose cover is neither "none: <why>" nor a file in <root>
uncovered() {
  python3 - "$1" "$2" "$3" <<'PY'
import os, re, sys
deleted = [l.strip() for l in open(sys.argv[1]) if l.strip()]
rows = {}
for l in open(sys.argv[2]):
    if l.startswith("#") or "\t" not in l:
        continue
    d, cover = l.rstrip("\n").split("\t", 1)
    rows[d] = cover.strip()
test_file = re.compile(r"(\.bats|\.test\.(ts|js|cjs|mjs|sh)|\.spec\.(cjs|mjs|ts|js)|_test\.rs|\.feature)$|/tests/[^/]+\.rs$")
for f in deleted:
    if test_file.search(f) and f not in rows:
        print(f"deleted with no named cover: {f}")
for d, cover in rows.items():
    if cover.startswith("none:") and cover[5:].strip():
        continue
    if not cover or not os.path.isfile(os.path.join(sys.argv[3], cover)):
        print(f"{d}: cover {cover!r} is not a test in the tree")
PY
}

@test "AC3: every test file this card deletes names the test that still covers it" {
  base="$(git -C "$ROOT" merge-base HEAD origin/main 2>/dev/null)" || { echo "no merge-base with origin/main"; return 1; }
  git -C "$ROOT" diff --diff-filter=D --name-only "$base" HEAD > "$BATS_TEST_TMPDIR/deleted"
  # the werk's own uncommitted deletes count too (a run before the commit)
  git -C "$ROOT" diff --diff-filter=D --name-only >> "$BATS_TEST_TMPDIR/deleted"
  out="$(uncovered "$BATS_TEST_TMPDIR/deleted" "$COVERS" "$ROOT")"
  [ -z "$out" ] || { echo "$out"; return 1; }
}

@test "AC3 NEGATIVE PROOF: a planted delete with no cover, and a cover that is not a test, are named" {
  printf 'platform/tests/planted-4925.bats\n' > "$BATS_TEST_TMPDIR/deleted"
  printf '# header\nplatform/tests/other.bats\tplatform/tests/does-not-exist.bats\nx.test.ts\tnone:\n' > "$BATS_TEST_TMPDIR/covers"
  out="$(uncovered "$BATS_TEST_TMPDIR/deleted" "$BATS_TEST_TMPDIR/covers" "$ROOT")"
  [[ "$out" == *"deleted with no named cover: platform/tests/planted-4925.bats"* ]] || { echo "$out"; return 1; }
  [[ "$out" == *"platform/tests/other.bats: cover 'platform/tests/does-not-exist.bats' is not a test"* ]] || { echo "$out"; return 1; }
  [[ "$out" == *"x.test.ts: cover 'none:'"* ]] || { echo "an empty 'none:' passed: $out"; return 1; }
  # control: a real cover and a reasoned none pass
  printf 'platform/tests/planted-4925.bats\tplatform/tests/4925-deletes-name-cover.bats\ny.test.ts\tnone: a stub\n' > "$BATS_TEST_TMPDIR/covers"
  [ -z "$(uncovered "$BATS_TEST_TMPDIR/deleted" "$BATS_TEST_TMPDIR/covers" "$ROOT")" ]
}
