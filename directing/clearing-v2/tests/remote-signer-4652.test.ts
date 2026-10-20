// @test-type: unit — an in-memory relay and a stub signing service; no network, no real key.
// @domain: messages
// @card: #4652
// @owner: wren
/**
 * #4652 reopen 2 — Jeff picked A2 (10-20 11:53): a signing service holds a copy of his app
 * key and Clearing v2 asks it to sign his posts, so the app shows them as Jeff and the
 * room never holds his key. Silas measured 11:54 that our relay carries kind 24133 between
 * non-member keys.
 */
import { schnorr } from '@noble/curves/secp256k1';
import { bytesToHex, hexToBytes } from '@noble/hashes/utils';
import { RemoteSigner, SignerTransport, NIP46_KIND, UnsignedEvent } from '../src/remote-signer';
import { conversationKey, decrypt, encrypt } from '../src/nip44';
import { buildEventId, serializeEvent } from '../src/identity';
import { NostrEvent } from '../src/types';

const hex = (n: number) => bytesToHex(new Uint8Array(32).fill(n));
const pub = (sec: string) => bytesToHex(schnorr.getPublicKey(hexToBytes(sec)));
const CLIENT = hex(11), BUNKER = hex(12), JEFF = hex(13), STRANGER = hex(14);

function sign(sec: string, e: Omit<NostrEvent, 'id' | 'pubkey' | 'sig'>): NostrEvent {
  const pubkey = pub(sec);
  const id = buildEventId(serializeEvent(pubkey, e.created_at, e.kind, e.tags, e.content));
  return { ...e, id, pubkey, sig: bytesToHex(schnorr.sign(hexToBytes(id), hexToBytes(sec))) };
}

/** A relay in memory: published events go to every matching subscriber. */
function memoryRelay(): SignerTransport & { published: NostrEvent[] } {
  const subs: Array<{ f: Record<string, unknown>; cb: (e: NostrEvent) => void }> = [];
  const matches = (f: Record<string, unknown>, e: NostrEvent) =>
    (!f.kinds || (f.kinds as number[]).includes(e.kind))
    && (!f.authors || (f.authors as string[]).includes(e.pubkey))
    && (!f['#p'] || e.tags.some((t) => t[0] === 'p' && (f['#p'] as string[]).includes(t[1])));
  const relay = {
    published: [] as NostrEvent[],
    async publish(e: NostrEvent) { relay.published.push(e); for (const s of [...subs]) if (matches(s.f, e)) setTimeout(() => s.cb(e), 5); return { ok: true, reason: '' }; },
    subscribe(f: Record<string, unknown>, cb: (e: NostrEvent) => void) { const s = { f, cb }; subs.push(s); return () => { subs.splice(subs.indexOf(s), 1); }; },
  };
  return relay;
}

/** A stub signing service holding `userSec`; `tamper` lets a test make it misbehave. */
function bunker(relay: SignerTransport, userSec: string, tamper?: (e: NostrEvent) => NostrEvent, allowedClient = CLIENT) {
  relay.subscribe({ kinds: [NIP46_KIND], '#p': [pub(BUNKER)] }, (req) => {
    const ck = conversationKey(BUNKER, req.pubkey);
    const { id, params } = JSON.parse(decrypt(req.content, ck));
    const asked = JSON.parse(params[0]);
    // Silas 11:56: the service signs only for the room's client key, and only kind 9 posts.
    const refusal = req.pubkey !== pub(allowedClient) ? 'unknown client' : asked.kind !== 9 ? 'only kind 9 is signed' : '';
    let signed = sign(userSec, asked);
    if (tamper) signed = tamper(signed);
    const body = refusal ? { id, error: refusal } : { id, result: JSON.stringify(signed) };
    const reply = sign(BUNKER, { kind: NIP46_KIND, created_at: Math.floor(Date.now() / 1000), tags: [['p', req.pubkey]], content: encrypt(JSON.stringify(body), ck) });
    void relay.publish(reply);
  });
}

const post: UnsignedEvent = { kind: 9, created_at: 1_800_000_000, tags: [['h', '160e1f5d-6816-4c68-94b3-b5f1ade75565'], ['t', 'team']], content: '@silas hello from Clearing' };
const signer = (relay: SignerTransport, timeoutMs = 2000) => new RemoteSigner({ clientSecret: CLIENT, bunkerPubkey: pub(BUNKER), userPubkey: pub(JEFF), transport: relay, timeoutMs });

describe('#4652 Clearing asks the signing service to sign as Jeff', () => {
  test('the signed post is the one asked for, signed by Jeff\'s key', async () => {
    const relay = memoryRelay();
    bunker(relay, JEFF);
    const r = await signer(relay).signEvent(post);
    expect(r).toEqual({ ok: true, event: expect.objectContaining({ pubkey: pub(JEFF), content: post.content }) });
  });

  test('the request on the relay is encrypted: the post text never appears in the clear', async () => {
    const relay = memoryRelay();
    bunker(relay, JEFF);
    await signer(relay).signEvent(post);
    const req = relay.published.find((e) => e.pubkey === pub(CLIENT))!;
    expect(req.kind).toBe(NIP46_KIND);
    expect(req.content).not.toContain('hello from Clearing');
  });

  test('NEGATIVE PROOF: the signing service is down, so the answer is "not signed" with the reason', async () => {
    const r = await signer(memoryRelay(), 200).signEvent(post);
    expect(r).toEqual({ ok: false, reason: expect.stringContaining('did not answer') });
  });

  test('NEGATIVE PROOF: a service that signs with some other key is refused', async () => {
    const relay = memoryRelay();
    bunker(relay, STRANGER);
    expect(await signer(relay).signEvent(post)).toEqual({ ok: false, reason: expect.stringContaining("not Jeff's") });
  });

  test('NEGATIVE PROOF: a service that changes the text before signing is refused', async () => {
    const relay = memoryRelay();
    bunker(relay, JEFF, (e) => sign(JEFF, { kind: e.kind, created_at: e.created_at, tags: e.tags, content: 'something else' }));
    expect(await signer(relay).signEvent(post)).toEqual({ ok: false, reason: expect.stringContaining('different event') });
  });

  test('NEGATIVE PROOF: the service refuses anything but a kind 9 post, and the reason comes back', async () => {
    const relay = memoryRelay();
    bunker(relay, JEFF);
    expect(await signer(relay).signEvent({ ...post, kind: 1 })).toEqual({ ok: false, reason: 'signing service refused: only kind 9 is signed' });
  });

  test('NEGATIVE PROOF: the service refuses a client key it does not know', async () => {
    const relay = memoryRelay();
    bunker(relay, JEFF, undefined, STRANGER);
    expect(await signer(relay).signEvent(post)).toEqual({ ok: false, reason: 'signing service refused: unknown client' });
  });
});
