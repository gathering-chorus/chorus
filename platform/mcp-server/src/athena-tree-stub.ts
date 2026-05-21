// #3025 AC6 — Athena lookups, repointed to the v2 live graph.
//
// History: this file proxied to chorus-api's v1 routes (/api/athena/tree,
// /ownership/:iri, /blast-radius/:iri), which read the hand-authored
// data/athena/tree.json. That file drifted — e.g. v1 returned not-found for
// chorus:cards-service while the live graph has it (owner Wren, step Directing).
// Per ADR-031 (one source of truth) the lookups now read the v2 SPARQL
// subdomains resource: /api/athena/subdomains[/:id[/blast-radius]].
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

// Opaque handle, kept for signature compatibility (lookups ignore it).
export type TreeHandle = Record<string, never>;
export function loadTree(): TreeHandle {
  return {};
}

// chorus:cards-service -> cards-service (the v2 subdomain id).
function iriToId(iri: string): string {
  return iri.replace(/^chorus:/, '');
}

type V2Envelope = { data?: unknown } | null | undefined;

/** Full tree (v2 subdomains list) — backs chorus_tree_get. */
export function getTree(): unknown {
  const res = getter('/api/athena/subdomains') as V2Envelope;
  return res?.data ?? [];
}

/** Who owns this IRI + where it sits — v2 subdomain detail. null = not found. */
export function lookupOwnership(_tree: TreeHandle, iri: string): unknown {
  const res = getter(`/api/athena/subdomains/${encodeURIComponent(iriToId(iri))}`) as V2Envelope;
  const d = res?.data as { id?: string; label?: string; owner?: string; step?: string } | null | undefined;
  if (!d || !d.id) return null;
  return { iri, id: d.id, kind: 'subdomain', owner: d.owner, label: d.label, step: d.step };
}

export type BlastRadiusResult = { consumers: unknown[]; [k: string]: unknown };

/** Inferred blast-radius — v2 subdomain blast-radius. null = not found. */
export function computeBlastRadius(_tree: TreeHandle, iri: string): BlastRadiusResult | null {
  const res = getter(`/api/athena/subdomains/${encodeURIComponent(iriToId(iri))}/blast-radius`) as V2Envelope;
  const d = res?.data as { consumers?: unknown[] } | null | undefined;
  if (!d || !Array.isArray(d.consumers)) return null;
  return { iri, consumers: d.consumers };
}
