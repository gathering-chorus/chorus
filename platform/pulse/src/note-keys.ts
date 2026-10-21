// @domain: messages
/**
 * #4629 AC4 — whose key signed a room note. The answer comes from the key
 * registry (/owl/v1/identity/keyregistryentries, nostrPubkey per principal),
 * the same rows Clearing v2 reads since #4652 — never local files. A revoked
 * key is a removed row, so a re-read drops it. Fail closed (Silas 06:33):
 * with no registry read yet, a note is unverified, never valid.
 */
export const NOSTR_TYPE = 'nostr-secp256k1';
const PUBKEY_HEX = /^[0-9a-f]{64}$/i;
const PRINCIPAL = /^principal-([a-z][a-z0-9-]{0,31})$/;

/** The registry's nostr rows as pubkey → principal name (same rule as clearing-v2 key-registry.ts). */
export function keysFromRegistry(body: unknown): Map<string, string> {
  const keys = new Map<string, string>();
  const rows = (body as { data?: unknown } | null)?.data;
  if (!Array.isArray(rows)) return keys;
  for (const r of rows as Array<Record<string, unknown>>) {
    const who = typeof r.forPrincipal === 'string' ? PRINCIPAL.exec(r.forPrincipal)?.[1] : undefined;
    const pubkey = r.nostrPubkey;
    if (r.keyType !== NOSTR_TYPE || !who || typeof pubkey !== 'string' || !PUBKEY_HEX.test(pubkey)) continue;
    keys.set(pubkey.toLowerCase(), who);
  }
  return keys;
}

export type KeyVerdict = 'ok' | 'wrong-key' | 'unverified' | 'unknown-key';

/**
 * A note is signed rightly when its pubkey is the registry key of the role its
 * author tag names. Until #4629 S3 retires pulse's bridge copies, a note the
 * bridge signed is also right, but only as a real bridge copy (msg pulse-<id>).
 */
export function keyVerdict(pubkey: string, author: string | undefined, keys: Map<string, string> | null, msg = ''): KeyVerdict {
  if (!keys) return 'unverified';
  const who = keys.get(pubkey.toLowerCase());
  if (!who) return 'unknown-key';
  // a real bridge copy only (msg pulse-<id>); a bridge-signed sender note is wrong (Silas 06:36)
  if (who === 'bridge') return /^pulse-\d+$/.test(msg) ? 'ok' : 'wrong-key';
  return who === author ? 'ok' : 'wrong-key';
}

export interface NoteKeysOptions {
  /** re-read period (default 5 min) */
  everyMs?: number;
  /** a read older than this is not trusted: unverified, so a revoked key stops being ok (default 30 min) */
  staleMs?: number;
  /** at most one extra read per this window for an unknown key, so a junk flood is not a read per note (default 1 min) */
  unknownEveryMs?: number;
  now?: () => number;
}

/** The registry, read at start, every 5 min, and on an unknown key at most once a minute (Silas 06:36). */
export class NoteKeys {
  private keys: Map<string, string> | null = null;
  private readAt = 0;
  private lastUnknownRead = -Infinity;
  private timer: ReturnType<typeof setInterval> | null = null;
  private readonly now: () => number;

  constructor(private readonly fetchBody: () => Promise<unknown>, private readonly o: NoteKeysOptions = {}) {
    this.now = o.now ?? Date.now;
    void this.refresh();
    const every = o.everyMs ?? 300_000;
    if (every > 0) { this.timer = setInterval(() => void this.refresh(), every); this.timer.unref(); }
  }

  async refresh(): Promise<void> {
    try {
      const keys = keysFromRegistry(await this.fetchBody());
      // an answer with no keys is not a reason to keep old ones: a registry emptied of rows
      // revokes them all, so it reads unverified, never the stale set (abby review 07:43)
      this.keys = keys.size > 0 ? keys : null;
      this.readAt = this.now();
    } catch { /* no answer: keep the last good read until it goes stale; none yet means unverified */ }
  }

  private current(): Map<string, string> | null {
    return this.keys && this.now() - this.readAt <= (this.o.staleMs ?? 1_800_000) ? this.keys : null;
  }

  /** The verdict, re-reading the registry (rate-limited) before calling an unknown key wrong. */
  async check(pubkey: string, author: string | undefined, msg = ''): Promise<Exclude<KeyVerdict, 'unknown-key'>> {
    let v = keyVerdict(pubkey, author, this.current(), msg);
    if (v === 'unknown-key' && this.now() - this.lastUnknownRead >= (this.o.unknownEveryMs ?? 60_000)) {
      this.lastUnknownRead = this.now();
      await this.refresh();
      v = keyVerdict(pubkey, author, this.current(), msg);
    }
    return v === 'unknown-key' ? 'wrong-key' : v;
  }

  stop(): void { if (this.timer) clearInterval(this.timer); }
}
