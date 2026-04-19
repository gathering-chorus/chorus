# Building chorus-hooks

**Do not use `cargo build --release` directly.** Use:

```
../../scripts/build-signed.sh chorus-hooks
```

## Why

`cargo build --release` emits an ad-hoc-signed Mach-O with a rebuild-dependent identifier (e.g., `chorus_hook_shim-<hash>`). macOS TCC treats each distinct identifier as a different binary and re-evaluates Accessibility grants on every rebuild, occasionally flipping them OFF — which silently breaks `nudge` keystroke injection.

`build-signed.sh` runs `cargo build --release` then re-codesigns the binary with the stable identifier `com.chorus.hook-shim`. TCC now sees the same identity across rebuilds.

## What this doesn't fix

This addresses rebuild-triggered TCC re-evaluation only. Overnight macOS security re-validation (XProtect scans, TCC cache refresh) can still flip Accessibility grants independently of any rebuild — see RCA #114. If nudge breaks without a rebuild, this script is not the cause or the cure.

## Verification

```
codesign -dvvv target/release/chorus-hook-shim | grep Identifier
# → Identifier=com.chorus.hook-shim
```

If you see `Identifier=chorus_hook_shim-<hash>` instead, bare `cargo build` was used — re-run `build-signed.sh` to restore the stable identifier.
