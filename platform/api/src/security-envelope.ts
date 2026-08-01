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
import type { Request, Response, NextFunction, RequestHandler } from 'express';
import type { VerifyResult } from './es256-identity';

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
  /** #3719 — the ES256 identity verifier (es256-identity.ts). Owns JWKS,
   *  issuer, exp, and model-resolved scope; the envelope only decides. */
  verify: (token: string) => Promise<VerifyResult>;
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

/** The pure decision core — no Express, no I/O, fully injectable. */
export async function decideEnvelope(req: EnvelopeRequest, deps: EnvelopeDeps): Promise<EnvelopeDecision> {
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
  const result = token ? await deps.verify(token) : null;

  if (!result || !result.ok) {
    const retired = result?.reason === 'hs256-retired';
    const reason = retired ? 'hs256-retired' : 'authn-missing';
    events.push({
      event: 'security.envelope.refused',
      fields: { surface: match.surface, path: req.path, reason },
    });
    return {
      action: 'refuse',
      status: 401,
      body: retired
        ? { error: 'hs256-retired', message: 'HS256 service tokens are retired (#3719) — present a CSS ES256 identity token' }
        : { error: 'authn-missing', message: 'a valid Bearer identity token is required for this surface' },
      events,
    };
  }
  const claims = result;

  // Scope is the Principal's chorus:hasScope GRANTS from the model, not a
  // token claim (#3689/#3719): no grant = refuse, and the grant set must name
  // this surface's requirement.
  if (claims.scope.length === 0 || !claims.scope.includes(match.requiresScope)) {
    events.push({
      event: 'security.envelope.refused',
      fields: { surface: match.surface, path: req.path, reason: 'out-of-scope', webId: claims.webId },
    });
    return {
      action: 'refuse',
      status: 403,
      body: { error: 'out-of-scope', message: `this surface requires a Principal granted scope '${match.requiresScope}'` },
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
  verify: (token: string) => Promise<VerifyResult>;
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
    void (async () => {
      const decision = await decideEnvelope(
        { method: req.method, path: req.path, authorization: req.headers.authorization ?? '' },
        { surfaces: deps.getSurfaces(), verify: deps.verify },
      );
      for (const e of decision.events) deps.emit(e.event, e.fields);
      if (decision.action === 'refuse') {
        res.status(decision.status ?? 401).json(decision.body);
        return;
      }
      next();
    })().catch(() => {
      // a verifier crash must fail CLOSED on a secured surface, not hang the request
      res.status(401).json({ error: 'authn-error', message: 'identity verification failed' });
    });
  };
}
