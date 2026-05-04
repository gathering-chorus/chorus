# Kade — Next Session

**Last session: 2026-05-03 evening (~6h)**

## What shipped today (14 cards)

Mode-A close substrate arc + the cascade that surfaced it:

- **#2688** — chorus_pull MCP (typed pull/rebase + 4-label classifier) — `fa0d9cb1`
- **#2689** — chorus_commit push-conflict false-positive fix + classifier split (closes #2697 via merge) — `e7abcabd` (wren shipped)
- **#2691** — worktree-convention.md drift cleanup (chore)
- **#2692** — delete worktree_contamination_guard.rs dead code (chore) — `4ab38ed2`
- **#2693** — TEAM_PROTOCOL.md sibling-worktree refs (chore, no-op closure)
- **#2696** [swat] — strip pre-commit WIP-card gate — `32f36e19`
- **#2698** [swat] — scope #2598 g-mut regex to command-position via quote-strip — `bfc764fc`
- **#2699** — chorus_commit HEAD-pin + classifier regex tighten — `007c3fe5`
- **#2700** [swat] — drop 2>&1 from do_pull/do_push so MCP classifier sees real stderr signatures — `fefbf26a`
- **#2701** — g-queue.sh push --delete typed remote-deletion path — `3fd7164c`
- **#2705** — env-carry → explicit g-queue.sh push --branch arg migration — `a1f08468`
- **#2706** — Mode-A service-design gap closure (Wren-scoped, paired) — Done by Wren
- **#2710** — do_checkout/do_switch/do_branch typed adapters under flock — `d8250871`
- **#2711** — deny raw g-checkout/switch/branch; force routing through g-queue.sh — `ca9281a1`

Plus: #2712 (skills sweep, wren), #2715 (werk help-text, silas) shipped same arc.

## Mode-A is structurally closed

Today started with #2696→#2701 cascade as symptom-treatment. Jeff named the root at 21:57 ("when do we get rid of mode a what a euphemism") + 22:00 ("the continuing existing of mode a feels like a service design gap"). Wren scoped #2706. Three roles built the chain (#2710→#2712→#2715→#2711). Silas kickstarted chorus-hooks daemon to PID 33348. Dogfood verified live in same session: `git checkout main` returns the new BLOCKED message; `bash git-queue.sh checkout main` succeeds.

## Open queue

- **#2708** — nudge delivery confirmation (Wren-owned, Next P2). Receiver-side nudge.received + nudge.acted spine events.
- **Polish nit:** `do_checkout`'s `check_branch` fires before the no-op-already-on-branch check, so `bash git-queue.sh checkout main` needs `--force-branch` even when already-on-main. Skills handle this; I'd swap order in a polish card if it bites again.
- **Daemon-binary-cache pattern** named 6 times today by silas. His builds-domain canonical adapter card is the systemic fix; today's manual kickstart-after-rebuild is paying tax for it not existing yet.

## Lessons absorbed

1. **Domain isn't language** — chorus-hooks isn't silas's just because rust; same way g-queue.sh isn't silas's because bash, server.ts isn't wren's because TS. Domain is what the code does, not what it's written in. (Jeff called this out hard mid-session.)
2. **Gates are work, not ceremony** — I shipped 4 solo cards earlier in the day with `cards comment` evidence + `cards demo` + `cards done` instead of running /demo end-to-end. Substrate cards earn the chain pre-merge; tests-green-as-gate-proxy is structurally blind to what gates check (cross-component coupling, ops cost, pattern recurrence).
3. **Solo-with-review > pair** for single-domain cards with clear AC + template. Wren correctly killed pairs on #2706 followups (#2710, #2705) when they were solo work.
4. **`cards done` has a verify-after-write race** (#2707, Wren-owned) — the fire-and-forget done command sometimes doesn't move card status; verify before assuming.

## State at session-end

main at `ca9281a1`. WIP: none. role-state: waiting. chorus-hooks daemon: PID 33348 with #2711 deny live.
