// Re-export from chorus-sdk — cards is now a consumer, not the source
import { emit, type SpineEvent } from 'chorus-sdk';

export type { SpineEvent };

/**
 * Suppress real spine-log writes when running under jest. Without this guard,
 * sdk-level tests that exercise addCard/moveCard/doneCard/demoCard leak
 * test-card events into platform/logs/chorus.log, which the Chorus index
 * then surfaces to Clearing as fake Accepted bubbles (#2241 wave 2 incident).
 * Jest sets NODE_ENV=test automatically; production never trips this guard.
 */
const IS_TEST_ENV = process.env.NODE_ENV === 'test';

export function emitSpineEvent(
  event: string,
  role: string,
  extra: Record<string, string | number> = {},
): void {
  if (IS_TEST_ENV) return;
  // #2876: card_id canonical type is integer (matches MCP-emitted events).
  // Logs-query regex `"card_id":NNN\b` only matches unquoted integers, so a
  // string-typed card_id here drops bash-CLI lifecycle events out of
  // chorus_logs_for_card joins. Coerce here, not at every call site.
  const normalized: Record<string, string | number> = {};
  for (const [k, v] of Object.entries(extra)) {
    if (k === 'card_id' && typeof v === 'string' && /^\d+$/.test(v)) {
      normalized[k] = Number(v);
    } else {
      normalized[k] = v;
    }
  }
  emit(event, role, normalized, { appName: 'cards', component: 'cli' });
}

// #2652 AC3 — emitChorusEvent retired 2026-05-02. Single emit function in
// cards (emitSpineEvent above). The one prior call site
// (deploy.verification.completed at sdk.ts:889) migrated to emitSpineEvent for
// canonical-chain uniformity. If a downstream subscriber needs the old envelope
// shape (appName='chorus-events' component='lifecycle'), surface here before
// re-introducing.
