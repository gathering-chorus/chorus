// @test-type: integration:api
// @domain: security
// @card: #4707
// @owner: wren
// #4707 AC1 slices c and d — Jeff's go page and passkey registration
// (directing/clearing-v2/src/go-passkey.ts, routes in directing/clearing-v2/src/server.ts).
// Clearing carries the assertion and decides nothing; werk-merge verifies it.
import fs from 'fs';
import os from 'os';
import path from 'path';
import http from 'http';
import { challengeFor, readChallenge, assertionInput, goCommand, setGoRunner, sec1FromSpki, installCommand, PASSKEY_FILE } from '../src/go-passkey';
import { createApp } from '../src/server';

const C = { card: 4707, run_id: '034ktQIUdeT16HznvkjFIn', werk_head: '6e0a5fee94a5', nonce: 'n-1' };
const b64 = (s: string) => Buffer.from(s).toString('base64url');
const GOOD = { credentialId: b64('credential-1'), clientDataJSON: b64('{"type":"webauthn.get"}'), authenticatorData: b64('authdata-bytes'), signature: b64('signature-bytes') };

function dirWith(files: Record<string, unknown>): string {
  const d = fs.mkdtempSync(path.join(os.tmpdir(), 'go-4707-'));
  for (const [n, v] of Object.entries(files)) fs.writeFileSync(path.join(d, n), JSON.stringify(v));
  return d;
}

describe('#4707 the go challenge', () => {
  it('matches jeff_go::challenge byte for byte (cross-language vector)', () => {
    expect(challengeFor(C)).toBe('oX36ie-6fcYvZgJu7DR8Kc_LIkUg5M9wHK7NoHpjEH0');
  });

  it('negative proof: any other card, run, HEAD or nonce gives another challenge', () => {
    for (const other of [{ ...C, card: 4702 }, { ...C, run_id: 'x034kq648m' }, { ...C, werk_head: 'b08dccd24fc4' }, { ...C, nonce: 'n-2' }]) {
      expect(challengeFor(other)).not.toBe(challengeFor(C));
    }
  });

  it('reads the waiting challenge for this card only, and refuses odd values', () => {
    const d = dirWith({ 'a.json': C, 'b.json': { ...C, card: 4702 }, 'c.json': { ...C, card: 4707, nonce: 'bad nonce;rm' } });
    const got = readChallenge(d, 4707);
    expect(got?.nonce).toBe('n-1');
    expect(readChallenge(d, 9999)).toBeNull();
    expect(readChallenge('/nonexistent-4707', 4707)).toBeNull();
  });
});

describe('#4707 the assertion handed to the go step', () => {
  it('carries the four b64url parts and nothing else', () => {
    const input = assertionInput({ ...GOOD, accepter: 'jeff', decision: 'go' });
    expect(JSON.parse(Buffer.from(input as string, 'base64url').toString())).toEqual(GOOD);
  });

  it('negative proof: a missing part or a non-b64url value is refused', () => {
    expect(assertionInput({ ...GOOD, signature: undefined })).toBeNull();
    expect(assertionInput({ ...GOOD, clientDataJSON: 'not b64 at all!' })).toBeNull();
    expect(assertionInput('accepter=jeff')).toBeNull();
  });

  it('the go command names no accepter and no decision', () => {
    const args = goCommand('/x/dagu.yaml', C, 'abc123def');
    expect(args).toContain('assertion=abc123def');
    expect(args.join(' ')).not.toMatch(/accepter|decision/);
    expect(args).toEqual(expect.arrayContaining(['--run-id', C.run_id, '--step', 'skill_werk_go']));
  });
});

describe('#4707 registration', () => {
  it('takes the SEC1 point from a P-256 SPKI and builds a sudo install into the root-owned file', () => {
    const spki = Buffer.concat([Buffer.alloc(26, 1), Buffer.from([4]), Buffer.alloc(64, 7)]);
    const sec1 = sec1FromSpki(spki) as Buffer;
    expect(sec1.length).toBe(65);
    const cmd = installCommand('Y3JlZA', sec1);
    expect(cmd.startsWith('sudo ')).toBe(true);
    expect(cmd).toContain(PASSKEY_FILE);
    expect(cmd).toContain('chown root:wheel');
  });

  it('negative proof: a key that is not an uncompressed P-256 point is refused', () => {
    expect(sec1FromSpki(Buffer.alloc(91, 2))).toBeNull();
    expect(sec1FromSpki(Buffer.alloc(40, 4))).toBeNull();
  });
});

describe('#4707 routes', () => {
  let server: http.Server; let base: string; let calls: string[][];
  beforeAll((done) => {
    process.env.GO_CHALLENGE_DIR = dirWith({ 'run.json': C });
    calls = [];
    setGoRunner(async (args) => { calls.push(args); });
    const { app } = createApp({ relayUrl: '', knownKeys: new Map() });
    server = app.listen(0, () => { base = `http://127.0.0.1:${(server.address() as { port: number }).port}`; done(); });
  });
  afterAll((done) => { setGoRunner(null); delete process.env.GO_CHALLENGE_DIR; server.close(() => done()); });

  it('serves the go page with this run\'s challenge, and 404 for a card with no go waiting', async () => {
    const page = await (await fetch(`${base}/go/4707`)).text();
    expect(page).toContain(challengeFor(C));
    expect(page).toContain("userVerification: 'required'");
    expect((await fetch(`${base}/go/4702`)).status).toBe(404);
  });

  it('posts a well-formed assertion to the go step, and refuses a malformed one without calling dagu', async () => {
    const ok = await fetch(`${base}/api/go/4707`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(GOOD) });
    expect(((await ok.json()) as { ok: boolean }).ok).toBe(true);
    expect(calls).toHaveLength(1);
    const bad = await fetch(`${base}/api/go/4707`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ accepter: 'jeff' }) });
    expect(bad.status).toBe(400);
    expect(calls).toHaveLength(1);
  });

  it('serves the registration page, which sends nothing to the server', async () => {
    const page = await (await fetch(`${base}/passkey/register`)).text();
    expect(page).toContain('navigator.credentials.create');
    expect(page).not.toMatch(/fetch\(/);
  });
});
