# @domain: tests
# #4915 (#4555 follow-on, Silas 11-21 10:51) — where a built binary is, now that the repo is one
# cargo workspace. A pipeline build sets CARGO_TARGET_DIR to target/<account>; a hand build in a
# checkout lands in target/. Of the places that hold the binary, answer the NEWEST (slice 3, the
# code review: first-in-line could answer an older build while the fresh one sat further down).
# When none holds it, answer the pipeline's place, so the caller fails naming a real path.
#   workspace_bin <repo root> <debug|release|any> <binary>     (any: either profile, newest wins)
workspace_bin() {
  local root="$1" profile="$2" bin="$3" d p best="" c
  local profiles="$profile"; [ "$profile" = any ] && profiles="debug release"
  for d in "${CARGO_TARGET_DIR:-}" "$root/target/$(id -un)" "$root/target"; do
    [ -n "$d" ] || continue
    for p in $profiles; do
      c="$d/$p/$bin"
      [ -x "$c" ] || continue
      if [ -z "$best" ] || [ "$c" -nt "$best" ]; then best="$c"; fi
    done
  done
  [ -n "$best" ] && { printf '%s\n' "$best"; return 0; }
  [ "$profile" = any ] && profile=debug
  printf '%s\n' "${CARGO_TARGET_DIR:-$root/target/$(id -un)}/$profile/$bin"
}

# A test that needs a binary and may run where nobody built it: the newest build in either
# profile, else build the package once (wherever cargo puts it) and ask again. A failed build
# answers nothing, so the caller's [ -x ] fails naming the miss.
#   workspace_bin_built <repo root> <binary> <cargo package>
workspace_bin_built() {
  local root="$1" bin="$2" pkg="$3" b
  b="$(workspace_bin "$root" any "$bin")"
  [ -x "$b" ] && { printf '%s\n' "$b"; return 0; }
  (cd "$root" && cargo build -q -p "$pkg") >&2 || return 1
  workspace_bin "$root" any "$bin"
}
