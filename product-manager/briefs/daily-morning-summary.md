# Morning Summary — 2026-08-21

**HEADLINE:** Quality signal is blind for the 73rd day — `npm ci` has never been run in this repo and is now the single biggest risk on the board.

---

**OPS:** RED
- RED: 5 domain-context files all 14 days stale (threshold: 7 days). Chorus and infrastructure are highest priority given recent Silas lands (#3947, #3940, #3938).
- RED: 2 WIP cards stale 134 days — "Framework service design — OWL entity model" and "Restore chorus product boundary" — dead weight, need close or requeue today.
- YELLOW: 7 cargo warnings in chorus-hooks, stable but not shrinking. Cleanup card open.
- YELLOW: 17+ LaunchAgent plist files still log to `/tmp` — no migration to `~/.chorus/logs/`.

**QUALITY:** RED
- All 4 TS test suites blocked: `ts-jest preset not found` — **day 71**.
- Lint blocked: `@eslint/js` not found — **day 73**. Root cause: `npm ci` never run at repo root.
- Build: 235 type errors, up from 233 yesterday. Trend is +2/day for two consecutive days; +18 over the past week. Accelerating.
- 0 tests have run in 71 days. Coverage data is stale from June.

**YESTERDAY:** High-velocity day — 18+ cards shipped across all three roles.
- Kade: #3943 (werk-demo witness fix — go reads witness, not launcher pin), #3925 (TestResultShape gains runTs+cardId; werk-domains 0.3.0), #3921, #3939, #3931, #3930, #1778 (×2), #3924, #3929
- Silas: #3947, #3948, #3945, #2645, #3940, #3387, #3938
- Wren: #3944, #3941

**TODAY:**
1. Kade or Silas: `npm ci` at repo root — unblock 4 test suites and lint. 73 days is too long.
2. Silas: refresh `chorus` and `infrastructure` domain-context files first; then remaining 3.
3. Wren: close or requeue the two 134-day WIP cards — get them off the board.

**BLOCKERS (needs Jeff):**
- `npm ci` root: Is there a reason this has never been run? If it's a deliberate choice (remote-only CI), that needs to be documented so quality review can stop flagging it.
- 134-day WIP cards: close them or assign them to a role and set a date.
