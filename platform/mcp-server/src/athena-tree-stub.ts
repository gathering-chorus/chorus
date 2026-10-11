// #3025 AC6 / #4235 — the Athena lookups are served by chorus-api, which reads
// the live graph (products, domains and services as the model holds them):
//   /api/athena/tree                 (full tree)
//   /api/athena/ownership/:iri       (owner + containing path)
//   /api/athena/blast-radius/:iri    (consumers / dependents / hosts / files)
// These routes return the result object directly (no {data} envelope).
//
// Function signatures are preserved so the server.ts handlers compile unchanged.
// An injectable getter (__setAthenaGetter) is the test seam.

const CHORUS_API_URL = process.env.CHORUS_API_URL || 'http://localhost:3340';

import { execFileSync } from 'child_process';

type AthenaGetter = (path: string) => unknown;

// Default getter: synchronous curl over loopback. Athena lookups are rare;
// the sync shape preserves the call-site signatures the handlers were built on.
function curlGet(path: string): unknown {
  try {
    const out = execFileSync('curl', ['-s', '--max-time', '5', `${CHORUS_API_URL}${path}`], {
      encoding: 'utf-8',
    });
    return JSON.parse(out);
  } catch (err) {
    throw new Error(`chorus-api ${path} unreachable: ${(err as Error).message}`, { cause: err });
  }
}

let getter: AthenaGetter = curlGet;

/** Test seam — swap the HTTP getter. */
export function __setAthenaGetter(g: AthenaGetter | null): void {
  getter = g ?? curlGet;
}

// Opaque handle, kept for signature compatibility (lookups ignore it — the
// per-call HTTP getter is the data source).
export type TreeHandle = Record<string, never>;
export function loadTree(): TreeHandle {
  return {};
}

/** Full structural tree from the graph — backs chorus_tree_get. */
export function getTree(): unknown {
  return getter('/api/athena/tree');
}

/**
 * Who owns this IRI + where it sits — ownership from the graph.
 * Returns the chorus-api ownership object ({ iri, kind, owner, product, domain?, service? }).
 * null = not found (caller must surface not-found, never a confidently-wrong answer).
 */
export function lookupOwnership(_tree: TreeHandle, iri: string): unknown {
  const res = getter(`/api/athena/ownership/${encodeURIComponent(iri)}`) as
    | { iri?: string; ok?: boolean }
    | null
    | undefined;
  if (!res || (res as { ok?: boolean }).ok === false || !res.iri) return null;
  return res;
}

export type BlastRadiusResult = { consumers: unknown[]; [k: string]: unknown };

/** Inferred blast-radius from the graph. null = not found. */
export function computeBlastRadius(_tree: TreeHandle, iri: string): BlastRadiusResult | null {
  // #4499 — a repo path (file or code unit) or a service answers from the code
  // units: services, domains, tests and pages, never a file list
  if (iri.includes('/') || /^(chorus:)?service-/.test(iri)) {
    const code = getter(`/api/athena/code-blast?target=${encodeURIComponent(iri)}`) as Record<string, unknown> | null | undefined;
    return code ? ({ consumers: [], ...code } as BlastRadiusResult) : null;
  }
  const res = getter(`/api/athena/blast-radius/${encodeURIComponent(iri)}`) as
    | { consumers?: unknown[] }
    | null
    | undefined;
  if (!res || !Array.isArray(res.consumers)) return null;
  return res as BlastRadiusResult;
}
