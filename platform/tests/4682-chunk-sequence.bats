#!/usr/bin/env bats
# @test-type: unit
# @domain: board
# 4682-chunk-sequence.bats — prioritize and sequence cards for a chunk via product-radius (#4682)

setup() {
  export DIR="$BATS_TEST_TMPDIR"
  export SCRIPT="${BATS_TEST_DIRNAME}/../scripts/clearing-chunk"
}

@test "clearing-chunk prioritize lists open cards whose sequence matches chunk products in board order" {
  cat << 'STUB_CARDS' > "$DIR/fake-cards"
#!/usr/bin/env bash
if [ "$1" = "list" ]; then
  cat << 'BOARD'
WIP:
Next:
  101 Card One [Wren|P1]
  102 Card Two [Silas|P2]
  103 Card Three [Silas|P1]
BOARD
elif [ "$1" = "view" ]; then
  case "$2" in
    101) echo "Domains: sequence:borg" ;;
    102) echo "Domains: sequence:clearing" ;;
    103) echo "Domains: sequence:loom" ;;
  esac
fi
STUB_CARDS
  chmod +x "$DIR/fake-cards"
  export CARDS_CLI="$DIR/fake-cards"
  export CHUNK_PRODUCTS_alerts_decisions="borg loom"

  run "$SCRIPT" prioritize alerts-decisions
  [ "$status" -eq 0 ]
  [[ "$output" =~ "101" ]] || return 1
  [[ "$output" =~ "103" ]] || return 1
  ! [[ "$output" =~ "102" ]] || return 1

  pos101=$(echo "$output" | grep -n "101" | cut -d: -f1)
  pos103=$(echo "$output" | grep -n "103" | cut -d: -f1)
  [ "$pos101" -lt "$pos103" ]
}

@test "clearing-chunk sequence selects the first open card in chunk radius and excludes third product (#3734 negative proof)" {
  # 3 cards across 3 products:
  # 101: product 'clearing' (outside alerts-decisions radius) -> must NOT be picked
  # 102: product 'borg' (inside alerts-decisions radius) -> MUST be picked
  # 103: product 'loom' (inside alerts-decisions radius, but after 102)
  cat << 'STUB_CARDS' > "$DIR/fake-cards"
#!/usr/bin/env bash
if [ "$1" = "list" ]; then
  cat << 'BOARD'
Next:
  101 Card from Third Product [Wren|P1]
  102 First Matching Card [Silas|P1]
  103 Second Matching Card [Silas|P2]
BOARD
elif [ "$1" = "view" ]; then
  case "$2" in
    101) echo "Domains: sequence:clearing" ;;
    102) echo "Domains: sequence:borg" ;;
    103) echo "Domains: sequence:loom" ;;
  esac
fi
STUB_CARDS
  chmod +x "$DIR/fake-cards"
  export CARDS_CLI="$DIR/fake-cards"
  export CHUNK_PRODUCTS_alerts_decisions="borg loom"

  run "$SCRIPT" sequence alerts-decisions
  [ "$status" -eq 0 ]
  [ "$output" = "102" ]
  [ "$output" != "101" ]
}
