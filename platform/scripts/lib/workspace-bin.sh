# @domain: tests
# #4915 (#4555 follow-on) — where a built binary is, now that the repo is one cargo workspace.
# A pipeline build sets CARGO_TARGET_DIR to target/<account>; a hand build lands in target/.
#   workspace_bin <repo root> <debug|release|any> <binary>     (any: either profile)
# An explicit CARGO_TARGET_DIR that holds the binary is the answer: the caller named it (round 5,
# code review). Otherwise the NEWEST of target/<account> and target/ (slice 3: first-in-line
# could answer a stale build). None: the pipeline's place, so the caller fails naming a real path.
workspace_bin() {
  local root="$1" profile="$2" bin="$3" d p best="" c
  local profiles="$profile"; [ "$profile" = any ] && profiles="debug release"
  if [ -n "${CARGO_TARGET_DIR:-}" ]; then
    for p in $profiles; do
      c="$CARGO_TARGET_DIR/$p/$bin"
      [ -x "$c" ] && { [ -z "$best" ] || [ "$c" -nt "$best" ]; } && best="$c"
    done
    [ -n "$best" ] && { printf '%s\n' "$best"; return 0; }
  fi
  for d in "$root/target/$(id -un)" "$root/target"; do
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

# A test that needs a binary and may run where nobody built it: an existing build of the asked
# profile (any: either), else build the package once into the account's folder (CARGO_TARGET_DIR,
# else target/<account>; never the shared target/) and ask again. A failed build answers nothing.
#   workspace_bin_built <repo root> <binary> <cargo package> [debug|release|any]   (default any)
workspace_bin_built() {
  local root="$1" bin="$2" pkg="$3" profile="${4:-any}" b tgt flag=""
  b="$(workspace_bin "$root" "$profile" "$bin")"
  [ -x "$b" ] && { printf '%s\n' "$b"; return 0; }
  tgt="${CARGO_TARGET_DIR:-$root/target/$(id -un)}"
  [ "$profile" = release ] && flag="--release"
  (cd "$root" && CARGO_TARGET_DIR="$tgt" cargo build -q $flag -p "$pkg") >&2 || return 1
  CARGO_TARGET_DIR="$tgt" workspace_bin "$root" "$profile" "$bin"
}
