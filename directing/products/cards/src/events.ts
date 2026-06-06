// #3233/#3267: cards no longer depends on chorus-sdk. chorus-sdk's ONLY consumer
// was this file (one emit call); decoupling removes the chorus-sdk-dist-stale
// merged≠live source and collapses cards onto the ONE canonical spine emitter —
// the `chorus-log` script that every werk-verb already shells. No competing emit path.
import { execFile } from 'node:child_process';
import * as path from 'node:path';

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
): string[] {
  const kv: string[] = [];
  for (const [k, v] of Object.entries(extra)) {
    const val = k === 'card_id' && typeof v === 'string' && /^\d+$/.test(v) ? Number(v) : v;
    kv.push(`${k}=${val}`);
  }
  return [event, role, ...kv, 'appName=cards', 'component=cli'];
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
    execFile('bash', [CHORUS_LOG, ...spineArgs(event, role, extra)], () => {
      /* best-effort; ignore exit/stderr */
    });
  } catch {
    /* never let spine emit break a card op */
  }
}
