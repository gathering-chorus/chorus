#!/usr/bin/env bats
# @test-type: fitness:security — hermetic: the door's allow list and shut list read as files, the door's own route() run on them; no server, no tunnel
# @domain: security
# @card: #5039 AC2 · written by wren (navigator), owner: silas
#
# Jeff, 2026-11-24: "i feel i must be able to see all pages via public url". Measured 11-25:
# 33 of 93 served pages were refused by the sign-in door even when signed in, and about 40
# data paths that public pages fetch were refused, so pages opened empty.
#
# What must hold (platform/scripts/share-door-inventory, reading config/share-allowlist.txt and
# config/share-shut.txt):
#   - every page under platform/api/public is let through the door;
#   - every /api or /designing path a let-through page names is let through, or is named shut
#     with its reason: nothing is refused by accident;
#   - a page added tomorrow that nobody listed makes this red (never a silent 404);
#   - the personal data stays shut.
# NEGATIVE PROOFS: the allow list as it was before #5039, a new unlisted page, an unlisted
# data path, a shut line with no reason and a shut line the allow list contradicts each fail,
# and each failure names the path.
#
# Named limit: this checks the lists against the served tree. That the door then answers 200
# to a signed-in person is the door's own suite (test-chorus-share.sh) and the phone proof.

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
CHECK="$ROOT/platform/scripts/share-door-inventory.py"
ALLOW="$ROOT/config/share-allowlist.txt"
SHUT="$ROOT/config/share-shut.txt"

setup() { T="$(mktemp -d)"; }
teardown() { rm -rf "$T"; }

# a small served tree: one page that names one data path
tree() {
  mkdir -p "$T/public/athena"
  printf '<script>fetch("/api/demo/read")</script>\n' > "$T/public/athena/a.html"
  printf '/athena\n/api/demo/read\n' > "$T/allow"
  printf '/api/private  personal\n' > "$T/shut"
}
check() { run python3 "$CHECK" --allowlist "${1:-$T/allow}" --shut "${2:-$T/shut}" --public "${3:-$T/public}"; }

@test "#5039 every served page opens behind the door, and every refused data path is named shut" {
  run python3 "$CHECK"
  echo "$output"
  test "$status" -eq 0
  echo "$output" | grep -q " 0 problem(s)"
}

@test "#5039 the personal data stays shut" {
  for p in /api/chorus/jeff/posture/strip /api/chorus/voice /api/chorus/sessions /api/chorus/cost/summary; do
    # each must be refused by the allow list: a fixture page naming it is reported only if no shut line covers it
    mkdir -p "$T/public/athena"; printf '<script>fetch("%s")</script>\n' "$p" > "$T/public/athena/p.html"
    run python3 "$CHECK" --public "$T/public"
    test "$status" -eq 0 || { echo "$p is not covered by a shut line: $output"; return 1; }
    printf '/zz-none placeholder\n' > "$T/noshut"
    run python3 "$CHECK" --public "$T/public" --shut "$T/noshut"
    echo "$output" | grep -q "^DATA $p " || { echo "$p is let through by the allow list: $output"; return 1; }
  done
}

@test "#5039 NEGATIVE: the allow list before #5039 fails, naming the blocked pages" {
  # the list as it was: everything from the #5039 block down cut off (the cut must remove lines)
  sed '/^# #5039/,$d' "$ALLOW" > "$T/before"
  test "$(wc -l < "$T/before")" -lt "$(wc -l < "$ALLOW")"
  run python3 "$CHECK" --allowlist "$T/before"
  test "$status" -eq 1
  echo "$output" | grep -q "^PAGE /business-plan.html "
  echo "$output" | grep -q "^PAGE /book/index.html "
  echo "$output" | grep -q "^DATA /api/chorus/policies/counts "
}

@test "#5039 NEGATIVE: a new page nobody listed is named, never a silent 404" {
  tree; check; test "$status" -eq 0
  printf '<p>new</p>\n' > "$T/public/brand-new-5039.html"
  check
  test "$status" -eq 1
  echo "$output" | grep -q "^PAGE /brand-new-5039.html "
}

@test "#5039 NEGATIVE: a data path a public page names, neither open nor shut, is named" {
  tree
  printf '<script>fetch("/api/demo/unlisted")</script>\n' > "$T/public/athena/b.html"
  check
  test "$status" -eq 1
  echo "$output" | grep -q "^DATA /api/demo/unlisted — named by /athena/b.html"
  # named shut with a reason, it passes: the refusal is then on purpose
  printf '/api/demo/unlisted  not reviewed\n' >> "$T/shut"
  check; test "$status" -eq 0
}

@test "#5039 NEGATIVE: a shut line with no reason, and one the allow list contradicts, both fail" {
  tree
  printf '/api/other\n' >> "$T/shut"
  check
  test "$status" -eq 1
  echo "$output" | grep -q "^SHUT-NO-REASON /api/other "
  tree
  printf '/api/demo/read  personal\n' >> "$T/shut"
  check
  test "$status" -eq 1
  echo "$output" | grep -q "^STALE /api/demo/read "
}

@test "#5039 NEGATIVE: a missing or empty list is fatal, never a pass" {
  tree
  check "$T/no-such-allow"; test "$status" -ne 0; echo "$output" | grep -q "cannot read the allow list"
  : > "$T/empty"; check "$T/allow" "$T/empty"; test "$status" -ne 0; echo "$output" | grep -q "has no entries"
  check "$T/allow" "$T/shut" "$T/no-such-tree"; test "$status" -ne 0; echo "$output" | grep -q "no served tree"
}
