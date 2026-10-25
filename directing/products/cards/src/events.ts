// #3233/#3267: cards no longer depends on chorus-sdk. chorus-sdk's ONLY consumer
// was this file (one emit call); decoupling removes the chorus-sdk-dist-stale
// merged≠live source and collapses cards onto the ONE canonical spine emitter —
// the `chorus-log` script that every werk-verb already shells. No competing emit path.
import { execFile } from 'node:child_process';
import * as path from 'node:path';
import * as fs from 'node:fs';
import * as os from 'node:os';

// Local type (was re-exported from chorus-sdk; only consumed via index.ts re-export).
export type SpineEvent = {
  event: string;
  role: string;
  [key: string]: string | number;
};

/**
 * Suppress real spine-log writes when running under jest. Without this guard,
 * sdk-level tests that exercise addCard/moveCard/doneCard leak test-card events
 * into the spine, which the Chorus index surfaces to Clearing as fake Accepted
 * bubbles (#2241 wave 2 incident). Jest sets NODE_ENV=test automatically.
 */
const IS_TEST_ENV = process.env.NODE_ENV === 'test';

// Resolve chorus-log the same way cards resolves role-state (sdk.ts:19).
const CHORUS_LOG = path.resolve(__dirname, '../../../../platform/scripts/chorus-log');

/**
 * Pure builder for the chorus-log argv (after the script path). Exported so the
 * arg contract is unit-testable without spawning a subprocess.
 * #2876: card_id is coerced numeric-string → number so it reaches chorus-log as a
 * numeric token (chorus-log owns the final JSON typing; passing the bare integer
 * keeps the canonical `card_id:NNN` shape and keeps non-numeric ids untouched).
 */
export function spineArgs(
  event: string,
  role: string,
  extra: Record<string, string | number> = {},
  turnLine?: string,
): string[] {
  const kv: string[] = [];
  for (const [k, v] of Object.entries(extra)) {
    const val = k === 'card_id' && typeof v === 'string' && /^\d+$/.test(v) ? Number(v) : v;
    kv.push(`${k}=${val}`);
  }
  // #4637 AC4 — a card action taken in a turn a Jeff line woke names that line
  if (turnLine) kv.push(`started_by_line=${turnLine}`);
  return [event, role, ...kv, 'appName=cards', 'component=cli'];
}

/** A turn file older than this is not the current turn. */
export const TURN_FRESH_MS = 6 * 3600 * 1000;

/**
 * #4637 AC4 — the Jeff line that woke the role's current turn, from the file
 * chorus-hooks keeps (~/.chorus/run/turn-<role>.json, turn_line.rs). Undefined
 * when the turn was a nudge's, the file is stale, or there is no file: a card
 * action with no line is a fact, never a guess.
 */
export function readTurnLine(role: string, dir = process.env.CHORUS_TURN_DIR || path.join(os.homedir(), '.chorus', 'run'), now = Date.now()): string | undefined {
  if (!/^[a-z][a-z0-9-]*$/.test(role)) return undefined;
  try {
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- the role is checked against [a-z0-9-] above; dir is ours
    const t = JSON.parse(fs.readFileSync(path.join(dir, `turn-${role}.json`), 'utf8'));
    const line = typeof t.woken_by_line === 'string' ? t.woken_by_line : undefined;
    const at = typeof t.at === 'number' ? t.at * 1000 : 0;
    return line && now - at < TURN_FRESH_MS ? line : undefined;
  } catch {
    return undefined;
  }
}

export function emitSpineEvent(
  event: string,
  role: string,
  extra: Record<string, string | number> = {},
): void {
  if (IS_TEST_ENV) return;
  // Canonical spine emit: `bash chorus-log <event> <role> key=value...` — the same
  // contract werk-verbs use. Best-effort + non-blocking: a spine-emit failure must
  // never break a card op (mirrors werk-verb jsonl/spine discipline).
  try {
    execFile('bash', [CHORUS_LOG, ...spineArgs(event, role, extra, readTurnLine(role))], () => {
      /* best-effort; ignore exit/stderr */
    });
  } catch {
    /* never let spine emit break a card op */
  }
}
