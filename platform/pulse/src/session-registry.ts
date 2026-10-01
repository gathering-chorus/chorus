/* eslint-disable security/detect-non-literal-fs-filename -- #3429: the turn marker is read from a fixed internal dir, filename built from the role name */
/**
 * #3125 routing, #4361 source. Where a role is comes from its Presence row
 * (presence-target.ts); the ~/.chorus/sessions registry files are no longer
 * read for routing. This module keeps the transport plan (tmux / tty / name)
 * and the busy-turn marker (<role>.turn.json, written by chorus-hooks).
 */
import { readFileSync } from 'fs';
import os from 'os';
import path from 'path';

export interface SessionReg {
  role: string;
  pid: number;
  tty: string;
  host: string; // 'terminal' | 'iterm' | 'vscode' | 'tmux' | 'unknown'
  // #3668 — exact tmux pane id (%N) when the session runs inside tmux.
  // Present → delivery routes `--tmux <pane>`: app-level via the tmux server
  // (locked-screen-safe), instead of vscode HID keystrokes.
  tmux?: string;
  registered_at?: string;
}

export const SESSIONS_DIR = path.join(os.homedir(), '.chorus', 'sessions');

/**
 * #3700 (fallback taxonomy) — typed resolution. A miss SAYS WHY:
 *  - resolved:     a live, unpoisoned session for the role (delivery proceeds)
 *  - dead:         entries exist for the role but none is a live, honest pid
 *                  (includes poisoned entries — a pid whose actual role
 *                  disagrees is NOT a live session of this role)
 *  - unregistered: no entry for the role at all
 * #4361: resolved from the Presence row (presence-target.ts), never the
 * registry files. Callers must map dead/unregistered to a TYPED undelivered outcome — never
 * fall through to legacy name-match (the 2026-07-26 cross-role spray).
 */
export type TypedResolution =
  | { kind: 'resolved'; session: SessionReg }
  | { kind: 'dead' }
  | { kind: 'unregistered' }
  // #4362 — a live run whose Presence has no tmux pane (logged in outside tmux,
  // e.g. a headless gate run). Nothing safe to type into: the message waits
  // for the role's turn-end drain, and the sender is told why.
  | { kind: 'no-pane' };

export type DeliveryPlan =
  | { kind: 'inject'; args: string[] }
  | { kind: 'defer'; reason: string };

/**
 * #3125 — decide HOW to deliver to `role` given its resolved registration.
 * This is the routing/transport seam: routing produces the plan, the caller
 * (pulse runInject) executes it. Pure + fully testable.
 *
 *  - vscode host          → `chorus-inject --vscode` (#3130 layer 2). A VS Code
 *                           pseudo-tty is NOT a Terminal tab, so `--tty` returns
 *                           no-window-found. The vscode path targets the Code app
 *                           and keystrokes into its focused window. (#3130 layer 1
 *                           had removed the old `vscode → defer` silent-queue;
 *                           layer 2 gives vscode a transport that actually lands.)
 *  - other host + tty     → exact tty match via `chorus-inject --tty` (Terminal/
 *                           iTerm expose a tab tty; vscode does not).
 *  - no registration      → legacy `chorus-inject <role> <text>` name-match.
 *                           As-is delivery is preserved whenever the registry
 *                           is empty or stale — the new path can never strand.
 */
/** #4361 — Presence targets carry pid 0, so a pid match only counts when it
 * is a real pid; a shared tmux pane or tty is the same session. */
export function sameSession(a: SessionReg, b: SessionReg): boolean {
  if (a.pid > 0 && a.pid === b.pid) return true;
  if (a.tmux && a.tmux === b.tmux) return true;
  return !!a.tty && a.tty === b.tty;
}

