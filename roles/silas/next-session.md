# Next Session — Silas

## Headline carry-forward: the daemon-deploy self-reference (Jeff's observation)
Jeff, end of this session: *"i honestly dont know why we have a daemon managing deploy."* He's right, and it's the real root.
- chorus-mcp (daemon) serves `chorus_acp`/`commit`/`pull` — the very tools that deploy chorus-mcp. **Self-referential.** That loop is the root of tonight's whole storm: the #3012 AC3 verify-needs-deploy-needs-acp trap, the werk-close fetch-race, the rebase-into-canonical recovery.
- The daemon doesn't *need* to manage deploy — the actual work is plain scripts (chorus-deploy, chorus-werk, git-queue). The daemon is just a typed-MCP enforcement wrapper (added #2750/#2751 so the model can't skip gate steps).
- **#3016 is a workaround for the symptom, not the root.** Root question: should deploy be mediated by the daemon it deploys at all? Two directions: (1) keep MCP enforcement but break the self-reference (deploying chorus-mcp doesn't route through chorus-mcp's own acp); (2) deploy = plain scripts + hook enforcement, no daemon in the path.
- Jeff wants this thought through as a proper arch note **when fresh, not decided tired.** Highest-value next move. Do it BEFORE building more deploy machinery.

## #3016 — ready for Jeff's /acp (NOT yet accepted)
- Gates 5/5 PASS (product/code/quality/arch/ops). On #3012's base, HEAD 980c230a. #3012's `|| true` preserved, 17/10/4 tests green, no revert.
- Jeff did NOT /acp this session (his call). On /acp: rebases clean → I deploy chorus-mcp from canonical → live-confirm.
- WATCH on its acp (Kade flagged): werk-close fetch-race — if it throws werk-close-fail with the merge already on main, that's the race; fix is rebase/fetch, NOT re-run-into-canonical. (#3016's werk exists + resolver fix live, so safer than #3012's recovery.)

## Done this session
- Post-crash recovery (2hr power outage): verified all alerts; most transient false-positives; system self-healed.
- #3014 shipped (PR #297) — chorus-werk remove acp-commit-on-main check. Storm step 1.
- #3012 landed end-to-end (storm step 3) — env-setup zero-werk fix + live mcp-server AC3 desc. Deployed chorus-mcp from canonical; AC3 live-confirmed (three-way).
- Board-auth: Vikunja token clock-expired mid-session → fixed with a **no-expiry API token** (tk_, all scopes, 2099) in .env. Permanent.
- #3016 built + demoed (gates 5/5), pending Jeff /acp.

## Open (mine) — but per Jeff, do NOT just card these
- werk-close fetch-race (bit #3012's acp; real). Fix: werk-close must see the acp commit it merged before grepping origin/main.
- deploy-daemon-card.sh rename (double-misnomer post-#2927) + chorus-mcp not a KNOWN_UNIT.
- The daemon-deploy arch question above may make some moot. Root-first.

## Hard truth from Jeff (saved to memory: cards-are-not-fixes)
Cards in Later aren't fixes — they're deferral. #3009 sat 2 days while its alert kept firing. Stop offering "I'll file a card" as the resolution to a live problem. Fix-now or name-it-untouched. The board accumulates; that's the failure mode.

## Noise to ignore
chorus-health `session-envelope-completeness` — crash-window stale events, self-clearing, #3009 retire-target. Not real.
