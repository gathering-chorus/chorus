/**
 * #3619 AC4 — the sweep: prove "zero unauthenticated mutation endpoints"
 * instead of tracking it by hand.
 *
 * Pure logic only (this module): parse the mutation routes chorus-api actually
 * declares (server.ts is the deployed truth), then classify each against the
 * model-projected secured-surface table (security-surfaces-emit.ts) and the
 * exemptions ledger (proving/security/sweep-exemptions.json — every unflipped
 * endpoint carries a reason and a card, so remaining debt is visible, never
 * silent). The live prober (platform/scripts/security-sweep) feeds this real
 * inputs and 401-probes the secured set; CI runs the ratchet via the unit
 * suite + committed artifacts.
 *
 * Done-state (#3619 AC4): unprotected = [] AND exempted = [] — the sweep is
 * the truth-teller for card-done.
 */
import type { SecuredSurface } from './security-envelope';

export interface MutationRoute {
  method: 'POST' | 'PUT' | 'DELETE' | 'PATCH';
  path: string;
}

export interface SweepExemption {
  method: string;
  path: string;
  reason: string;
  card: number;
}

export interface SweepGap {
  secured: MutationRoute[];
  exempted: MutationRoute[];
  unprotected: MutationRoute[];
  staleExemptions: SweepExemption[];
}

const ROUTE_RE = /^\s*app\.(post|put|delete|patch)\(\s*['"]([^'"]+)['"]/;

/** Parse mutation-route declarations from Express server source. Line-anchored:
 *  a commented-out `// app.post(...)` never counts. */
export function parseMutationRoutes(serverSrc: string): MutationRoute[] {
  const out: MutationRoute[] = [];
  for (const line of serverSrc.split('\n')) {
    const m = ROUTE_RE.exec(line);
    if (m) {
      out.push({ method: m[1].toUpperCase() as MutationRoute['method'], path: m[2] });
    }
  }
  return out;
}

function surfaceCovers(s: SecuredSurface, r: MutationRoute): boolean {
  const methodOk = s.method === '*' || s.method.toUpperCase() === r.method;
  return methodOk && r.path.startsWith(s.pathPrefix);
}

function exemptionCovers(e: SweepExemption, r: MutationRoute): boolean {
  return e.method.toUpperCase() === r.method && e.path === r.path;
}

/** Classify every mutation route: secured (a surface gates it), exempted
 *  (ledger entry with reason+card), or unprotected (the red set). Exemptions
 *  that match no live route are surfaced as stale so the ledger can't rot. */
export function classifyEndpoints(
  routes: MutationRoute[],
  surfaces: SecuredSurface[],
  exemptions: SweepExemption[],
): SweepGap {
  const gap: SweepGap = { secured: [], exempted: [], unprotected: [], staleExemptions: [] };
  for (const r of routes) {
    if (surfaces.some(s => surfaceCovers(s, r))) {
      gap.secured.push(r);
    } else if (exemptions.some(e => exemptionCovers(e, r))) {
      gap.exempted.push(r);
    } else {
      gap.unprotected.push(r);
    }
  }
  for (const e of exemptions) {
    if (!routes.some(r => exemptionCovers(e, r))) {
      gap.staleExemptions.push(e);
    }
  }
  return gap;
}
