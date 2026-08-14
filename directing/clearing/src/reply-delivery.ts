/**
 * reply-delivery.ts — #3864: the Clearing half of reply-delivery correlation.
 *
 * chorus-hooks stamps `reply.emitted` (with a content hash) the moment a
 * role's final reply stands at the Stop hook; this module stamps
 * `reply.rendered` with the SAME hash when the session-tailer routes that
 * reply into Clearing. Pulse joins the pair; an emitted without a rendered
 * within the window fires `reply.delivery.gap`.
 *
 * The hash rule is the cross-language contract in
 * config/reply-hash-fixtures.json (word-cap-fixtures pattern): canonicalize
 * (header-strip + whitespace-collapse + trim), then sha256 hex[..16]. Nothing
 * beyond that — each extra normalization is a place for the two
 * implementations to disagree (#3818's lesson).
 */
import { createHash } from 'crypto';

/**
 * Canonical form both surfaces can reach from their own bytes: strip one
 * LEADING '--- ... ---' chorus header, collapse whitespace runs to a single
 * space, trim. The Rust extractor keeps the header and joins blocks with
 * '\n'; this tailer strips it and joins with spaces — canonicalization is
 * what lets the two hashes join at all.
 */
function canonicalize(text: string): string {
  const stripped = text.trimStart().replace(/^---[^]*?---\s*/, '');
  return stripped.split(/\s+/).filter(Boolean).join(' ');
}

/** Join key for one reply across surfaces. sha256(canonicalize(text)), hex[..16]. */
export function contentHash(text: string): string {
  return createHash('sha256').update(canonicalize(text), 'utf8').digest('hex').slice(0, 16);
}

export interface RenderedEvent {
  event: 'reply.rendered';
  role: string;
  surface: 'clearing';
  hash: string;
}

/** Shape the reply.rendered spine event for one routed role reply. */
export function renderedEvent(role: string, text: string): RenderedEvent {
  return { event: 'reply.rendered', role, surface: 'clearing', hash: contentHash(text) };
}

export type EmitSpine = (ev: RenderedEvent) => void;

/**
 * Build the default emitter: POST to chorus-api's spine door
 * (POST /api/chorus/pulse — the sanctioned lifecycle write, replaces
 * chorus-log.sh). Fire-and-forget with a loud console on failure: a dropped
 * rendered-stamp must not break rendering, but it must not vanish silently
 * either — it will surface as a delivery gap, which is the truthful signal.
 */
export function makeSpineEmitter(apiBase = 'http://localhost:3340'): EmitSpine {
  return (ev) => {
    fetch(`${apiBase}/api/chorus/pulse`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(ev),
    }).catch((e) => console.error('[reply-delivery] rendered-stamp failed:', e?.message ?? e));
  };
}