export function planDelivery(
  target: SessionReg | null,
  role: string,
  content: string,
  sender: SessionReg | null = null,
): DeliveryPlan {
  // #3352 final form, Jeff's ruling (2026-06-11, DEC-107 re-affirmed): "osascript
  // all the time" — delivery is UNCONDITIONAL. The defer-on-collision/-ambiguity
  // rules shipped earlier today made delivery conditional and silenced team wakes
  // for 2 hours; they are deleted. The 06-11 misdeliveries were a TARGETING DATA
  // bug (a stale registration claiming silas at wren's pid) — fixed as data, not
  // by skipping delivery. `sender` stays in the signature: a target registration
  // that collides with the SENDER is treated as STALE DATA and ignored, so
  // resolution falls through to the legacy role name-match — still a keystroke,
  // never a skip.
  if (target && sender && sameSession(target, sender)) {
    // #3608 review: Wren proposed defer-to-fold here (the 07-03 boomerang case);
    // Jeff KEPT unconditional keystroke (2026-07-04): "nudge has a way of
    // breaking and if it goes to the wrong terminal i want to see it." A
    // visible misdelivery is the alarm; silent defer would hide the break.
    // DEC-107/#3352 stands unamended. Poison prevention lives upstream
    // (env-verified registration + resolve-time role check + sweep).
    return { kind: 'inject', args: [role, content] }; // stale reg ignored — name-match delivers
  }
  // #3668 — a tmux-hosted session is reached through the tmux server (osascript
  // do-shell-script → load-buffer/paste-buffer), which lands with the screen
  // locked / monitor asleep. The pane id is the exact key; it beats host
  // classification because tmux-in-VS-Code may still register host=vscode
  // during rollout.
  if (target && target.tmux) {
    return { kind: 'inject', args: ['--tmux', target.tmux, content] };
  }
  if (target && target.host === 'vscode') {
    return { kind: 'inject', args: ['--vscode', content] };
  }
  if (target && target.tty) {
    return { kind: 'inject', args: ['--tty', target.tty, content] };
  }
  return { kind: 'inject', args: [role, content] };
}

/**
 * #3439 AC3 — a human-readable summary of WHERE a nudge resolved, so the MCP
 * can report the actual destination instead of a blind "sent". `target` is the
 * result of `resolveRoleTarget(role)`: null means no live session was found, so
 * delivery falls back to legacy name-match (named explicitly here rather than
 * hidden). Pure, so it's unit-tested without the registry/fs.
 */
/**
 * #3700 — the target's turn state, read from <role>.turn.json beside the
 * registry (written by the chorus-hooks shim: user-prompt-submit marks busy,
 * stop + session-start clear). File seam, no IPC — the Rust and TS halves
 * meet on disk.
 */
export interface TurnState { busy: boolean; since?: string }

/** Staleness bound (ms) — a busy marker older than this decays to idle: a
 * crash mid-turn must not queue the role's traffic forever. 30 min covers the
 * longest observed real turns (wren's 10-min herds) with margin. */
export const BUSY_STALENESS_MS = 30 * 60 * 1000;

/** #3700 — read <role>.turn.json (written by the chorus-hooks shim). Missing
 * or malformed file = idle: delivery must never be blocked by a bad marker. */
export function readTurnState(role: string, dir: string = SESSIONS_DIR): TurnState {
  try {
    const raw = readFileSync(path.join(dir, `${role}.turn.json`), 'utf8');
    const obj = JSON.parse(raw) as TurnState | null;
    return obj && typeof obj.busy === 'boolean' ? obj : { busy: false };
  } catch { return { busy: false }; }
}

export type TypedDeliveryPlan =
  | { kind: 'inject'; args: string[] }
  | { kind: 'queue'; reason: string }
  | { kind: 'undelivered'; reason: 'dead' | 'unregistered' | 'no-pane' };

/**
 * #3700 (Silas half) — the typed delivery decision, replacing null→name-match:
 *  - resolved + idle → inject NOW (nudge-as-wake preserved; transport args
 *    delegated to planDelivery so tmux/vscode/tty routing stays single-sourced)
 *  - resolved + busy (fresh marker) → queue; the target's own turn-boundary
 *    hook drains it — pull-based last mile, re-spray impossible by construction
 *  - dead/unregistered → typed undelivered. NEVER name-match. Jeff's 07-04
 *    "I want to see it" ruling is honored by VISIBILITY, not misdelivery: the
 *    caller must report the typed reason to the sender + emit a spine alarm
 *    (amendment approved with #3700, 2026-07-26 — the busy-spray incident).
 */
export function planDeliveryTyped(
  res: TypedResolution,
  role: string,
  content: string,
  turnStateOf: (role: string) => TurnState,
  now: () => number = Date.now,
): TypedDeliveryPlan {
  if (res.kind !== 'resolved') return { kind: 'undelivered', reason: res.kind };
  const turn = turnStateOf(role);
  const fresh = turn.busy && (!turn.since || now() - Date.parse(turn.since) < BUSY_STALENESS_MS);
  if (fresh) return { kind: 'queue', reason: `target busy since ${turn.since ?? 'unknown'}` };
  // transport routing delegated so tmux/vscode/tty stay single-sourced; a
  // defer plan (sender-collision path) maps to queue — same typed surface.
  const plan = planDelivery(res.session, role, content);
  return plan.kind === 'inject' ? plan : { kind: 'queue', reason: plan.reason };
}

export function describeTarget(role: string, target: SessionReg | null): string {
  if (!target) return `${role} [no live session — name-match fallback]`;
  return `${role} @ ${target.tty || '?'} (${target.host || 'unknown'}, pid ${target.pid})`;
}
