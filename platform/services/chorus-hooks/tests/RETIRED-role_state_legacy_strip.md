# Retired: role_state_legacy_strip.rs (#4111, 2026-09-06)

`tests/role_state_legacy_strip.rs` guarded a read path that no longer exists,
and its assertion had become the negation of the product's intended behaviour.

The test was written for #2629: `role-state query` used to dump the parsed
`/tmp/claude-team-scan/<role>-declared.json`, so a legacy file with a `card`
field leaked that field to consumers. It asserted the query output contains no
`card`.

#4028 removed declared state entirely — "nothing declared, nothing stored".
`role-state query` now reads the DERIVED rows from chorus-api's
`GET /api/chorus/context/roles`, and those rows carry the card *on purpose*,
from the board:

```
$ chorus-hook-shim role-state query kade
{ "card": 4111, "derived_state": { "card": 4111, "state": "building" ... } }
```

So the test asks the product not to do the thing #4028 was built to do. It was
green only because it skips itself unless RUN_INTEGRATION is set — the nightly
sets it, which is why it read as a Kade red on 2026-09-05 while passing on every
desk.

Two things worth keeping from it:

- **A guard whose target is deleted must fail loudly, not pass vacuously.** This
  one did fail loudly. Retirement is the answer, not a widened assertion.
- **It did not bring its own world (#3528).** It wrote the REAL
  `/tmp/claude-team-scan/kade-declared.json`, clobbering live role state during
  a run. Its own skip comment said "races team-scan" and it shipped anyway. If a
  future test needs that directory, `SCAN_DIR` in `src/shared/state_paths.rs`
  needs a world-seam first — the shape `chorus_log_file()` already has for the
  spine (#3615).
