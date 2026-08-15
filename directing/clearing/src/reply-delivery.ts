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
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';

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
export function makeSpineEmitter(spinePath?: string): EmitSpine {
  // #3890 root-cause: the HTTP pulse door grew an authn gate; unauthenticated
  // POSTs got authn-missing and the fire-and-forget fetch never checked the
  // body — every reply.rendered silently dropped (Jeff, 2026-08-15: "still
  // pretty sure im not getting messages"; measured 8 emitted / 0 rendered).
  // Append to the spine directly like pulse's emitSpine — same host, same
  // file, offset-ISO Boston (#3880 one-clock), no credential in the render path.
  const target = spinePath
    ?? process.env.CHORUS_LOG_FILE
    ?? path.join(os.homedir(), '.chorus', 'chorus.log');
  return (ev) => {
    const line = JSON.stringify({ timestamp: bostonOffsetIso(), ...ev }) + '\n';
    fs.appendFile(target, line, (e) => {
      if (e) console.error('[reply-delivery] rendered-stamp failed:', e.message);
    });
  };
}

/** #3880 one-clock: offset-ISO Boston (same rule as every spine writer). */
function bostonOffsetIso(d: Date = new Date()): string {
  const parts = new Intl.DateTimeFormat('en-US', {
    timeZone: 'America/New_York',
    year: 'numeric', month: '2-digit', day: '2-digit',
    hour: '2-digit', minute: '2-digit', second: '2-digit',
    hour12: false, timeZoneName: 'longOffset',
  }).formatToParts(d);
  const get = (t: string) => parts.find(p => p.type === t)?.value ?? '';
  const hour = get('hour') === '24' ? '00' : get('hour');
  const offset = (get('timeZoneName') || 'GMT+00:00').replace('GMT', '');
  const ms = String(d.getMilliseconds()).padStart(3, '0');
  return `${get('year')}-${get('month')}-${get('day')}T${hour}:${get('minute')}:${get('second')}.${ms}${offset}`;
}
