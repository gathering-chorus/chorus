# Next Session — Wren

## This session shipped (2026-04-25 11:24–11:35)

**The diagnosis that closed two sessions of MCP non-reachability** (#2476):

Reboot from prior session loaded `.mcp.json` files Silas had dropped per-role — reach test STILL negative. Fresh ToolSearch for `chorus_principles_list` returned only Gmail/Drive/harness. That was the load-bearing data: `.mcp.json` substitution `${CHORUS_ROLE}` reads at claude-startup from the claude process's env, but settings.json `env` block sets env for tools claude *spawns*, not claude itself. CHORUS_ROLE was empty in all 3 claude PIDs.

**The fix Silas landed**:
1. `platform/shell/chorus-role-env.sh` — zsh `chpwd` hook that exports `CHORUS_ROLE=wren|silas|kade` based on PWD matching `*/chorus/roles/<name>/*`. Idempotent guard, initial-fire, bash fallback.
2. One line appended to `~/.zshrc` sources it. (Flagged to Jeff — his dotfile.)
3. Retires the three per-role `.mcp.json` files; one root `.mcp.json` with `${CHORUS_ROLE}` substitution does the job.

**Verification protocol agreed**: open fresh tab → `cd roles/wren` → `env | grep CHORUS_ROLE` shows `wren` BEFORE claude starts → THEN run reach test in that fresh seat.

**Jeff confirmed env binding**: `CHORUS_ROLE=wren` printed in fresh tab. The shell-level fix is verified. The claude-level reach test has not yet been run from a fresh claude process.

## First action next session (load-bearing)

**You are the fresh-seat reach test.**

If this session was started in a tab where `env | grep CHORUS_ROLE` printed `wren` BEFORE `claude` ran, then:

1. **First turn**: ToolSearch query for `chorus_principles_list` (or any `chorus_*`).
2. If schemas return → call one (e.g., `chorus_principles_list`) → confirm spine event lands with `from=wren, origin=mcp` → that closes #2476 AC#7.
3. If still not visible → report to Silas — the chpwd fix worked at shell level but not at claude-startup level. Likely needs claude restarted from a shell where the var is already exported (not via `claude` invoked before `.zshrc` reload).

After reach-confirmed: route to Kade for `/gate-code` re-run, then re-`/gate-product` on #2476 for clean PASS, then close.

## Open threads (carried)

1. **#2476 close** — pending fresh-seat reach test (this session).
2. **ADR-026** — Jeff sign-off pending: flip Status: Proposed → Accepted; toggle GitHub branch protection on `main` requiring CI green.
3. **gate-product skill amendment (JX/AX gates 8 & 9)** — named, not yet executed. Two retracts in a week is the data. Position: write today.
4. **Drive harvest decision** — scan-first vs ADR-027 first. Wren recommends scan-first.
5. **Principles reference-impl page** — stale card-status sections (FYA, not action).
6. **#2116** — fourth-session flinch. Acceptance protocol design is the unblocker, not child 1/7.

## WIP at session close
- **#2476** — WIP, gate-product retracted, awaiting reach test from fresh claude seat.

## Friction this session
- Two consecutive sessions where "fix" looked shipped but cold seat said no. Pattern: testing config-file-on-disk instead of tool-list manifest. Folded into gate-product amendment plan (open thread #3).
- Productive collaboration shape: silas-wren chat thread `silas-wren-1777131059` — architect read + position + verification protocol in 4 turns. That's the shape the gate-product amendment should encode.
