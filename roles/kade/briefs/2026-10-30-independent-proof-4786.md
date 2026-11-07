# Brief: Independent Proof of #4786 in Prod

- **From**: Borg Queen
- **To**: Kade
- **Date**: 2026-10-30 10:06 Boston
- **Card**: #4786 (*Per-role macOS accounts (Slice 3): Presence hostAccount & cleanup, nudge-health, and bouncer approval*)
- **Status**: Done (commit `1edee733d`)

---

## 1. Request
Jeff directed: `@borg-queen ask kade to independently prove`.
Please independently prove card #4786 in prod and verify all 4 ACs pass under your own role session.

## 2. Context & Measured Evidence
- **Card**: #4786
- **Test Suite**: `platform/tests/4780-slice3-presence-nudge.bats` (16 tests, 0 failures, covering AC1–AC4 and 7 negative proofs).
- **Prover Run**: `borg-prove prod 4786 borg-queen` reported `Proven`:
  - `ac-4786-1`: `met` — AC1: nudge-health finds each role's window and every nudge/reply shows where it was delivered (`test 4780-slice3-presence-nudge.bats → pass`).
  - `ac-4786-2`: `met` — AC2: Nudge and pulse agree on whether a role is online (`test 4780-slice3-presence-nudge.bats → pass`).
  - `ac-4786-3`: `met` — AC3: Presence records real hostAccount, retires on run end, and cleans dangling deliveredTo (`test 4780-slice3-presence-nudge.bats → pass`).
  - `ac-4786-4`: `met` — AC4: Bouncer approval hook fires on Jeff's approve in chorus-<role> (`test 4780-slice3-presence-nudge.bats → pass`).
- **Commits on `main`**:
  - `d6938a3b4`: #4786 slice 3 changes.
  - `1edee733d`: Fix `nudge-health-check.sh` `MESSAGES_DB` fallback for test isolation, `borg-prove` LogQL path regex match, and Loki curl HTTP 429 retry.

## 3. How to Verify
1. Pull latest `main`: `git pull --ff-only`
2. Run the test suite:
   ```bash
   CARGO_TARGET_DIR=/tmp/cargo-target-kade bats platform/tests/4780-slice3-presence-nudge.bats
   ```
3. Run prod prover:
   ```bash
   borg-prove prod 4786 kade
   ```

## 4. Response Needed
Confirm whether `borg-prove prod 4786` passes independently in your environment and note the verdict to Jeff.
