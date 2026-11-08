#!/usr/bin/env bats
# @test-type: unit — role-boot-check against fixture ps output; the live run is the same script without ROLE_BOOT_PS
# @domain: identity
#
# #4929 AC4 — Jeff 2026-11-08 06:53: "there are only 5 of u and we cant normalize
# how u work". After a boot or a land, one check names each role's account and
# fails when any role runs as the launcher's account.

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  CHECK="$ROOT/platform/scripts/role-boot-check"
  T="$BATS_TEST_TMPDIR"
  export ROLE_BOOT_HOMES="$T/homes" ROLE_BOOT_PS="$T/ps.txt"
}

good_ps() {
  cat > "$T/ps.txt" <<'E'
chorus-silas /h/.local/bin/claude -c
chorus-wren /h/.local/bin/claude -c
chorus-kade /h/.local/bin/claude -c
chorus-abby-normal agy
chorus-borg-queen agy
root sudo -n /bin/launchctl asuser 502 /usr/bin/sudo -n -u chorus-silas -H bash -c claude -c
E
}

@test "#4929 all five in their own accounts: green, one line each" {
  good_ps
  run "$CHECK"
  [ "$status" -eq 0 ]
  [ "$(printf '%s\n' "$output" | grep -c 'own account')" -eq 5 ]
}

@test "#4929 NEGATIVE PROOF: the 11-08 boot (kade and borg-queen as jeffbridwell) is red and names both" {
  cat > "$T/ps.txt" <<'E'
chorus-silas /h/.local/bin/claude -c
chorus-wren /h/.local/bin/claude -c
jeffbridwell bash -lc cd '/h/CascadeProjects/chorus/roles/kade' && exec claude -c
jeffbridwell /h/.local/bin/claude -c
chorus-abby-normal agy
jeffbridwell bash -lc cd '/h/CascadeProjects/chorus/roles/borg-queen' && agy
jeffbridwell agy
E
  run "$CHECK"
  [ "$status" -eq 1 ]
  printf '%s\n' "$output" | grep -qF "kade  RUNS AS jeffbridwell"
  printf '%s\n' "$output" | grep -qF "borg-queen  RUNS AS jeffbridwell"
}

@test "#4929 a role with no session at all is red" {
  good_ps
  grep -v chorus-kade "$T/ps.txt" > "$T/ps2.txt"; mv "$T/ps2.txt" "$T/ps.txt"
  run "$CHECK"
  [ "$status" -eq 1 ]
  printf '%s\n' "$output" | grep -qF "kade  NO SESSION"
}
