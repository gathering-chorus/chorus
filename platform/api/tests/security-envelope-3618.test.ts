// @test-type: unit
/**
 * #3618 — the security envelope (hermetic tier).
 *
 * Core under test: decideEnvelope(request, deps) — the pure decision function
 * behind the Express adapter. Gates MODEL-DECLARED surfaces (the generated
 * table from the graph's APISurface instances). Deps injected: surface table,
 * signing secret, clock — this test brings its own world (#3528): no live
 * stack, no $HOME, no real spine, fixture secret.
 *
 * Contract (mirrors the door's semantics, #3573):
 *   - request matches a secured surface + no/invalid Bearer      → refuse 401
 *   - valid token, scope claim missing the surface requiresScope → refuse 403
 *   - valid token + scope                                        → pass
 *   - request matches no secured surface                         → pass, no events
 *   - every decision on a secured surface emits attempt + refuse/allow
 */
import * as crypto from 'crypto';
import {
  decideEnvelope,
  type EnvelopeRequest,
  type EnvelopeDeps,
  type SecuredSurface,
} from '../src/security-envelope';

const SECRET = 'test-secret-3618';
const NOW = 1_800_000_000; // fixed clock (secs)

function b64url(buf: Buffer | string): string {
  return Buffer.from(buf).toString('base64url');
}

function mintToken(opts: { scope?: string[]; expAt?: number; aud?: string } = {}): string {
  const header = b64url(JSON.stringify({ alg: 'HS256', typ: 'JWT' }));
  const claims = b64url(JSON.stringify({
    webId: 'http://localhost:3000/pods/chorus/_agents/reindex-worker/profile/card.ttl#me',
    aud: opts.aud ?? 'chorus',
    exp: opts.expAt ?? NOW + 300,
    scope: opts.scope ?? ['urn:chorus:index'],
  }));
  const sig = b64url(crypto.createHmac('sha256', SECRET).update(`${header}.${claims}`).digest());
  return `${header}.${claims}.${sig}`;
}

const SURFACES: SecuredSurface[] = [
  { method: 'POST', pathPrefix: '/api/chorus/reindex', requiresScope: 'urn:chorus:index', surface: 'surface-index-writes' },
  { method: 'POST', pathPrefix: '/api/athena/discover-', requiresScope: 'urn:chorus:domains:code', surface: 'surface-discover-writes' },
];

function deps(overrides: Partial<EnvelopeDeps> = {}): EnvelopeDeps {
  return { surfaces: SURFACES, secret: SECRET, nowSecs: () => NOW, ...overrides };
}

function req(overrides: Partial<EnvelopeRequest> = {}): EnvelopeRequest {
  return { method: 'POST', path: '/api/chorus/reindex', authorization: '', ...overrides };
}

describe('decideEnvelope (#3618)', () => {
  test('secured surface without Bearer → refuse 401 authn-missing + attempt/refused events', () => {
    const d = decideEnvelope(req(), deps());
    expect(d.action).toBe('refuse');
    expect(d.status).toBe(401);
    expect(d.body?.error).toBe('authn-missing');
    expect(d.events.map(e => e.event)).toEqual(
      ['security.envelope.attempt', 'security.envelope.refused']);
    expect(d.events[1].fields.surface).toBe('surface-index-writes');
  });

  test('valid token + right scope → pass + allowed event carries webId', () => {
    const d = decideEnvelope(req({ authorization: `Bearer ${mintToken()}` }), deps());
    expect(d.action).toBe('pass');
    expect(d.events.map(e => e.event)).toContain('security.envelope.allowed');
    const allowed = d.events.find(e => e.event === 'security.envelope.allowed');
    expect(allowed?.fields.webId).toContain('reindex-worker');
  });

  test('valid token, wrong scope → refuse 403 out-of-scope', () => {
    const d = decideEnvelope(
      req({ authorization: `Bearer ${mintToken({ scope: ['urn:chorus:domains:tests'] })}` }),
      deps());
    expect(d.action).toBe('refuse');
    expect(d.status).toBe(403);
    expect(d.body?.error).toBe('out-of-scope');
  });

  test('empty scope claim → refuse 403 (no legacy allow-all through the envelope)', () => {
    const d = decideEnvelope(req({ authorization: `Bearer ${mintToken({ scope: [] })}` }), deps());
    expect(d.action).toBe('refuse');
    expect(d.status).toBe(403);
  });

  test('expired token → refuse 401', () => {
    const d = decideEnvelope(
      req({ authorization: `Bearer ${mintToken({ expAt: NOW - 10 })}` }), deps());
    expect(d.action).toBe('refuse');
    expect(d.status).toBe(401);
  });

  test('forged signature → refuse 401', () => {
    const good = mintToken();
    const forged = good.slice(0, good.lastIndexOf('.') + 1) + b64url('not-a-real-sig');
    const d = decideEnvelope(req({ authorization: `Bearer ${forged}` }), deps());
    expect(d.action).toBe('refuse');
    expect(d.status).toBe(401);
  });

  test('wrong audience → refuse 401', () => {
    const d = decideEnvelope(
      req({ authorization: `Bearer ${mintToken({ aud: 'gathering' })}` }), deps());
    expect(d.action).toBe('refuse');
    expect(d.status).toBe(401);
  });

  test('prefix match secures the whole discover class', () => {
    const d = decideEnvelope(req({ path: '/api/athena/discover-code' }), deps());
    expect(d.action).toBe('refuse');
    expect(d.status).toBe(401);
    expect(d.events[0].fields.surface).toBe('surface-discover-writes');
  });

  test('unsecured surface passes untouched with zero events — mixed-state by construction', () => {
    const d = decideEnvelope(req({ path: '/api/cards/add' }), deps());
    expect(d.action).toBe('pass');
    expect(d.events).toHaveLength(0);
  });

  test('GET on a secured path prefix is not gated (mutation-only envelope, reads stay open)', () => {
    const d = decideEnvelope(req({ method: 'GET' }), deps());
    expect(d.action).toBe('pass');
    expect(d.events).toHaveLength(0);
  });

  test('empty surface table gates nothing (pre-generation boot safety)', () => {
    const d = decideEnvelope(req(), deps({ surfaces: [] }));
    expect(d.action).toBe('pass');
    expect(d.events).toHaveLength(0);
  });
});