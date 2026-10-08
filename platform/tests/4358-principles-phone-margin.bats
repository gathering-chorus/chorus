#!/usr/bin/env bats
# @test-type: unit — reads the page's own stylesheet from disk; no browser, no service.
# @domain: principles — the product domain this suite guards (#4334)
#
# #4358 — Jeff, 2026-10-08 09:29: "the principles page renders with margin on
# phone". The body kept a 1-inch side padding at every width, so on a 390px
# phone the text had about half the screen. A phone-width rule cuts the side
# margin to 16px; desktop and print keep the inch. Checked in a browser at 390px
# and 1280px during the build (16px and 96px).

ROOT="$BATS_TEST_DIRNAME/../.."
PAGE="$ROOT/platform/api/public/loom/principles.html"

# The side padding the phone rule sets, in px, or empty when there is no phone rule.
phone_side_padding() {
  awk '/@media screen and \(max-width: [0-9]+px\)/{m=1} m&&/body *\{/{print; exit}' "$1" \
    | sed -nE 's/.*padding: *[^ ;]+ +([0-9.]+)px.*/\1/p'
}

@test "a phone (600px or less) gets a side margin of 24px or less" {
  run phone_side_padding "$PAGE"
  [ -n "$output" ] || return 1
  [ "${output%.*}" -le 24 ] || return 1
}

@test "desktop keeps the 1-inch side padding" {
  grep -qE '^ +padding: 3rem 1in 6rem 1in;' "$PAGE" || return 1
}

@test "NEGATIVE PROOF: the page without the phone rule has no phone margin" {
  old="$BATS_TEST_TMPDIR/principles-old.html"
  sed '/@media screen and (max-width: 600px)/,/^    }$/d' "$PAGE" > "$old"
  ! cmp -s "$old" "$PAGE" || return 1
  run phone_side_padding "$old"
  [ -z "$output" ] || return 1
}
