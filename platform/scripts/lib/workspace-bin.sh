# @domain: tests
# #4915 (#4555 follow-on, Silas 11-21 10:51) — where a built binary is, now that the repo is one
# cargo workspace. A pipeline build sets CARGO_TARGET_DIR to target/<account>; a hand build in a
# checkout lands in target/. Ask in that order and answer the first that exists; when none does,
# answer the pipeline's place, so the caller fails naming a real path (never a silent skip here).
#   workspace_bin <repo root> <debug|release> <binary>
workspace_bin() {
  local root="$1" profile="$2" bin="$3" d
  for d in "${CARGO_TARGET_DIR:-}" "$root/target/$(id -un)" "$root/target"; do
    [ -n "$d" ] && [ -x "$d/$profile/$bin" ] && { printf '%s\n' "$d/$profile/$bin"; return 0; }
  done
  printf '%s\n' "${CARGO_TARGET_DIR:-$root/target/$(id -un)}/$profile/$bin"
}
