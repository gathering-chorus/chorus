/* eslint-disable security/detect-non-literal-fs-filename -- #3429: reads session-registry files from a fixed internal dir (CHORUS_HOME/.sessions), filenames enumerated by readdir of that dir — not untrusted input */
/**
 * #3125 — session registry (read/resolve side).
 *
 * Sessions register {role, pid, tty, host} at SessionStart by writing
 * ~/.chorus/sessions/<role>-<pid>.json (written by chorus-hooks). Delivery
 * resolves role → tty HERE — an exact, host-agnostic routing key — instead
 * of letting chorus-inject guess by window-title substring.
 *
 * Routing/transport split (the card's thesis): this module is ROUTING. It
 * answers "which session, on which tty, in which host." chorus-inject is
 * TRANSPORT. Keeping them separate is what stops a broken transport (a host
 * it can't reach) from corrupting routing.
 *
 * Liveness (AC2): a registration is only valid while its pid is alive. Dead
 * entries are never resolved (and can be pruned). This is what stops the
 * stale "wren — -zsh" class — a dead session can't be a target.
 */
import { readdirSync, readFileSync, unlinkSync, appendFileSync } from 'fs';
import { execFileSync } from 'child_process';
import os from 'os';
import path from 'path';

export interface SessionReg {
  role: string;
  pid: number;
  tty: string;
  host: string; // 'terminal' | 'iterm' | 'vscode' | 'unknown'
  registered_at?: string;
}

export type IsAlive = (pid: number) => boolean;

/** #3608 — what role a pid ACTUALLY runs as (its CHORUS_ROLE env), or null when
 * unverifiable. Injectable so the resolver rule is unit-tested without ps. */
export type RoleOfPid = (pid: number) => string | null;

export const SESSIONS_DIR = path.join(os.homedir(), '.chorus', 'sessions');

/** #3608 — read a live pid's CHORUS_ROLE from its environment via `ps eww`.
 * null when the probe fails or the var is absent (unverifiable ≠ poisoned:
 * never strand delivery on uncertainty — same stance as pid_alive). */
export function actualRoleOfPid(pid: number): string | null {
  try {
    const out = execFileSync('ps', ['eww', '-p', String(pid)], { encoding: 'utf8', timeout: 2000 });
    const m = out.match(/\bCHORUS_ROLE=([a-zA-Z0-9_-]+)/);
    return m ? m[1] : null;
  } catch {
    return null;
  }
}

/** Default liveness probe — signal 0 throws iff the pid is gone. */
export function pidAlive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch (e: unknown) {
    // EPERM means the process exists but we can't signal it — still alive.
    return (e as NodeJS.ErrnoException | undefined)?.code === 'EPERM';
  }
}

/**
 * Resolve a role to its single best live session. Pure: caller supplies the
 * registration list + a liveness predicate, so this is fully unit-testable
 * without touching the filesystem or real processes.
 *
 * - Filters to the role's LIVE sessions (AC2).
 * - #3608: filters out POISONED entries — a registration whose pid actually
 *   runs a DIFFERENT role (its CHORUS_ROLE env disagrees with the file). This
 *   is what made the 07-03 misroutes stable: a test suite registered "silas"
 *   at wren's/kade's live pids, and liveness alone kept trusting it. A null
 *   verdict (unverifiable) keeps the entry — never strand on uncertainty.
 * - Among multiple live sessions (a role with two terminals), picks the
 *   most-recently-registered.
 * - Returns null when none — caller then falls back to the legacy name-match
 *   path, so as-is delivery is preserved when nothing is registered.
 */
export function resolveTarget(
  regs: SessionReg[],
  role: string,
  isAlive: IsAlive,
  roleOf?: RoleOfPid,
): SessionReg | null {
  const live = regs
    .filter((r) => r.role === role && isAlive(r.pid))
    .filter((r) => {
      if (!roleOf) return true;
      const actual = roleOf(r.pid);
      return actual === null || actual === r.role;
    })
    .sort((a, b) => (b.registered_at ?? '').localeCompare(a.registered_at ?? ''));
  return live[0] ?? null;
}

/** Read all registration files from `dir`. Best-effort: a malformed or
 * vanished file is skipped, never throws (registry reads must not break
 * delivery). Returns [] if the dir doesn't exist yet. */
