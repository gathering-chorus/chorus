/* #3827 — Jeff's key, in Jeff's browser.
 *
 * Jeff's ruling (2026-08-11): "css pod is the top - all other integrations must
 * depend on that identity." So the key is not a second account: it is bound to
 * his WebID, and it is generated and used HERE, in the page. Nothing on the
 * server ever sees the private half.
 */
import { schnorr } from '/vendor/@noble/curves/secp256k1.js';
import { sha256 as nobleSha256 } from '/vendor/@noble/hashes/sha256.js';

const STORE = 'chorus.room.key.v1';

const hex = (bytes) => Array.from(bytes).map((b) => b.toString(16).padStart(2, '0')).join('');
const bytes = (h) => new Uint8Array(h.match(/.{2}/g).map((b) => parseInt(b, 16)));
const utf8 = (s) => new TextEncoder().encode(s);
const sha256 = (data) => nobleSha256(data);

function getStorage(customStorage) {
  if (customStorage) return customStorage;
  if (typeof localStorage !== 'undefined') return localStorage;
  return null;
}

function stored(webid, customStorage) {
  try {
    const storage = getStorage(customStorage);
    if (!storage) return null;
    const all = JSON.parse(storage.getItem(STORE) || '{}');
    return all[webid] || null;
  } catch {
    return null;
  }
}

function store(webid, skHex, customStorage) {
  const storage = getStorage(customStorage);
  if (!storage) return;
  const all = JSON.parse(storage.getItem(STORE) || '{}');
  all[webid] = skHex;
  storage.setItem(STORE, JSON.stringify(all));
}

export function loadOrCreate(webid, customStorage) {
  if (!webid) {
    throw new Error('room-key: not signed in — no WebID to bind a key to. Sign in first; a key minted now would belong to nobody.');
  }
  const existing = stored(webid, customStorage);
  if (existing) return { sk: bytes(existing), pubkey: pubkeyOf(bytes(existing)), fresh: false };
  const sk = crypto.getRandomValues(new Uint8Array(32));
  const skHex = hex(sk);
  store(webid, skHex, customStorage);
  return { sk, pubkey: pubkeyOf(sk), fresh: true };
}

export function pubkeyOf(sk) {
  return hex(schnorr.getPublicKey(sk));
}

export function signEvent(sk, kind, tags, content) {
  const pubkey = pubkeyOf(sk);
  const created_at = Math.floor(Date.now() / 1000);
  const serialized = JSON.stringify([0, pubkey, created_at, kind, tags, content]);
  const id = hex(sha256(utf8(serialized)));
  const sig = hex(schnorr.sign(bytes(id), sk));
  return { id, pubkey, created_at, kind, tags, content, sig };
}

export async function join(webid, customStorage) {
  const { sk, pubkey, fresh } = loadOrCreate(webid, customStorage);
  const res = await fetch('/api/room/bind', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ pubkey, webid }),
  });
  if (!res.ok) {
    const detail = await res.text().catch(() => '');
    throw new Error('room-key: the server would not bind this key (' + res.status + ') ' + detail);
  }
  const binding = await res.json();
  return { pubkey, fresh, binding, sign: (kind, tags, content) => signEvent(sk, kind, tags, content) };
}

if (typeof window !== 'undefined') {
  window.roomKey = { join, loadOrCreate, signEvent, pubkeyOf };

  (async function autoJoin() {
    const webid = window.CHORUS_WEBID || '';
    if (!webid) return;
    try {
      const joined = await join(webid);
      window.roomKeyStatus = {
        state: 'bound',
        pubkey: joined.pubkey,
        fresh: joined.fresh,
      };
    } catch (err) {
      window.roomKeyStatus = { state: 'failed', detail: String(err && err.message ? err.message : err) };
    }
  }());
}
