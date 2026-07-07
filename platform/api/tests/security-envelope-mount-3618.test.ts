// @test-type: unit
/**
 * #3618 — the Express adapter mount (hermetic tier).
 *
 * Covers the deploy-safety contract of securityEnvelope() as mounted in
 * server.ts: OFF by default (pure pass-through so a deploy can't 401 live
 * workers), a live-swappable surface table (loaded async at boot), and events
 * only when enabled + a surface matches.
 *
 * Own world (#3528): in-process express app on an ephemeral port, injected
 * secret/clock/emit — no live stack.
 */
import express from 'express';
import * as crypto from 'crypto';
import { AddressInfo } from 'net';
import { securityEnvelope, type SecuredSurface } from '../src/security-envelope';

const SECRET = 'test-secret-3618';
const NOW = 1_800_000_000;

function b64url(s: Buffer | string): string { return Buffer.from(s).toString('base64url'); }
function token(scope: string[] = ['urn:chorus:index']): string {
  const h = b64url(JSON.stringify({ alg: 'HS256', typ: 'JWT' }));
  const c = b64url(JSON.stringify({ webId: 'x', aud: 'chorus', exp: NOW + 300, scope }));
  const s = b64url(crypto.createHmac('sha256', SECRET).update(`${h}.${c}`).digest());
  return `${h}.${c}.${s}`;
}

const SURFACE: SecuredSurface = {
  method: 'POST', pathPrefix: '/api/chorus/reindex', requiresScope: 'urn:chorus:index', surface: 'surface-index-writes',
};

function makeApp(opts: { enabled: boolean; surfaces: SecuredSurface[] }) {
  const app = express();
  const state = { enabled: opts.enabled, surfaces: opts.surfaces };
  const events: string[] = [];
  app.use(securityEnvelope({
    getSurfaces: () => state.surfaces,
    secret: SECRET,
    nowSecs: () => NOW,
    emit: (e) => events.push(e),
    enabled: state.enabled,
  }));
  app.post('/api/chorus/reindex', (_req, res) => res.json({ ok: true }));
  return { app, state, events };
}

describe('securityEnvelope mount (#3618)', () => {
  test('disabled (default deploy state): secured path passes through, zero events', async () => {
    const { app, events } = makeApp({ enabled: false, surfaces: [SURFACE] });
    const server = app.listen(0);
    const base = `http://127.0.0.1:${(server.address() as AddressInfo).port}`;
    const res = await fetch(`${base}/api/chorus/reindex`, { method: 'POST' });
    expect(res.status).toBe(200);
    expect(events).toHaveLength(0);
    await new Promise<void>((r) => server.close(() => r()));
  });

  test('enabled + no token → 401 (the flip is live)', async () => {
    const { app, events } = makeApp({ enabled: true, surfaces: [SURFACE] });
    const server = app.listen(0);
    const base = `http://127.0.0.1:${(server.address() as AddressInfo).port}`;
    const res = await fetch(`${base}/api/chorus/reindex`, { method: 'POST' });
    expect(res.status).toBe(401);
    expect(events).toContain('security.envelope.refused');
    await new Promise<void>((r) => server.close(() => r()));
  });

  test('enabled + valid scoped token → 200', async () => {
    const { app } = makeApp({ enabled: true, surfaces: [SURFACE] });
    const server = app.listen(0);
    const base = `http://127.0.0.1:${(server.address() as AddressInfo).port}`;
    const res = await fetch(`${base}/api/chorus/reindex`, {
      method: 'POST', headers: { Authorization: `Bearer ${token()}` },
    });
    expect(res.status).toBe(200);
    await new Promise<void>((r) => server.close(() => r()));
  });

  test('enabled but table not yet loaded (empty) → passes through (boot-race safety)', async () => {
    const { app } = makeApp({ enabled: true, surfaces: [] });
    const server = app.listen(0);
    const base = `http://127.0.0.1:${(server.address() as AddressInfo).port}`;
    const res = await fetch(`${base}/api/chorus/reindex`, { method: 'POST' });
    expect(res.status).toBe(200); // no surface loaded yet → nothing to gate
    await new Promise<void>((r) => server.close(() => r()));
  });

  test('live table swap takes effect without re-mount', async () => {
    const { app, state } = makeApp({ enabled: true, surfaces: [] });
    const server = app.listen(0);
    const base = `http://127.0.0.1:${(server.address() as AddressInfo).port}`;
    let res = await fetch(`${base}/api/chorus/reindex`, { method: 'POST' });
    expect(res.status).toBe(200); // empty table
    state.surfaces = [SURFACE];   // async boot-load completes
    res = await fetch(`${base}/api/chorus/reindex`, { method: 'POST' });
    expect(res.status).toBe(401); // now gated, same middleware instance
    await new Promise<void>((r) => server.close(() => r()));
  });
});