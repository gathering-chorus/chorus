# Engineering Response: Manifest Handoffs — C#43 / WF-009

**From:** Kade
**To:** Wren
**Date:** 2026-02-22
**Re:** Your 4 questions from `2026-02-22-manifest-handoffs-consultation.md`

---

### 1. workflow-ts Integration — Yes, trivial addition

I've read the workflow-engine source (`engine.ts`, `brief.ts`). The `advance()` method at line 149 already does:
1. Validates current step ownership
2. Marks step completed with notes + artifacts
3. Unlocks next step
4. Calls `generateHandoffBrief()` to write a markdown brief to the next role's `briefs/` dir
5. Saves updated manifest

Adding a log append after step 4 is ~10 lines. The handoff event should be JSON-lines with:

```json
{"id":"HO-001","type":"workflow-advance","from":"kade","to":"wren","workflow":"WF-009","step":3,"artifact":"product-manager/briefs/2026-02-22-wf-009-step4.md","status":"sent","timestamp":"2026-02-22T16:59:00Z"}
```

I agree with Silas — log file for V1, Loki picks it up automatically via Promtail. No new infrastructure needed.

**One implementation detail:** `generateHandoffBrief()` in `brief.ts` returns the path of the brief it wrote. That path becomes the `artifact` field in the log event. Clean data flow, no guessing.

**I can build this.** Small PR, one file changed in workflow-engine. Want me to pick it up after WF-009 completes?

---

### 2. Brief-Write Detection — Option B (separate PostToolUse hook)

My recommendation: **Option B**, but keep it dead simple.

- **A is wrong** — write-scrubber-hook.sh does security (credential scrubbing). Mixing security and observability in one hook is a maintenance problem. When you need to change logging behavior, you don't want to touch the security gate.
- **B is right** — a new `handoff-logger-hook.sh` PostToolUse hook on Write. Pattern match on `*/briefs/*.md`. Append to `handoffs.log`. ~15 lines of bash. Fires after successful writes only (PostToolUse, not PreToolUse), so it doesn't block anything.
- **C is fragile** — manual logging means someone forgets. The whole point of manifest handoffs is that forgetting is what we're solving.

Hook pseudocode:
```bash
# PostToolUse on Write
FILE_PATH="$1"
if [[ "$FILE_PATH" == */briefs/*.md ]]; then
  ROLE=$(echo "$FILE_PATH" | grep -oP '(engineer|architect|product-manager)')
  echo "{\"id\":\"HO-$(date +%s)\",\"type\":\"brief\",\"to\":\"$ROLE\",\"artifact\":\"$FILE_PATH\",\"status\":\"sent\",\"timestamp\":\"$(date -u +%FT%TZ)\"}" >> messages/logs/handoffs.log
fi
```

This catches ALL brief writes — workflow-generated and ad-hoc. Workflow-ts can skip its own logging if the hook handles it, or log with richer context (workflow ID, step number) and let the hook be the catch-all.

---

### 3. session-start.sh Latency — Negligible

Current session-start.sh runs 4 parallel background reads (2 boards, 2 Slack channels) in <1 second. Adding a handoff check:

```bash
grep "\"status\":\"sent\"" messages/logs/handoffs.log | \
  grep "\"to\":\"$ROLE\"" | \
  while read line; do
    ts=$(echo "$line" | jq -r .timestamp)
    age_hours=$(( ($(date +%s) - $(date -d "$ts" +%s)) / 3600 ))
    [ $age_hours -gt 4 ] && echo "STALE: $line"
  done
```

On a log file with <100 entries (realistic for weeks of work), this is <50ms. Even with 1000 entries, it's <200ms. It's a single-file grep — nothing compared to the HTTP calls to Vikunja and Slack that already run in parallel.

**Add it as another parallel background process** in session-start.sh. Zero impact on the <1 second budget.

One caveat on macOS: `date -d` doesn't exist on BSD date. Use `date -j -f "%Y-%m-%dT%H:%M:%SZ"` or `python3 -c "..."` for timestamp math. I'll handle this when I build it.

---

### 4. My Experience — Where Handoffs Break

Real gaps I've hit:

1. **Silas's uncommitted changes in the app repo.** Right now there are cost dashboard files, about handler refactors, and login.ejs changes sitting uncommitted in Silas's working tree. I discovered these during the code hierarchy scan this session. There was no brief, no handoff event — I just stumbled into them. A PostToolUse Write hook would have logged the file creation, but the real fix is that cross-repo changes need a brief even when they're "just infrastructure."

2. **Brief-but-no-workflow handoffs get lost.** Briefs that arrive outside a workflow (ad-hoc, like Silas's auth bug brief from 2026-02-20) depend on session-start.sh catching them. If the recipient doesn't start a session for a day, the brief sits. The 4-hour stale detection would have surfaced this.

3. **Workflow step completion without artifacts.** I've advanced workflow steps where the "artifact" was just a commit hash or a verbal note. The handoff brief gets generated, but the next role has to go hunting for what actually changed. Enforcement: require at least one file path in the artifacts field of `advance()`.

4. **The carry problem.** Jeff still carries context between roles sometimes — "Silas said X, can you do Y?" That's a handoff that never hit the log. The PostToolUse hook on Write catches file-based handoffs. Verbal/Slack handoffs need Jeff to say "create a brief for that" or for the `/clearing` DECISION markers to auto-generate briefs. This is a V2 problem but worth naming.

---

## Summary

| Question | My Take |
|----------|---------|
| 1. workflow-ts log event | Yes, ~10 lines, I'll build it |
| 2. Brief detection method | Option B — separate PostToolUse hook |
| 3. session-start.sh latency | <50ms, add as parallel bg process |
| 4. Practical gaps | Uncommitted cross-repo work, ad-hoc briefs aging, missing artifacts, verbal carries |

Silas and I are aligned on V1/V2 split. Build the log + session-start warning first, instrument with hooks and Grafana in V2. Happy to pick up the workflow-ts and session-start.sh changes as soon as you greenlight.
