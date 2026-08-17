# History disposition for the 35 baselined credential findings (#3386)

**Verdict: NO history rewrite required. All 35 are dead values.** Recorded
2026-08-17 with the commands that establish it, so the next person can re-run
the check instead of re-trusting this page.

## Why this card exists
`.gitleaks-baseline.json` carries 35 accepted findings. A baseline is an
*acknowledgement*, not a disposition: a clone still ships whatever is in
history. Before this repo can go public, each finding has to be one of
"rotated — the historical value is dead" or "still live — rewrite history."
That question had never been answered per finding.

## What the 35 actually are

| count | where | class |
|---|---|---|
| 29 | `platform/logs/permission-prompts.log`, `ops/logs/chorus.log`, `platform/logs/chorus.log` | log lines echoing a `curl -u admin:…` invocation |
| 4 | two archived copies of `2026-02-23-fuseki-401-storm-fix.md`, `seed-probe.sh`, `seed_steps.ts` | the same curl example, pasted into docs/scripts |
| 2 | two copies of `write_scrubber.rs` (pre/post the platform/ move) | the scrubber's own **test fixture** — a fake key it must detect |

Distinct secret values in the baseline: **1** (the file stores `REDACTED`; the
real string lives only in the historical blobs).

## The disposition

**1. The Fuseki admin credential (33 of 35).** Every occurrence is the
pre-#3611 literal `admin:admin` — Fuseki's stock default from when the store
ran open on the LAN. Verified in the oldest blobs
(`git show 0d8b73b0…:platform/logs/permission-prompts.log`), and no blob in the
file's 16-commit history carries a longer value. The live credential today is
20 characters, provisioned to `~/.gathering/data/fuseki-write.env` (0600) by
`fuseki-shiro-deploy.sh` — **the historical value has been dead since #3611.**
Rotated-since ⇒ moot. A rewrite would spend a full-repo history rewrite and a
team-wide re-clone to hide the word `admin`.

**2. The scrubber fixtures (2 of 35).** `write_scrubber.rs:268` is the test
string the scrubber exists to catch. Removing it would delete the negative
proof of a security guard — the exact hollow-gate move we forbid elsewhere.
Keep, and keep baselined.

## Re-verification (run these, don't trust this file)

```bash
# 1. no historical blob holds a credential longer than the stock default
git log --all --format=%H -- platform/logs/permission-prompts.log \
  | while read c; do git show "$c:platform/logs/permission-prompts.log" 2>/dev/null \
      | grep -cE '\-u +admin:[A-Za-z0-9]{6,}'; done | grep -v '^0$'   # expect: no output

# 2. the live credential is not the historical one
grep -c '^FUSEKI_ADMIN_PASSWORD=admin$' ~/.gathering/data/fuseki-write.env   # expect: 0
```

Both are folded into the security probe suite as `history-disposition.sh`
(#3900 runner), so a NEW long credential entering history reds the nightly
instead of waiting for the next audit.

## Open-source readiness

This closes the history question. Remaining gates before public are separate
cards: `#3387` (scrubber does not cover the log-emit path — the reason logs
carried these lines at all) and `#2645` (no detector on emitted text).
Recording that explicitly so "history disposition complete" is never mistaken
for "safe to open-source."
