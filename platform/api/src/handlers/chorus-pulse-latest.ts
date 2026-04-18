/**
 * GET /api/chorus/pulse/latest — most-recent team pulse snapshot (#2188).
 *
 * Dependencies injected:
 *   readPulse — () => string | null — returns file contents or null if missing
 *
 * Behavior:
 *   - null → 404 with instruction
 *   - valid JSON → 200 with parsed object
 *   - unparseable / throws → 500 with error message
 */
import type { FetchResult } from './codebase-topology';

export interface ChorusPulseLatestDeps {
  readPulse: () => string | null;
}

export function fetchChorusPulseLatest(deps: ChorusPulseLatestDeps): FetchResult {
  try {
    const content = deps.readPulse();
    if (content === null) {
      return {
        status: 404,
        body: { error: 'No pulse snapshot available. Run chorus-hook-shim pulse first.' },
      };
    }
    const pulse = JSON.parse(content);
    return { status: 200, body: pulse };
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return { status: 500, body: { error: message } };
  }
}
