#!/usr/bin/env bats
# @test-type: unit — static repo grep, no live service
# 3370-no-new-hardcoded-lan-ips.bats — regression guard (#3370)
# What Jeff sees: a machine can change address (DHCP drift, hub unplugged —
# the 2026-06-12 incident) and nothing goes silently dark, because no NEW
# code pins a 192.168.86.x address. Legacy pins are baselined and may only
# SHRINK (the test-hardcoded-bin-paths.sh ratchet pattern).

REPO="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
# #3904/#3915 — scan governs CODE. Captured data (.nt graph dumps, .backup and
# LIVE .db/.sqlite files — messages.db is a SQLite binary whose PAGES happen to
# contain message text with addresses) and prose/rendered surfaces
# (.md/.html/.ejs) may legitimately CONTAIN addresses;
# pinning behavior lives in scripts/services, which stay fully governed.
BASELINE="$REPO/platform/tests/3370-lan-ip-baseline.txt"

# #4111 — a COMMENT is not a pin. The header above already grants that prose
# surfaces may carry addresses; a comment inside a .ts is the same prose one
# level down. cors-origin.ts tripped this today on a single line explaining
# WHY the code uses a range test instead of pinning an address — the guard
# flagged the sentence describing the behaviour it wants. Counting only
# non-comment lines is the conservative direction: an address in a string on a
# code line still counts.
lan_hits() {
  grep -E "192\.168\.86\." "$1" 2>/dev/null \
    | grep -cvE '^[[:space:]]*(//|#|\*|--|/\*)' || true
}

@test "no NEW hardcoded 192.168.86.x references (shrink-only ratchet)" {
  [ -f "$BASELINE" ]
  fails=""
  while IFS=: read -r file allowed; do
    [ -z "$file" ] && continue
    case "$file" in \#*) continue ;; esac          # comment lines in the baseline
    allowed="${allowed%%[!0-9]*}"                    # strip trailing inline comment
    [ -n "$allowed" ] || continue
    actual=$(lan_hits "$REPO/$file")
    if [ "$actual" -gt "$allowed" ]; then
      fails="$fails $file($actual>$allowed)"
    fi
  done < "$BASELINE"
  # any file NOT in the baseline must have zero references
  while IFS= read -r hit; do
    grep -q "^${hit}:" "$BASELINE" && continue
    [ "$(lan_hits "$REPO/$hit")" -gt 0 ] && fails="$fails NEW:$hit"
  done < <(grep -rl "192\.168\.86\." "$REPO/platform" "$REPO/proving" "$REPO/designing" "$REPO/.github" 2>/dev/null \
    | grep -vE "\.git|node_modules|/dist|/coverage|/logs/|/target/|board-snapshot|baseline" \
    | grep -vE "/backups/|\.nt$|\.backup$|\.md$|\.html$|\.ejs$|\.db$|\.sqlite3?$" \
    | sed "s|^$REPO/||")
  [ -z "$fails" ] || { echo "ratchet violations:$fails"; false; }
}

@test "#3915 NEGATIVE PROOF: a NEW source pin still fails the ratchet" {
  # The exclusions above are for captured DATA. Prove they did not blind the
  # ratchet to the thing it exists to catch: a real script pinning the address.
  W="$BATS_TEST_TMPDIR/lan-neg"
  mkdir -p "$W/platform/scripts"
  printf '#!/usr/bin/env bash\ncurl http://192.168.86.242:3000/\n' > "$W/platform/scripts/new-pin.sh"
  run bash -c "grep -rl '192\.168\.86\.' '$W' | grep -vE '/backups/|\.nt$|\.backup$|\.md$|\.html$|\.ejs$|\.db$|\.sqlite3?$'"
  [ "$status" -eq 0 ]
  [[ "$output" == *"new-pin.sh"* ]]

  # and a captured .db in the same tree is correctly ignored
  printf 'binary-ish 192.168.86.242 payload\n' > "$W/platform/scripts/messages.db"
  run bash -c "grep -rl '192\.168\.86\.' '$W' | grep -vE '/backups/|\.nt$|\.backup$|\.md$|\.html$|\.ejs$|\.db$|\.sqlite3?$' | grep -c messages.db"
  [ "$output" = "0" ]
}
