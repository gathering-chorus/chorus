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
import { readdirSync, readFileSync } from 'fs';
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

export const SESSIONS_DIR = path.join(os.homedir(), '.chorus', 'sessions');

/** Default liveness probe — signal 0 throws iff the pid is gone. */
export function pidAlive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch (e: unknown) {
    // EPERM means the process exists but we can't signal it — still alive.
    return (e as NodeJS.ErrnoException)?.code === 'EPERM';
  }
}

/**
 * Resolve a role to its single best live session. Pure: caller supplies the
 * registration list + a liveness predicate, so this is fully unit-testable
 * without touching the filesystem or real processes.
 *
 * - Filters to the role's LIVE sessions (AC2).
 * - Among multiple live sessions (a role with two terminals), picks the
 *   most-recently-registered.
 * - Returns null when none — caller then falls back to the legacy name-match
 *   path, so as-is delivery is preserved when nothing is registered.
 */
export function resolveTarget(
  regs: SessionReg[],
  role: string,
  isAlive: IsAlive,
): SessionReg | null {
  const live = regs
    .filter((r) => r.role === role && isAlive(r.pid))
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
      const obj = JSON.parse(raw) as SessionReg;
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
 * The full resolve a caller wants: read the live registry from disk and
 * return the best live target for `role`, or null to fall back to name-match.
 */
export function resolveRoleTarget(role: string, dir: string = SESSIONS_DIR, isAlive: IsAlive = pidAlive): SessionReg | null {
  return resolveTarget(readRegistry(dir), role, isAlive);
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
export function planDelivery(target: SessionReg | null, role: string, content: string): DeliveryPlan {
  if (target && target.host === 'vscode') {
    return { kind: 'inject', args: ['--vscode', content] };
  }
  if (target && target.tty) {
    return { kind: 'inject', args: ['--tty', target.tty, content] };
  }
  return { kind: 'inject', args: [role, content] };
}
