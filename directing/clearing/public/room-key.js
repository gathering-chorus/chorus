/* #3827 — Jeff's key, in Jeff's browser.
 *
 * Jeff's ruling (2026-08-11): "css pod is the top - all other integrations must
 * depend on that identity." So the key is not a second account: it is bound to
 * his WebID, and it is generated and used HERE, in the page. Nothing on the
 * server ever sees the private half.
 *
 * WHAT THIS IS AND IS NOT, plainly, because the difference is the whole card:
 * the private key is generated in this browser and stored in this browser. It
 * is genuinely his — no service holds it, and we cannot sign as him. It is NOT
 * yet portable: signing in from a second browser produces a second key until
 * the pod-storage half lands. Until then a new browser must be bound again,
 * and the join says so rather than pretending.
 *
 * The failure this file refuses to have: quietly minting a fresh key when
 * something goes wrong. That is exactly how two Jeffs ended up on the relay
 * this morning, and it looks like success from every angle except the one that
 * matters.
 */
import { schnorr } from '/vendor/@noble/curves/secp256k1.js';
import { sha256 as nobleSha256 } from '/vendor/@noble/hashes/sha256.js';

const STORE = 'chorus.room.key.v1';

const hex = (bytes) => Array.from(bytes).map((b) => b.toString(16).padStart(2, '0')).join('');
const bytes = (h) => new Uint8Array(h.match(/.{2}/g).map((b) => parseInt(b, 16)));
const utf8 = (s) => new TextEncoder().encode(s);

const sha256 = (data) => nobleSha256(data);

/**
 * The identity this browser holds for a given WebID.
 *
 * Keyed BY WebID: if a different person signs in on this machine, they do not
 * inherit the key. Two people sharing a browser is not exotic — it is Jeff
 * showing someone the app.
 */
function stored(webid) {
  try {
    const all = JSON.parse(localStorage.getItem(STORE) || '{}');
    return all[webid] || null;
  } catch {
    // Unreadable storage is "no key here", not an error worth breaking the page
    // over — the caller mints one and binds it.
    return null;
  }
}

function store(webid, sk) {
  const all = JSON.parse(localStorage.getItem(STORE) || '{}');
  all[webid] = sk;
  localStorage.setItem(STORE, JSON.stringify(all));
}

/**
 * Load this WebID's key, or make one.
 *
 * REFUSES when there is no WebID. An anonymous visitor has no identity to
 * bind a key to, and generating one anyway would create a key that belongs to
 * nobody while looking exactly like a key that belongs to Jeff.
 */
function loadOrCreate(webid) {
  if (!webid) {
    throw new Error('room-key: not signed in — no WebID to bind a key to. Sign in first; a key minted now would belong to nobody.');
  }
  const existing = stored(webid);
  if (existing) return { sk: bytes(existing), fresh: false };
  const sk = crypto.getRandomValues(new Uint8Array(32));
  store(webid, hex(sk));
  return { sk, fresh: true };
}

/** Schnorr public key for a secret. */
function pubkeyOf(sk) {
  return hex(schnorr.getPublicKey(sk));
}

/**
 * Sign a Nostr event in the browser. The serialization is NIP-01 canonical:
 * [0, pubkey, created_at, kind, tags, content]. created_at is computed once
 * and reused — computing it twice lets the signed and sent values differ by a
 * second, and the relay then rejects a signature that is actually correct
 * while complaining about crypto.
 */
function signEvent(sk, kind, tags, content) {
  const pubkey = pubkeyOf(sk);
  const created_at = Math.floor(Date.now() / 1000);
  const serialized = JSON.stringify([0, pubkey, created_at, kind, tags, content]);
  const id = hex(sha256(utf8(serialized)));
  const sig = hex(schnorr.sign(bytes(id), sk));
  return { id, pubkey, created_at, kind, tags, content, sig };
}

/**
 * Join the room: make sure this browser's key is bound to the signed-in
 * WebID, then keep it for signing.
 *
 * Only the PUBLIC key is ever sent. The POST is a binding request, not a
 * key handoff — if the server refuses it, we surface the refusal rather than
 * carrying on unbound, because an unbound key produces messages that render
 * as nobody.
 */
async function join(webid) {
  const { sk, fresh } = loadOrCreate(webid);
  const pubkey = pubkeyOf(sk);
  const res = await fetch('/api/room/bind', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ pubkey }),
  });
  if (!res.ok) {
    const detail = await res.text().catch(() => '');
    throw new Error('room-key: the server would not bind this key (' + res.status + ') ' + detail);
  }
  const binding = await res.json();
  return { pubkey, fresh, binding, sign: (kind, tags, content) => signEvent(sk, kind, tags, content) };
}

window.roomKey = { join, loadOrCreate, signEvent, pubkeyOf };

