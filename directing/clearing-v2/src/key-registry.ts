// @domain: messages
// #4652 — whose key is whose, from the security graph's key registry only. Before
// this the room read ~/.chorus/identity/<who>/nostr.json, and on 10-20 Jeff's file
// held ee70fd19, a key bound to no one, while the registry binds his Buzz app key
// a42ccebe. Silas 07:44: graph only; a local file must never name anyone. The room's
// own signing key (relay-signer.ts) is not affected: this maps pubkeys to people.

export const DEFAULT_KEYS_URL = 'http://localhost:3340/owl/v1/identity/keyregistryentries';

/** The registry's type for a Buzz (nostr) public key. */
export const NOSTR_TYPE = 'nostr-secp256k1';
const PUBKEY_HEX = /^[0-9a-f]{64}$/i;
const PRINCIPAL = /^principal-([a-z][a-z0-9-]{0,31})$/;

/** The registry's nostr rows as pubkey → principal name. Anything else names no one. */
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

/**
 * Fills a shared keys map from the registry, and re-reads it so a revoked key stops
 * speaking (Silas 10-20 07:51). Until the first read succeeds the map holds no
 * registry keys, so no post is attributed and nothing is handed off (fail closed);
 * a failed read retries sooner. Keys a signed-in session binds (/api/room/bind)
 * share the map and are never dropped by a re-read.
 */
export class RegistryKeys {
  private timer: ReturnType<typeof setTimeout> | null = null;
  private stopped = false;
  private fromRegistry = new Set<string>();

  constructor(
    private readonly keys: Map<string, string>,
    private readonly url: string = DEFAULT_KEYS_URL,
    private readonly retryMs = 60000,
    private readonly refreshMs = 600000,
  ) {}

  /** One read, then the next one scheduled; resolves either way. False means this read failed. */
  async start(): Promise<boolean> {
    let ok = false;
    try {
      const res = await fetch(`${this.url}${this.url.includes('?') ? '&' : '?'}limit=1000`, { signal: AbortSignal.timeout(5000) });
      if (!res.ok) throw new Error(`key registry answered ${res.status}`);
      this.replace(keysFromRegistry(await res.json()));
      ok = true;
    } catch (e) {
      console.error(`[clearing-v2] key registry not read: ${(e as Error).message}`);
    }
    if (!this.stopped) {
      this.timer = setTimeout(() => { void this.start(); }, ok ? this.refreshMs : this.retryMs);
      this.timer.unref(); // the re-read never keeps a process alive on its own
    }
    return ok;
  }

  stop(): void {
    this.stopped = true;
    if (this.timer) clearTimeout(this.timer);
  }

  private replace(next: Map<string, string>): void {
    for (const pubkey of this.fromRegistry) if (!next.has(pubkey)) this.keys.delete(pubkey);
    for (const [pubkey, who] of next) this.keys.set(pubkey, who);
    this.fromRegistry = new Set(next.keys());
  }
}
