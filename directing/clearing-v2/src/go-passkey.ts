// @domain: security
/**
 * #4707 AC1 slice c — Jeff's go, answered with his passkey. The cicd run that
 * stops at the go writes one challenge file per run (card, run, werk HEAD,
 * nonce); this page asks Jeff's authenticator to sign it with Touch ID and
 * hands the raw assertion to the go step. Clearing signs nothing and decides
 * nothing: werk-merge verifies the assertion (platform/services/jeff-go).
 */
import crypto from 'crypto';
import fs from 'fs';
import path from 'path';

/** The relying party every passkey is bound to, and the one origin a go may come from. */
export const GO_RP_ID = 'lightlifeurbangardens.com';
export const GO_ORIGIN = 'https://lightlifeurbangardens.com';

export interface GoChallenge { card: number; run_id: string; werk_head: string; nonce: string }

/** Same bytes as jeff_go::challenge: b64url(sha256("go|card|run|head|nonce")), no padding. */
export function challengeFor(c: GoChallenge): string {
  return crypto.createHash('sha256').update(`go|${c.card}|${c.run_id}|${c.werk_head}|${c.nonce}`).digest('base64url');
}

/** The newest challenge file for `card` in `dir`, or null. A file that names another card is ignored. */
export function readChallenge(dir: string, card: number): GoChallenge | null {
  let names: string[];
  try { names = fs.readdirSync(dir).filter((n) => n.endsWith('.json')); } catch { return null; }
  const found = names.map((n) => {
    const p = path.join(dir, n);
    try { return { c: JSON.parse(fs.readFileSync(p, 'utf8')) as GoChallenge, at: fs.statSync(p).mtimeMs }; } catch { return null; }
  }).filter((x): x is { c: GoChallenge; at: number } => !!x && x.c.card === card
    && [x.c.run_id, x.c.werk_head, x.c.nonce].every((v) => typeof v === 'string' && /^[A-Za-z0-9_-]{1,80}$/.test(v)));
  found.sort((a, b) => b.at - a.at);
  return found[0]?.c ?? null;
}

export interface GoAssertion { credentialId: string; clientDataJSON: string; authenticatorData: string; signature: string }

/** The assertion as the go step's input: every part b64url, nothing else carried. Null if malformed. */
export function assertionInput(body: unknown): string | null {
  const b = (body ?? {}) as Record<string, unknown>;
  const keys: Array<keyof GoAssertion> = ['credentialId', 'clientDataJSON', 'authenticatorData', 'signature'];
  if (!keys.every((k) => typeof b[k] === 'string' && /^[A-Za-z0-9_-]{8,8192}$/.test(b[k] as string))) return null;
  const a = Object.fromEntries(keys.map((k) => [k, b[k]])) as unknown as GoAssertion;
  return Buffer.from(JSON.stringify(a)).toString('base64url');
}

/** The dagu command that answers the go step with the assertion: no accepter, no decision typed by anyone. */
export function goCommand(daguConfig: string, c: GoChallenge, input: string): string[] {
  return ['--config', daguConfig, 'human-task', 'complete', 'cicd', '--run-id', c.run_id,
    '--step', 'skill_werk_go', '--input', `assertion=${input}`];
}

