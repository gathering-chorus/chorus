/**
 * #3863 — `GET /api/chorus/properties/resolve` — the door the word-cap
 * consumers have been knocking on since #3838.
 *
 * `mcp-server/src/word-cap.ts:118` and the chorus-hooks equivalent have always
 * called this route. It has never existed. Both consumers treat any non-ok
 * response as "fail open and use the compiled constant", so a missing route is
 * indistinguishable from a working one at the call site — which is why every
 * cap Jeff mandated has been a hardcoded number wearing the costume of a
 * governed value, and why nobody noticed for weeks.
 *
 * This PROXIES the generated cascade; it does not resolve. owl-api's
 * `/effective/<node>/<key>` walks the real scope chain. A second cascade here
 * would be a competing implementation of the thing this card just built, and
 * the two would drift the moment either changed.
 */

export type ResolveQuery = { key: string; scope: string; name: string };

export type ResolveDeps = {
  fetchImpl: (url: string) => Promise<Response>;
  owlBase: string;
};

export type ResolveResult = { status: number; body: unknown };

/**
 * How a (scope, name) pair becomes a node localname.
 *
 * Kept as an explicit map rather than string concatenation so an unknown scope
 * REFUSES instead of composing a plausible-looking IRI for a subject that does
 * not exist. `galaxy` + `wren` would otherwise resolve `galaxy-wren` and 404
 * from upstream — the same status as a genuinely unset key, and a caller could
 * not tell the difference between "no such property" and "you asked wrong".
 */
const NODE_FOR: Record<string, (name: string) => string> = {
  role: (name) => `role-${name}`,
  service: (name) => name,
  domain: (name) => name,
  product: (name) => name,
};

export async function resolveProperty(
  q: ResolveQuery,
  deps: ResolveDeps,
): Promise<ResolveResult> {
  if (!q.key) {
    return { status: 400, body: { error: 'key is required' } };
  }
  const toNode = NODE_FOR[q.scope];
  if (!toNode) {
    return {
      status: 400,
      body: { error: `unknown scope '${q.scope}'`, scopes: Object.keys(NODE_FOR) },
    };
  }
  if (!q.name) {
    return { status: 400, body: { error: 'name is required' } };
  }

  const node = toNode(q.name);
  const url = `${deps.owlBase}/effective/${encodeURIComponent(node)}/${encodeURIComponent(q.key)}`;

  let resp: Response;
  try {
    resp = await deps.fetchImpl(url);
  } catch (e) {
    // An outage is NOT an answer. Returning a value here — any value — would
    // put a number in front of a consumer that has no way to know the store
    // was unreachable, which is exactly the fail-open silence this route
    // exists to replace.
    return {
      status: 502,
      body: { error: `properties upstream unreachable: ${String(e)}`, node, key: q.key },
    };
  }

  if (!resp.ok) {
    // Pass the upstream status through unchanged. A 404 means no property in
    // the node's scope chain sets this key, and that is a fact the caller is
    // entitled to — not something to paper over with a default.
    return {
      status: resp.status,
      body: { error: 'no property sets key', node, key: q.key },
    };
  }

  const body = (await resp.json()) as Record<string, unknown>;
  return { status: 200, body };
}