export function readRegistry(dir: string = SESSIONS_DIR): SessionReg[] {
  let names: string[];
  try {
    names = readdirSync(dir).filter((n) => n.endsWith('.json'));
  } catch {
    return [];
  }
  const out: SessionReg[] = [];
  for (const name of names) {
    try {
      const raw = readFileSync(path.join(dir, name), 'utf8');
      const obj = JSON.parse(raw) as SessionReg | null;
      if (obj && typeof obj.role === 'string' && typeof obj.pid === 'number' && typeof obj.tty === 'string') {
        out.push({ ...obj, host: obj.host || 'unknown' });
      }
    } catch {
      /* skip malformed/vanished entry */
    }
  }
  return out;
}

/**
 * #3608 — sweep the registry: delete entries that are dead-pid or role-poisoned
 * (pid's actual CHORUS_ROLE disagrees with the registration). Runs at resolve
 * time so the registry self-heals — no manual `rm` ever again. Best-effort.
 * Returns the swept filenames (for the caller's spine event / log line).
 */
/** #3608 AC2 — spine emitter seam so sweeps are queryable in Loki, not lost to
 * stdout (Kade's review + product gate). Injectable; the default appends a
 * canonical chorus.log JSON line (same shape the pulse emitSpine writes). */
export type SweepEmit = (event: string, fields: Record<string, string>) => void;

export function defaultSweepEmit(event: string, fields: Record<string, string>): void {
  try {
    const logPath = process.env.CHORUS_SPINE_LOG
      || path.join(os.homedir(), 'CascadeProjects', 'chorus', 'platform', 'logs', 'chorus.log');
    const line = JSON.stringify({ timestamp: new Date().toISOString(), event, role: 'pulse', ...fields });
    appendFileSync(logPath, line + '\n'); // covered by the file-level #3429 disable — inner directive was flagged unused (#3606)
  } catch { /* spine emit is best-effort — never break delivery */ }
}

export function sweepRegistry(
  dir: string = SESSIONS_DIR,
  isAlive: IsAlive = pidAlive,
  roleOf: RoleOfPid = actualRoleOfPid,
  emit: SweepEmit = defaultSweepEmit,
): string[] {
  const swept: string[] = [];
  for (const r of readRegistry(dir)) {
    const dead = !isAlive(r.pid);
    const actual = dead ? null : roleOf(r.pid);
    const poisoned = actual !== null && actual !== r.role;
    if (!dead && !poisoned) continue;
    const file = path.join(dir, `${r.role}-${r.pid}.json`);
    try {
      unlinkSync(file);
      const desc = `${r.role}-${r.pid}.json${poisoned ? ` (poisoned: pid runs ${actual})` : ' (dead pid)'}`;
      swept.push(desc);
      // AC2 — poison/stale sweeps are spine events, queryable, never stdout-only.
      emit(poisoned ? 'routing.poison.detected' : 'routing.stale.swept', {
        reg_role: r.role, pid: String(r.pid), tty: r.tty,
        ...(poisoned ? { actual_role: actual as string } : {}),
      });
    } catch { /* vanished or unwritable — skip */ }
  }
  return swept;
}

/**
 * The full resolve a caller wants: read the live registry from disk and
 * return the best live target for `role`, or null to fall back to name-match.
 * #3608: sweeps dead + poisoned entries first (self-healing), then resolves
 * with role re-verification.
 */
export function resolveRoleTarget(role: string, dir: string = SESSIONS_DIR, isAlive: IsAlive = pidAlive, roleOf: RoleOfPid = actualRoleOfPid, emit: SweepEmit = defaultSweepEmit): SessionReg | null {
  sweepRegistry(dir, isAlive, roleOf, emit);
  return resolveTarget(readRegistry(dir), role, isAlive, roleOf);
}

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
  if (target && sender && (target.pid === sender.pid || (!!target.tty && target.tty === sender.tty))) {
    // #3608 review: Wren proposed defer-to-fold here (the 07-03 boomerang case);
    // Jeff KEPT unconditional keystroke (2026-07-04): "nudge has a way of
    // breaking and if it goes to the wrong terminal i want to see it." A
    // visible misdelivery is the alarm; silent defer would hide the break.
    // DEC-107/#3352 stands unamended. Poison prevention lives upstream
    // (env-verified registration + resolve-time role check + sweep).
    return { kind: 'inject', args: [role, content] }; // stale reg ignored — name-match delivers
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
export function describeTarget(role: string, target: SessionReg | null): string {
  if (!target) return `${role} [no live session — name-match fallback]`;
  return `${role} @ ${target.tty || '?'} (${target.host || 'unknown'}, pid ${target.pid})`;
}