export function renderGoPage(c: GoChallenge, base: string): string {
  const esc = (s: string) => s.replace(/[&<>"]/g, (ch) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[ch] as string));
  return `<!doctype html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Go #${c.card}</title></head><body style="font-family:system-ui;margin:16px">
<h1>Go #${c.card}?</h1>
<p>Run <code>${esc(c.run_id)}</code> at <code>${esc(c.werk_head.slice(0, 12))}</code>. Touch ID signs this go for this card, this run and this commit only.</p>
<button id="go" style="font-size:1.4em;padding:12px 28px">Go</button> <span id="out"></span>
<script>
const b64 = (buf) => btoa(String.fromCharCode(...new Uint8Array(buf))).replace(/\\+/g,'-').replace(/\\//g,'_').replace(/=+$/,'');
const unb64 = (s) => Uint8Array.from(atob(s.replace(/-/g,'+').replace(/_/g,'/')), (c) => c.charCodeAt(0));
document.getElementById('go').onclick = async () => {
  const out = document.getElementById('out');
  try {
    const cred = await navigator.credentials.get({ publicKey: { rpId: ${JSON.stringify(GO_RP_ID)},
      challenge: unb64(${JSON.stringify(challengeFor(c))}), userVerification: 'required', timeout: 120000 } });
    const r = cred.response;
    const res = await fetch(${JSON.stringify(`${base}/api/go/${c.card}`)}, { method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ credentialId: b64(cred.rawId), clientDataJSON: b64(r.clientDataJSON), authenticatorData: b64(r.authenticatorData), signature: b64(r.signature) }) });
    const j = await res.json(); out.textContent = j.ok ? 'Sent. The land checks it.' : ('Refused: ' + j.error);
  } catch (e) { out.textContent = 'Not sent: ' + e.message; }
};
</script></body></html>`;
}

/** Where Jeff's passkey public key lives: root-owned, so no process running as his user can swap it (Silas 10:21). */
export const PASSKEY_FILE = '/Library/Application Support/Chorus/jeff-passkey.json';

/** SEC1 uncompressed point (65 bytes) from a P-256 SPKI, as getPublicKey() returns it. Null if it isn't one. */
export function sec1FromSpki(spki: Buffer): Buffer | null {
  const p = spki.subarray(spki.length - 65);
  return spki.length >= 91 && p[0] === 0x04 ? p : null;
}

/** The one install command Jeff runs: sudo, so it takes his password, which no agent has. */
export function installCommand(credentialId: string, sec1: Buffer): string {
  const json = JSON.stringify({ credentialId, publicKeySec1: sec1.toString('base64url'), signCount: 0, rpId: GO_RP_ID, origin: GO_ORIGIN });
  const dir = path.dirname(PASSKEY_FILE);
  return `sudo mkdir -p "${dir}" && echo '${json}' | sudo tee "${PASSKEY_FILE}" >/dev/null && sudo chown root:wheel "${dir}" "${PASSKEY_FILE}" && sudo chmod 0755 "${dir}" && sudo chmod 0644 "${PASSKEY_FILE}"`;
}

export function renderRegisterPage(): string {
  return `<!doctype html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Register passkey</title></head><body style="font-family:system-ui;margin:16px">
<h1>Your go passkey</h1>
<p>Once, on the Mac you give goes from. Touch ID makes a key that only this device can use. Nothing is sent anywhere: copy the command below and run it in your own terminal. It asks for your password.</p>
<button id="reg" style="font-size:1.2em;padding:10px 22px">Make my passkey</button>
<pre id="cmd" style="white-space:pre-wrap;word-break:break-all;background:#eee;padding:8px"></pre>
<script>
const b64 = (buf) => btoa(String.fromCharCode(...new Uint8Array(buf))).replace(/\\+/g,'-').replace(/\\//g,'_').replace(/=+$/,'');
document.getElementById('reg').onclick = async () => {
  const pre = document.getElementById('cmd');
  try {
    const cred = await navigator.credentials.create({ publicKey: {
      rp: { id: ${JSON.stringify(GO_RP_ID)}, name: 'Chorus go' },
      user: { id: new TextEncoder().encode('jeff'), name: 'jeff', displayName: 'Jeff' },
      challenge: crypto.getRandomValues(new Uint8Array(32)),
      pubKeyCredParams: [{ type: 'public-key', alg: -7 }],
      authenticatorSelection: { userVerification: 'required', residentKey: 'preferred' } } });
    const spki = new Uint8Array(cred.response.getPublicKey());
    const sec1 = spki.slice(spki.length - 65);
    if (cred.response.getPublicKeyAlgorithm() !== -7 || sec1[0] !== 4) throw new Error('not a P-256 key');
    const json = JSON.stringify({ credentialId: b64(cred.rawId), publicKeySec1: b64(sec1), signCount: 0, rpId: ${JSON.stringify(GO_RP_ID)}, origin: ${JSON.stringify(GO_ORIGIN)} });
    const f = ${JSON.stringify(PASSKEY_FILE)}, d = ${JSON.stringify(path.dirname(PASSKEY_FILE))};
    pre.textContent = 'sudo mkdir -p "' + d + '" && echo \\'' + json + '\\' | sudo tee "' + f + '" >/dev/null && sudo chown root:wheel "' + d + '" "' + f + '" && sudo chmod 0755 "' + d + '" && sudo chmod 0644 "' + f + '"';
  } catch (e) { pre.textContent = 'Not made: ' + e.message; }
};
</script></body></html>`;
}

/** Where the cicd run writes its go challenge; GO_CHALLENGE_DIR overrides (tests). */
export function goChallengeDir(env: NodeJS.ProcessEnv = process.env): string {
  return env.GO_CHALLENGE_DIR || path.join(env.CHORUS_HOME || path.join(env.HOME || '', 'CascadeProjects/chorus'), 'ops/go-challenges');
}

export function goDaguConfig(env: NodeJS.ProcessEnv = process.env): string {
  return env.GO_DAGU_CONFIG || path.join(env.CHORUS_HOME || path.join(env.HOME || '', 'CascadeProjects/chorus'), 'platform/config/dagu.yaml');
}

type GoRunner = (args: string[]) => Promise<void>;
let runnerOverride: GoRunner | null = null;
/** Tests replace the dagu call; nothing else does. */
export function setGoRunner(r: GoRunner | null): void { runnerOverride = r; }
export function goRunner(): GoRunner {
  if (runnerOverride) return runnerOverride;
  return (args) => new Promise((resolve, reject) => {
    // eslint-disable-next-line @typescript-eslint/no-var-requires
    require('child_process').execFile('dagu', args, { timeout: 20000 }, (err: Error | null) => (err ? reject(err) : resolve()));
  });
}
