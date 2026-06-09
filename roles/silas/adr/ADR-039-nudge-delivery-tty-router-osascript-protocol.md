# ADR-039: Nudge delivery — the tty is the router, osascript is the protocol

**Status:** **Proposed** — COS (Wren) drafted 2026-06-09 from Jeff's framing ("tty routing is almost an ADR"; "the tty is the router and osascript is the protocol"; "do NOT start a passive router"). Awaiting SA (Silas) review — it touches Borg observability (the health-check). The routing/transport split is **already built** (#3125, `platform/pulse/src/session-registry.ts`); this ADR canonicalizes the decision and extends it to observability (the #3284 AC8 gap).
**Date:** 2026-06-09
**Author:** Wren (chief-of-staff / product lens) — from Jeff's shaping
**Reviewers:** Silas — systems-architect / Borg-observability lens · Kade — pipeline lens
**Builds on:** #3125 (session registry + tty routing), #3130 (the `--vscode` transport), DEC-107 (persist AND deliver — both paths fire, no fallback-chain)

## Context

Nudge delivery historically resolved a target by **window-title substring match** — `chorus-inject` counted macOS Terminal windows whose name contained `"<role>" + "claude"`. This conflated two different concerns into one fragile guess:

- **Routing** ("which session, where") — answered by *guessing* from a window title.
- **Transport** ("how the keystroke gets there") — *hard-wired* to one mechanism (Terminal-window osascript).

Both axes broke this session, live:

- **Routing broke on title drift.** Jeff's wren session window is titled `chorus — -zsh`, not `wren … claude`. The title is set at launch by the shell/profile, not by Claude — `/rename` (Claude's session name) does not touch it. So the title match found **no wren window** and emitted `nudge.health.failed reason=no-window` **84×** on the 7-day pain board.
- **Transport was wrong for the host.** Jeff runs wren in **VS Code's integrated terminal** (registry: `host=vscode`), whose pseudo-tty is not a Terminal tab — a Terminal-window osascript can never reach it.

The fix is **not** a passive store the agent drains (the inbox/fold as *primary* delivery). Jeff named that inversion explicitly — *"do NOT start a passive router"* — it is the same anti-pattern as a passive data store that doesn't also push (the #3080 "isn't the store also the pulse" lesson). **Delivery stays active push.** osascript **does** reach VS Code — via `chorus-inject --vscode` (#3130), which activates the Code app and keystrokes its focused window. The fold is a history backstop, never the delivery.

## Decision

Nudge delivery is a **routing / transport split**, with two named layers:

1. **The router is the tty (the session registry).** At SessionStart each session writes `{role, pid, tty, host}` to `~/.chorus/sessions/<role>-<pid>.json`. Resolution = the role's **most-recently-registered LIVE** session (`pid` alive; dead entries are never targets). The tty + host is the **address** — exact, host-agnostic, never guessed from a window title. The registry is passive **only as a lookup**, never as a holding pen.

2. **The protocol is active osascript, selected by host.** Routing produces a plan; the transport executes it as an **active push**:
   - `host = terminal | iterm` → `chorus-inject --tty <tty>` (keystroke the tab that owns that tty).
   - `host = vscode` → `chorus-inject --vscode` (activate Code, keystroke its focused window).
   - **no registration** → legacy `chorus-inject <role>` name-match — the *only* place window-title matching survives, as a can't-strand fallback when the registry is empty/stale.

3. **Observability reads the same router.** Any health/liveness check resolves the role through the **registry**, not a Terminal-window probe. A `host=vscode` session's healthy delivery is the `--vscode` push, so the health-check MUST NOT emit `no-window` for it. `no-window` is reserved for a *terminal-host* session whose registered tty has no live window — and it must fail **loud and actionable** ("role X registered terminal tty Y, no live window"), never a blanket title-miss.

The window-title path is **demoted to the no-registration fallback** and retired as a primary resolver or a health signal.

## Consequences

- **Routing survives title drift.** `chorus — -zsh` vs `wren … claude` stops mattering — the tty/host is the address.
- **New hosts add a transport, not a rewrite.** A future host (web, tmux) adds one `planDelivery` arm; routing is untouched.
- **The 84× false `no-window` dies.** The health-check, made registry-aware, stops crying no-window at every vscode session; a real terminal-window loss becomes a loud, actionable alert.
- **No passive intermediary.** Delivery is active push on every host; the fold remains a backstop.
- **#3284 AC8 is the implementation of this clause** — `nudge-health-check.sh` becomes registry-aware (host-branched). The delivery side (`planDelivery`, #3125/#3130) already conforms; this ADR names it as the standing contract so the observability side can't drift back to title-probing.

## Already-conforming / remaining

- **Conforms (built):** `session-registry.ts` (`resolveTarget`/`planDelivery`) + `service.ts:201` (delivery calls `planDelivery(resolveRoleTarget(to), …)`) + `chorus-inject --vscode`/`--tty`.
- **Remaining (AC8 / this ADR's teeth):** `nudge-health-check.sh` is registry-blind — the one surface still title-probing. Making it registry-aware closes the gap.
