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

export function emitSpineEvent(
  event: string,
  role: string,
  extra: Record<string, string | number> = {},
): void {
  if (IS_TEST_ENV) return;
  // #2876: card_id canonical type is integer (matches MCP-emitted events).
  // Logs-query regex `"card_id":NNN\b` only matches unquoted integers, so a
  // string-typed card_id drops bash-CLI lifecycle events out of
  // chorus_logs_for_card joins. Coerce here, not at every call site.
  const normalized: Record<string, string | number> = {};
  for (const [k, v] of Object.entries(extra)) {
    if (k === 'card_id' && typeof v === 'string' && /^\d+$/.test(v)) {
      normalized[k] = Number(v);
    } else {
      normalized[k] = v;
    }
  }
  // Canonical spine emit: `bash chorus-log <event> <role> key=value...` — the same
  // contract werk-verbs use. Best-effort + non-blocking: a spine-emit failure must
  // never break a card op (mirrors werk-verb jsonl/spine discipline).
  const kv = Object.entries(normalized).map(([k, v]) => `${k}=${v}`);
  try {
    execFile('bash', [CHORUS_LOG, event, role, ...kv, 'appName=cards', 'component=cli'], () => {
      /* best-effort; ignore exit/stderr */
    });
  } catch {
    /* never let spine emit break a card op */
  }
}
