/**
 * Codebase topology proxy — #2099.
 *
 * First handler extraction under the #2173 AC4 pattern. Pure HTTP proxy —
 * behavior is expressible as a function over a fetcher dependency, so unit
 * tests mock the fetcher and never hit real HTTP.
 *
 * Signature pattern (to be reused for remaining handlers):
 *   - Function returns `{status: number, body: unknown}` — Express-free.
 *   - Deps passed in explicitly (fetcher here; would be sparql/sqlite clients
 *     for Fuseki/DB-backed handlers).
 *   - Router registration in server.ts becomes one line:
 *       `const r = await fetchTopology(); res.status(r.status).json(r.body)`.
 */

export interface FetchResult {
  status: number;
  body: unknown;
}

export type Fetcher = (url: string, init?: RequestInit) => Promise<Response>;

const UPSTREAM = 'http://localhost:3000/api/codebase/topology';
const TIMEOUT_MS = 8000;

export async function fetchTopology(fetcher: Fetcher = fetch): Promise<FetchResult> {
  try {
    const r = await fetcher(UPSTREAM, { signal: AbortSignal.timeout(TIMEOUT_MS) });
    if (!r.ok) {
      return { status: r.status, body: { error: 'upstream ' + r.status } };
    }
    return { status: 200, body: await r.json() };
  } catch (e) {
    return {
      status: 502,
      body: { error: e instanceof Error ? e.message : String(e) },
    };
  }
}
