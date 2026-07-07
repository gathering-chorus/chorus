/**
 * #3618 — the security envelope: model-declared surfaces get a generated gate.
 *
 * Architecture (the #3414 seam extended beyond owl-api's own routes):
 *   - The MODEL declares which surfaces are secured (APISurface instances with
 *     securedBy + requiresScope in urn:chorus:ontology).
 *   - The GENERATOR projects that into a data table (generated/security-surfaces.json)
 *     — generated data, committed in the repo, drift-checked against the graph.
 *   - THIS module is the one hand-written engine: a pure decision function
 *     (decideEnvelope) + a thin Express adapter (securityEnvelope). Verification
 *     mirrors the door's semantics (#3573 / owl-api auth.rs): HS256, aud=chorus,
 *     exp, non-empty scope that names the surface's requiresScope.
 *
 * Mutation-only: reads stay open by design. Mixed-state by construction: a
 * surface with no table entry passes untouched — the graph shows what's gated
 * (securedBy edge) and what isn't, so #3619 can query its own remaining work.
 */
import * as crypto from 'crypto';
import type { Request, Response, NextFunction, RequestHandler } from 'express';

export interface SecuredSurface {
  method: string;       // mutation verb: POST | PUT | DELETE | PATCH
  pathPrefix: string;   // request path prefix, e.g. /api/athena/discover-
  requiresScope: string;
  surface: string;      // the APISurface instance slug (spine event field)
}

export interface EnvelopeRequest {
  method: string;
  path: string;
  authorization: string; // raw Authorization header value ('' when absent)
}

export interface EnvelopeDeps {
  surfaces: SecuredSurface[];
  secret: string;
  nowSecs: () => number;
}

export interface EnvelopeEvent {
  event: string;
  fields: Record<string, string>;
}

export interface EnvelopeDecision {
  action: 'pass' | 'refuse';
  status?: number;
  body?: { error: string; message: string };
  events: EnvelopeEvent[];
}

interface Claims {
  webId: string;
  aud: string;
  exp: number;
  scope: string[];
}

function b64urlDecode(s: string): Buffer {
  return Buffer.from(s, 'base64url');
}

/** Verify an HS256 service token; null on ANY failure (fail closed, no reasons leaked). */
function verifyToken(token: string, secret: string, nowSecs: number): Claims | null {
  const parts = token.split('.');
  if (parts.length !== 3) return null;
  const [header, payload, sig] = parts;
  const expected = crypto.createHmac('sha256', secret).update(`${header}.${payload}`).digest();
  const got = b64urlDecode(sig);
  if (got.length !== expected.length || !crypto.timingSafeEqual(got, expected)) return null;
  let claims: Partial<Claims>;
  try {
    claims = JSON.parse(b64urlDecode(payload).toString('utf-8'));
  } catch {
    return null;
  }
  if (claims.aud !== 'chorus') return null;
  if (typeof claims.exp !== 'number' || claims.exp <= nowSecs) return null;
  if (typeof claims.webId !== 'string' || claims.webId.length === 0) return null;
  const scope = Array.isArray(claims.scope) ? claims.scope.filter((s) => typeof s === 'string') : [];
  return { webId: claims.webId, aud: claims.aud, exp: claims.exp, scope };
}

/** The pure decision core — no Express, no I/O, fully injectable. */
export function decideEnvelope(req: EnvelopeRequest, deps: EnvelopeDeps): EnvelopeDecision {
  const match = deps.surfaces.find(
    (s) => s.method === req.method && req.path.startsWith(s.pathPrefix),
  );
  if (!match) return { action: 'pass', events: [] };

  const events: EnvelopeEvent[] = [
    { event: 'security.envelope.attempt', fields: { surface: match.surface, path: req.path } },
  ];

  const token = req.authorization.startsWith('Bearer ')
    ? req.authorization.slice(7)
    : req.authorization.startsWith('bearer ')
      ? req.authorization.slice(7)
      : '';
  const claims = token ? verifyToken(token, deps.secret, deps.nowSecs()) : null;

  if (!claims) {
    events.push({
      event: 'security.envelope.refused',
      fields: { surface: match.surface, path: req.path, reason: 'authn-missing' },
    });
    return {
      action: 'refuse',
      status: 401,
      body: { error: 'authn-missing', message: 'a valid Bearer service-token is required for this surface' },
      events,
    };
  }

  // The envelope never honors a legacy/unscoped token (same Wren gate as /batch):
  // empty scope = refuse, and the scope set must name this surface's requirement.
  if (claims.scope.length === 0 || !claims.scope.includes(match.requiresScope)) {
    events.push({
      event: 'security.envelope.refused',
      fields: { surface: match.surface, path: req.path, reason: 'out-of-scope', webId: claims.webId },
    });
    return {
      action: 'refuse',
      status: 403,
      body: { error: 'out-of-scope', message: `this surface requires a token scoped to '${match.requiresScope}'` },
      events,
    };
  }

  events.push({
    event: 'security.envelope.allowed',
    fields: { surface: match.surface, path: req.path, webId: claims.webId },
  });
  return { action: 'pass', events };
}

export interface EnvelopeAdapterDeps {
  // The surface table is read per-request via a getter so the server can load
  // it asynchronously at boot and swap it in without re-mounting the middleware.
  getSurfaces: () => SecuredSurface[];
  secret: string;
  nowSecs: () => number;
  emit: (event: string, fields: Record<string, string>) => void;
  // Deploy-before-require: default OFF. The gate goes live only when the flip
  // step sets this true, AFTER the surface's consumers carry credentials.
  // Off = pure pass-through, zero events (mounting can't break live workers).
  enabled: boolean;
}

/** Express adapter: mount early with app.use(securityEnvelope(deps)). */
export function securityEnvelope(deps: EnvelopeAdapterDeps): RequestHandler {
  return (req: Request, res: Response, next: NextFunction) => {
    if (!deps.enabled) { next(); return; }
    const decision = decideEnvelope(
      { method: req.method, path: req.path, authorization: req.headers.authorization ?? '' },
      { surfaces: deps.getSurfaces(), secret: deps.secret, nowSecs: deps.nowSecs },
    );
    for (const e of decision.events) deps.emit(e.event, e.fields);
    if (decision.action === 'refuse') {
      res.status(decision.status ?? 401).json(decision.body);
      return;
    }
    next();
  };
}
