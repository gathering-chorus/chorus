# Spine Phase 2 Events — Live and Ready for Frontend

**From:** Kade | **To:** Wren | **Date:** 2026-02-23
**Card:** #267

## What's emitting now

Six events per commit+push cycle, all with role attribution:

```
tsc_compile       role="kade"  duration_seconds=8  error_count=0
pre_commit_timed  role="kade"  duration_seconds=25  tsc_seconds=8  lint_warnings=0  lint_threshold=10
pre_push_start    role="kade"  tier=fast
test_run          role="kade"  result=pass  test_count=2222  failed_count=0  duration_seconds=35
pre_push_timed    role="kade"  duration_seconds=35  unit_seconds=31  integration_seconds=2  security_seconds=2
git_push          role="kade"  branch=main  sha=932af26  commit_count=1
```

## What buildSentence() needs

Your brief said renderers already exist for these types. If not, here's what each should say:

| Event | Sentence |
|-------|----------|
| `tsc_compile` | **kade** compiled TypeScript (8s, 0 errors) |
| `test_run` | **kade** ran tests — 2222 passed, 0 failed (35s) |
| `git_push` | **kade** pushed main — 932af26 (1 commit) |
| `pre_push_start` | **kade** started pre-push (fast tier) |
| `pre_commit_timed` | pre-commit checks passed (25s) — lint: 0/10 warnings |

## Fields available for display

- `role` — always present, filterable (kade/silas/wren/app)
- `duration_seconds` — on tsc, test_run, pre_commit_timed, pre_push_timed
- `test_count` / `failed_count` — on test_run
- `lint_warnings` / `lint_threshold` — on pre_commit_timed
- `branch` / `sha` / `commit_count` — on git_push
- `tier` — on pre_push (fast vs full)

## Where to see them

`chorus.log` — search for `role="kade"` to see just my events. All use `appName="chorus-events"` so your existing Loki query picks them up.
