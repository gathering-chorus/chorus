# /demo — Runbook

**Owner:** Wren
**Product:** Demo proving gate

## Health Check

```bash
# Test preflight on a WIP card
bash platform/skills/demo/gates/preflight.sh <wip-card-id>
# Should exit 0
```

## Test

```bash
cd $CHORUS_ROOT && bats platform/skills/demo/tests/gates.bats
```

Expected: 6 tests, 6 pass.

## Common Failures

**"not found on board"** — cards CLI can't reach Vikunja, or card ID doesn't exist. Check `.env` for the Vikunja token, check `cards list --limit 1`.

**"no acceptance criteria"** — card description missing `## AC` section. Add it to the card.

**"smoke check failed"** — a page is down. Run `smoke-check.sh --all` to see which.

**False positive on demo evidence** — stale briefs from pre-DB-rebuild card numbers can match. Check the brief date vs card creation date.

## Gate Script Locations

```
skills/demo/gates/
  preflight.sh    — pre-demo validation
  done-gate.sh    — demo evidence before Done
  provenance.sh   — brief generation + spine event
```

## Rebuild

Gate scripts are shell — no build step. If chorus-hooks dispatch needs rebuilding:
```bash
cd platform/services/chorus-hooks && cargo build --release
launchctl kickstart -k user/501/com.chorus.hooks
```
