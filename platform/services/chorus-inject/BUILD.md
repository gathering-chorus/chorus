# Building chorus-inject

**Do not use `cargo build --release` directly.** Use:

```
../../scripts/build-signed.sh chorus-inject
```

## Why

Same rationale as `chorus-hooks/BUILD.md` — `cargo build --release` produces a rebuild-dependent codesign identifier, which TCC can re-evaluate on each rebuild. `build-signed.sh` pins the identifier to `com.chorus.inject` so Accessibility grants persist across rebuilds.

## Verification

```
codesign -dvvv target/release/chorus-inject | grep Identifier
# → Identifier=com.chorus.inject
```
