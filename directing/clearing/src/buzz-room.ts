/**
 * #3823 — the room, both directions.
 *
 * OUT: a visible Clearing message is published to the relay as a kind:1 note
 *      signed by THAT ACTOR's WebID-derived key.
 * IN:  notes on the topic are read back and ingested into the Clearing, with the
 *      author resolved from the signing key rather than from anything the
 *      message says about itself.
 *
 * Jeff asked for this on 2026-03-08 — "how multiple agents can chat in a way
 * that includes a single person, a team interaction not just a jeff->team
 * style" — and again on 2026-08-11. What made it hard was never the transport:
 * our messaging addresses one recipient at a time, so an unaddressed message
 * defaults to a single role and the room is really a switchboard with Jeff
 * standing at it. A topic every actor reads has no switchboard.
 */

import type { ClearingMsg, NostrEvent, NostrSigner } from './buzz-bridge';
import { buildNote } from './buzz-bridge';
import { derivedSigner } from './buzz-signer';

/** The actors with derived keys in the test-drive. */
export const ROOM_ACTORS = ['jeff', 'wren', 'silas', 'kade'] as const;
export type RoomActor = typeof ROOM_ACTORS[number];

export interface RoomIdentity {
  /** hex pubkey → actor name. The only way an incoming note gets attributed. */
  byPubkey: Map<string, RoomActor>;
  /** actor name → signer. Absent for anyone we cannot sign as. */
  signerFor: (actor: string) => NostrSigner | null;
}

/** Build the identity table from the derivation — one source, both directions. */
export function buildRoomIdentity(actors: readonly string[] = ROOM_ACTORS): RoomIdentity {
  const signers = new Map<string, NostrSigner>();
  const byPubkey = new Map<string, RoomActor>();
  for (const actor of actors) {
    const signer = derivedSigner(actor);
    signers.set(actor, signer);
    byPubkey.set(signer.pubkey, actor as RoomActor);
  }
  return {
    byPubkey,
    signerFor: (actor: string) => signers.get(actor.toLowerCase()) ?? null,
  };
}

/**
 * Who signed this note, or null.
 *
 * Null is not "anonymous" — it is "a key we cannot place", and the caller must
 * treat it as such rather than falling back to whatever the note claims. An
 * unbound key rendering as a role would make the signature decorative, which
 * is the failure this whole card exists to avoid.
 */
export function authorOf(ev: { pubkey: string }, identity: RoomIdentity): RoomActor | null {
  return identity.byPubkey.get(ev.pubkey) ?? null;
}

export interface OutboundDeps {
  topic: string;
  identity: RoomIdentity;
  publish: (ev: NostrEvent) => Promise<void>;
  log?: (level: 'info' | 'error', event: string, fields: Record<string, unknown>) => void;
}

/**
 * Publish one Clearing message to the room as its author.
 *
 * Refuses rather than substitutes when the sender has no key. The tempting
 * fallback — sign it with some default key so the message still gets out — is
 * exactly how "[wren] hello" from the bridge key became indistinguishable from
 * Wren. A message nobody can vouch for should not appear in a room whose whole
 * claim is that every message is vouched for.
 */
export async function publishToRoom(msg: ClearingMsg, deps: OutboundDeps): Promise<'sent' | 'no-key' | 'not-visible'> {
  if (!msg.visible) return 'not-visible';
  const signer = deps.identity.signerFor(msg.from);
  if (!signer) {
    deps.log?.('error', 'buzz.room.no_key', { from: msg.from });
    return 'no-key';
  }
  const ev = buildNote(msg, deps.topic, signer);
  await deps.publish(ev);
  deps.log?.('info', 'buzz.room.published', { from: msg.from, id: ev.id.slice(0, 12) });
  return 'sent';
}

export interface InboundResult {
  /** The message to hand the Clearing router, or null if it should not render. */
  msg: ClearingMsg | null;
  /** Why, in the caller's words — for logs and for the refusal to name its state. */
  disposition: 'rendered' | 'unknown-key' | 'own-echo' | 'empty';
}

/**
 * Turn an incoming note into a Clearing message.
 *
 * `seenIds` carries the ids we published ourselves. Without it the room echoes:
 * we publish, the subscription reads our own note back, and it lands in the
 * Clearing a second time. That is not cosmetic — a duplicate of your own
 * message is the single loudest way a chat feels broken.
 */
export function inboundToClearing(
  ev: NostrEvent,
  identity: RoomIdentity,
  seenIds: Set<string>,
): InboundResult {
  if (seenIds.has(ev.id)) return { msg: null, disposition: 'own-echo' };
  if (!ev.content || !ev.content.trim()) return { msg: null, disposition: 'empty' };
  const author = authorOf(ev, identity);
  if (author === null) return { msg: null, disposition: 'unknown-key' };
  return {
    msg: {
      from: author,
      text: ev.content,
      ts: new Date(ev.created_at * 1000).toISOString(),
      type: author === 'jeff' ? 'jeff-input' : 'role-response',
      visible: true,
    },
    disposition: 'rendered',
  };
}
