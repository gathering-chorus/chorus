// @domain: messages

import { ClearingMessage, NostrEvent } from './types';

export function roomOf(event: NostrEvent): string {
  // tags is typed required, but relay events are external input: guard at runtime.
  const tags: unknown = event.tags;
  const roomTag = Array.isArray(tags)
    ? event.tags.find((t) => t[0] === 'room' || (t[0] === 't' && t[1] !== 'reply'))
    : undefined;
  return roomTag ? roomTag[1] : 'team';
}

/** #4592 — the note's kind tag (v1's buildNote), only the two kinds the room's toggle owns. */
export function kindOf(event: { tags?: string[][] }): 'nudge' | 'alert' | undefined {
  const v = (event.tags || []).find((t) => t[0] === 'kind')?.[1];
  return v === 'nudge' || v === 'alert' ? v : undefined;
}

export function toClearingMessage(event: NostrEvent, role: string, tick: boolean, signer?: string): ClearingMessage {
  const kind = kindOf(event);
  // #4608 — the room's stored copy of a sender's note carries that note's id, so
  // the copy and the original are one message (the router drops a seen id).
  const signed = Array.isArray(event.tags) ? event.tags.find((t) => t[0] === 'signed')?.[1] : undefined;
  return {
    ...(kind ? { kind } : {}),
    id: signed || event.id,
    from: role,
    text: event.content,
    ts: new Date(event.created_at * 1000).toISOString(),
    type: role === 'jeff' ? 'jeff-input' : 'role-response',
    visible: true,
    pubkey: event.pubkey,
    room: roomOf(event),
    tick,
    signer,
  };
}
