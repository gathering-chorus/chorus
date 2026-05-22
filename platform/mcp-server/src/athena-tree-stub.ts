// #3025 AC6 — Athena lookups read the v2 JSON tree, NOT the AS-IS Fuseki surface.
//
// CORRECTED 2026-05-22: a prior pass had v1/v2 inverted. It repointed these
// lookups to /api/athena/subdomains[/:id[/blast-radius]] — which the design
// (designing/docs/athena-subproduct-design.html, line 541) names as the AS-IS
// surface that "Athena v2 replaces", a 4-field Fuseki record with no products.
// That dropped every product that lives only in the v2 JSON tree — e.g. The
// Clearing (owner Wren) — so the lookups handed callers confidently-wrong
// answers. v2's operational source is Move 0: the hand-authored JSON tree
// data/athena/tree.json, served by chorus-api at:
//   /api/athena/tree                 (full tree)
//   /api/athena/ownership/:iri       (owner + containing path)
//   /api/athena/blast-radius/:iri    (consumers / dependents / hosts)
// These routes return the result object directly (no {data} envelope). This
// reverts to the routes the file proxied before the regression.
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
    throw new Error(`chorus-api ${path} unreachable: ${(err as Error).message}`);
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

/** Full structural tree (v2 JSON tree, data/athena/tree.json) — backs chorus_tree_get. */
export function getTree(): unknown {
  return getter('/api/athena/tree');
}

/**
 * Who owns this IRI + where it sits — v2 tree.json ownership lookup.
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

/** Inferred blast-radius — v2 tree.json blast-radius. null = not found. */
export function computeBlastRadius(_tree: TreeHandle, iri: string): BlastRadiusResult | null {
  const res = getter(`/api/athena/blast-radius/${encodeURIComponent(iri)}`) as
    | { consumers?: unknown[] }
    | null
    | undefined;
  if (!res || !Array.isArray(res.consumers)) return null;
  return res as BlastRadiusResult;
}
