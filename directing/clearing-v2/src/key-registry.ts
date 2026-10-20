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
 * Fills a shared keys map from the registry. Until the first read succeeds the map
 * holds no registry keys, so no post is attributed to anyone and nothing is handed
 * off (fail closed); a failed read retries on a timer. Keys a signed-in session
 * binds (/api/room/bind) live in the same map and are left alone.
 */
export class RegistryKeys {
  private timer: ReturnType<typeof setTimeout> | null = null;
  private stopped = false;

  constructor(
    private readonly keys: Map<string, string>,
    private readonly url: string = DEFAULT_KEYS_URL,
    private readonly retryMs = 60000,
  ) {}

  /** One read; resolves either way. False means the map is still empty of registry keys. */
  async start(): Promise<boolean> {
    try {
      const res = await fetch(this.url, { signal: AbortSignal.timeout(5000) });
      if (!res.ok) throw new Error(`key registry answered ${res.status}`);
      for (const [pubkey, who] of keysFromRegistry(await res.json())) this.keys.set(pubkey, who);
      return true;
    } catch (e) {
      console.error(`[clearing-v2] key registry not read, nobody is attributed yet: ${(e as Error).message}`);
      if (!this.stopped) this.timer = setTimeout(() => { void this.start(); }, this.retryMs);
      return false;
    }
  }

  stop(): void {
    this.stopped = true;
    if (this.timer) clearTimeout(this.timer);
  }
}
