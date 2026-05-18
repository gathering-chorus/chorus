// #2997 — Athena tree stub. The real implementation lives in chorus-api
// (platform/api/src/handlers/athena-tree.ts) and depends on fuseki client +
// oxigraph + lancedb. To keep chorus-mcp lean and decoupled from chorus-api's
// data layer, we proxy these calls to chorus-api over HTTP instead of importing
// in-process. chorus-api remains the canonical source for the Athena tree.

// API shape matches the in-process handler the chorus-mcp code was extracted
// from: synchronous loadTree() returning a tree token, then sync lookup /
// blast-radius calls that take (tree, iri). The tree token is opaque from
// the caller's perspective — only the stub knows it's actually a URL handle.
// Under the hood we proxy to chorus-api over HTTP; we cache nothing for now
// (chorus-api owns the data, our role is pass-through).
//
// The sync-shape preserves the call-site signature so the extracted
// chorus-mcp server.ts compiles without touching every Athena tool handler.

const CHORUS_API_URL = process.env.CHORUS_API_URL || 'http://localhost:3340';

// Opaque tree handle. The actual proxy URL is baked in.
export type TreeHandle = { url: string };

export function loadTree(): TreeHandle {
  return { url: CHORUS_API_URL };
}

// Sync wrapper around the HTTP proxy via synchronous XHR-style fetch.
// Node's global fetch is async; we use a worker-thread-style sync pattern via
// child_process execFileSync (curl). Slow per-call but Athena tree lookups
// are rare. Cost in latency: ~5-20ms over loopback.
import { execFileSync } from 'child_process';

function curlJson(path: string): unknown {
  try {
    const out = execFileSync('curl', ['-s', '--max-time', '5', `${CHORUS_API_URL}${path}`], {
      encoding: 'utf-8',
    });
    return JSON.parse(out);
  } catch (err) {
    throw new Error(`chorus-api ${path} unreachable: ${(err as Error).message}`);
  }
}

export function lookupOwnership(_tree: TreeHandle, iri: string): unknown {
  return curlJson(`/api/athena/ownership?iri=${encodeURIComponent(iri)}`);
}

export type BlastRadiusResult = { consumers: unknown[]; [k: string]: unknown };
export function computeBlastRadius(_tree: TreeHandle, iri: string): BlastRadiusResult | null {
  const out = curlJson(`/api/athena/blast-radius?iri=${encodeURIComponent(iri)}`);
  return (out as BlastRadiusResult) ?? null;
}
