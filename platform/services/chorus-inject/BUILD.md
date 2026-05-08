# Building chorus-inject

**Do not use `cargo build --release` directly.** Use:

```
../../scripts/build-signed.sh chorus-inject
```

## Why

Same rationale as `chorus-hooks/BUILD.md` — `cargo build --release` produces a rebuild-dependent codesign identifier, which TCC can re-evaluate on each rebuild. `build-signed.sh` pins the identifier to `com.chorus.inject` so Accessibility grants persist across rebuilds.

## Verification

After `build-signed.sh` completes, the deployed binary lives at `~/.chorus/bin/chorus-inject` (per #2734 — split build artifact from deploy artifact so cdhash + TCC grants survive rebuilds). Verify the installed binary's signing identity:

```
codesign -dvvv ~/.chorus/bin/chorus-inject | grep Identifier
# → Identifier=com.chorus.inject
```

`target/release/chorus-inject` is the build artifact and is intentionally NOT referenced in operational paths — `test-hardcoded-bin-paths.sh` is a regression guard that fails if any non-test file adds a `target/release/chorus-*` reference.
