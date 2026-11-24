/**
 * #5034 — the generated APIs behind the same-origin /owl proxy, read from the graph.
 *
 * A generated API is a Domain row whose `hasProperty` names a Property row with
 * `propertyKey === "server.bind"`; that row's `propertyValue` is where the domain's
 * own API listens (principles: 127.0.0.1:3393). Until this card the proxy named one
 * such API in code, and the discovery document at /owl/ listed none of them, so the
 * Athena pages said "Principle: Not served" about a class that was served.
 *
 * Three things live here, and server.ts only wires them:
 *   - readGeneratedApis / generatedApis — the domain → bind map, from athena-make's rows
 *   - routeOwl — which upstream a proxied path goes to
 *   - mergeDiscovery — athena-make's discovery plus each generated API's own
 *
 * A generated API that does not answer is NAMED in the merged document
 * (`unreachable`), never dropped: a missing entry and an entry we could not ask
 * about are different answers.
 */

export type FetchLike = (
  url: string,
  init?: { headers?: Record<string, string>; signal?: AbortSignal },
) => Promise<Response>;

/** domain name → bind (host:port) */
export type GeneratedApis = Map<string, string>;

export type OwlRoute = {
  /** the full upstream URL */
  target: string;
  /** who to name when it does not answer: `athena-make` or `<domain>-api` */
  upstream: string;
  /** true when the path is the discovery document (`/owl/`, with or without a query) */
  discovery: boolean;
};

export type DiscoveryAnswer = { status: number; contentType: string; body: string };

type Row = Record<string, unknown>;

const BIND_KEY = 'server.bind';
/** the propertyKeys of the two Property rows that carry the bind rule's values (Silas's rule; Jeff: values live in rows, not code) */
const RULE_HOSTS = 'owl.proxy.allowed_hosts';
const RULE_PORTS = 'owl.proxy.port_range';
/** a domain is a local name — it is matched against a path segment */
const DOMAIN = /^[A-Za-z][A-Za-z0-9_-]*$/;
const READ_TIMEOUT_MS = 3000;
const CACHE_TTL_MS = 60_000;

/**
 * Every upstream read this module makes is bounded by this: a hung athena-make or
 * generated API must not hang /owl/ or a /domains page. Overridable so a test with a
 * stub that never answers does not wait the real three seconds.
 */
let readTimeoutMs = READ_TIMEOUT_MS;
export function setReadTimeoutMs(ms: number = READ_TIMEOUT_MS): void {
  readTimeoutMs = ms;
}
const bounded = (): AbortSignal => AbortSignal.timeout(readTimeoutMs);

const strings = (v: unknown): string[] => {
  if (typeof v === 'string') return [v];
  return Array.isArray(v) ? v.filter((x): x is string => typeof x === 'string') : [];
};

/** `property-principles-server-bind`, an IRI ending in it, or `principles-server-bind` → the last form */
const propertyName = (v: string): string => (v.split(/[#/]/).pop() ?? '').replace(/^property-/, '');

const rowsOf = (body: unknown): Row[] => {
  const data = (body as { data?: unknown } | null)?.data;
  return Array.isArray(data) ? data.filter((r): r is Row => r !== null && typeof r === 'object') : [];
};

/** Which binds this proxy may fetch: the values of the two rule rows. */
export type BindRule = { hosts: string[]; minPort: number; maxPort: number };

/** A bind the rule refused: the Property row that named it, and why. */
export type SkippedBind = { row: string; reason: string };

/**
 * The bind rule from the Property rows, or the reason there is none. Pure.
 * The two rows are found by propertyKey, never by name. Both are needed; a missing,
 * doubled or malformed one means NO rule, and with no rule no bind is used
 * (bindRefusal refuses every bind) — fail closed.
 */
export function bindRuleFromRows(properties: Row[]): BindRule | { missing: string } {
  /** the one row's value; a string reason when there is no such row, or more than one */
  const value = (key: string): { value: string } | string => {
    const rows = properties.filter((p) => p.propertyKey === key);
    if (rows.length === 0) return `no ${key} row`;
    if (rows.length > 1) return `${rows.length} rows carry ${key}`;
    return typeof rows[0].propertyValue === 'string' ? { value: rows[0].propertyValue.trim() } : `${key} has no value`;
  };
  const hostsRow = value(RULE_HOSTS);
  if (typeof hostsRow === 'string') return { missing: hostsRow };
  const hosts = hostsRow.value.split(',').map((h) => h.trim()).filter(Boolean);
  if (hosts.length === 0) return { missing: `${RULE_HOSTS} names no host` };
  const portsRow = value(RULE_PORTS);
  if (typeof portsRow === 'string') return { missing: portsRow };
  const portsValue = portsRow.value;
  const m = /^(\d{1,5})-(\d{1,5})$/.exec(portsValue);
  const [minPort, maxPort] = m ? [Number(m[1]), Number(m[2])] : [0, 0];
  if (!m || minPort < 1 || maxPort > 65535 || minPort > maxPort) {
    return { missing: `${RULE_PORTS} is not a port range (<low>-<high>, 1-65535)` };
  }
  return { hosts, minPort, maxPort };
}

/**
 * THE decision: may this proxy fetch `bind`? Returns null when it may, else the reason
 * it may not. Pure. A bind is used only when its host is in the allowed-hosts list
 * (exact string — `localhost` is not `127.0.0.1`), its port is an integer inside the
 * allowed range, and the port is not this server's own (a loop). No rule, no bind.
 */
export function bindRefusal(bind: string, rule: BindRule | { missing: string }, ownPort: number): string | null {
  if ('missing' in rule) return `no bind rule (${rule.missing})`;
  const m = /^([^\s:/@?#]+):(\d{1,5})$/.exec(bind);
  if (!m) return 'not host:port';
  const [host, port] = [m[1], Number(m[2])];
  if (!rule.hosts.includes(host)) return `host ${host} is not in ${RULE_HOSTS}`;
  if (port < rule.minPort || port > rule.maxPort) return `port ${port} is outside ${RULE_PORTS} ${rule.minPort}-${rule.maxPort}`;
  if (port === ownPort) return `port ${port} is this server's own`;
  return null;
}

/**
 * The map from two row listings, and the binds the rule refused. Pure.
 * A refused bind is not in the map, so nothing can fetch it: its domain is then a
 * domain with no generated API.
 */
export function generatedApisFromRows(
  domains: Row[],
  properties: Row[],
  ownPort: number,
): { apis: GeneratedApis; skipped: SkippedBind[]; ruleProblem: string | null } {
  const rule = bindRuleFromRows(properties);
  const ruleProblem = 'missing' in rule ? rule.missing : null;
  const binds = new Map<string, { row: string; value: string }>();
  for (const p of properties) {
    if (p.propertyKey !== BIND_KEY || typeof p.name !== 'string') continue;
    const value = typeof p.propertyValue === 'string' ? p.propertyValue.trim() : '';
    binds.set(propertyName(p.name), { row: p.name, value });
  }
  const apis: GeneratedApis = new Map();
  const skipped: SkippedBind[] = [];
  for (const d of domains) {
    if (typeof d.name !== 'string' || !DOMAIN.test(d.name)) continue;
    for (const named of strings(d.hasProperty)) {
      const bind = binds.get(propertyName(named));
      if (!bind) continue;
      const reason = bindRefusal(bind.value, rule, ownPort);
      if (reason) {
        skipped.push({ row: bind.row, reason });
        continue;
      }
      apis.set(d.name, bind.value);
      break;
    }
  }
  return { apis, skipped, ruleProblem };
}

async function readRows(url: string, fetchImpl: FetchLike): Promise<Row[]> {
  const r = await fetchImpl(url, {
    headers: { Accept: 'application/json' },
    signal: bounded(),
  });
  if (!r.ok) throw new Error(`athena-make answered HTTP ${r.status} on ${url}`);
  return rowsOf(await r.json());
}

/** Says a refused bind once per map read (never per request). Replaceable so a test can read it. */
let logSkipped: (line: string) => void = (line) => console.warn(line);
export function setSkippedBindLog(log: (line: string) => void): void {
  logSkipped = log;
}

/**
 * Read the map from athena-make. Throws when either listing cannot be read.
 * The bind rule's two rows ride the same Property listing. Each refused bind is
 * logged here, once per read, by row name and reason.
 */
export async function readGeneratedApis(owlBase: string, fetchImpl: FetchLike, ownPort: number): Promise<GeneratedApis> {
  const [domains, properties] = await Promise.all([
    readRows(`${owlBase}/domains`, fetchImpl),
    readRows(`${owlBase}/properties/properties?limit=2000`, fetchImpl),
  ]);
  const { apis, skipped, ruleProblem } = generatedApisFromRows(domains, properties, ownPort);
  if (ruleProblem) logSkipped(`[owl-proxy] bind rule unreadable, no generated API is used: ${ruleProblem}`);
  if (skipped.length > 0) {
    logSkipped(`[owl-proxy] server.bind not used: ${skipped.map((s) => `${s.row} (${s.reason})`).join('; ')}`);
  }
  return apis;
}

let cache: { owlBase: string; ownPort: number; at: number; apis: GeneratedApis } | null = null;
/** "no map yet" has been logged since the last successful read */
let noMapSaid = false;

/** Forget the cached map (tests; and nothing else needs it). */
export function resetGeneratedApisCache(): void {
  cache = null;
  noMapSaid = false;
}

/**
 * The map, cached for a minute. When athena-make cannot be read the last good map
 * for the same upstream is served; with none, there are no generated APIs and every
 * path goes to athena-make, which then answers (or 502s) for itself.
 * `ownPort` is the port this server listens on: a bind on it is never used.
 * `now` is the clock (epoch ms), passed in so a test controls the map's age.
 * A stale map is never served silently: each failed read logs the map's age; a failed
 * read with no map at all is logged once until a read succeeds.
 */
export async function generatedApis(
  owlBase: string,
  fetchImpl: FetchLike,
  ownPort: number,
  now: number = Date.now(),
): Promise<GeneratedApis> {
  const known = cache && cache.owlBase === owlBase && cache.ownPort === ownPort ? cache : null;
  if (known && now - known.at < CACHE_TTL_MS) return known.apis;
  try {
    const apis = await readGeneratedApis(owlBase, fetchImpl, ownPort);
    cache = { owlBase, ownPort, at: now, apis };
    noMapSaid = false;
    return apis;
  } catch (e) {
    const reason = e instanceof Error ? e.message : String(e);
    if (known) {
      // a stale map is in use: say so on every failed read, with its age (Silas: keep last-good, but never silently)
      logSkipped(`[owl-proxy] athena-make unreadable, serving the map read ${Math.round((now - known.at) / 1000)} s ago (${reason})`);
      return known.apis;
    }
    // said once, until a read succeeds: with no map every /owl request repeats this read
    if (!noMapSaid) logSkipped(`[owl-proxy] athena-make unreadable, no map yet, no generated API is used (${reason})`);
    noMapSaid = true;
    return new Map();
  }
}

/**
 * Where a proxied path goes. `path` is the request URL with `/owl` removed.
 * `/v1/<domain>/...` goes to that domain's own API with `/v1` dropped, for every
 * domain in the map; a path that only starts with a domain's name stays on athena-make.
 */
export function routeOwl(path: string, apis: GeneratedApis, owlBase: string): OwlRoute {
  const domain = /^\/v1\/([^/?]+)(?=\/|\?|$)/.exec(path)?.[1];
  const bind = domain ? apis.get(domain) : undefined;
  if (domain && bind) {
    return { target: `http://${bind}${path.replace(/^\/v1/, '')}`, upstream: `${domain}-api`, discovery: false };
  }
  return { target: owlBase + path, upstream: 'athena-make', discovery: /^\/?(\?|$)/.test(path) };
}

/** One generated API's primitives, or null when it did not give a usable answer. */
async function primitivesOf(bind: string, fetchImpl: FetchLike): Promise<Row[] | null> {
  try {
    const r = await fetchImpl(`http://${bind}/`, {
      headers: { Accept: 'application/json' },
      signal: bounded(),
    });
    if (!r.ok) return null;
    const primitives = ((await r.json()) as { primitives?: unknown } | null)?.primitives;
    return Array.isArray(primitives)
      ? primitives.filter((p): p is Row => p !== null && typeof p === 'object')
      : null;
  } catch {
    return null;
  }
}

/**
 * athena-make's discovery document with each generated API's primitives added.
 * A generated API lists its collection at its OWN route (`/principles/principles`);
 * the merged entry carries the public path through this proxy (`/v1/principles/principles`).
 * A kind athena-make already lists is not added again. An API that does not answer is
 * named in `unreachable`.
 *
 * Throws when athena-make itself is unreachable (the caller answers 502 naming it).
 * An answer from athena-make that is not a discovery document is passed back as it came.
 */
export async function mergeDiscovery(
  path: string,
  apis: GeneratedApis,
  owlBase: string,
  fetchImpl: FetchLike,
): Promise<DiscoveryAnswer> {
  // bounded like every other read here: a timeout throws, and the caller answers 502 naming athena-make
  const r = await fetchImpl(owlBase + path, { headers: { Accept: 'application/json' }, signal: bounded() });
  const text = await r.text();
  const asCame: DiscoveryAnswer = {
    status: r.status,
    contentType: r.headers.get('content-type') ?? 'application/json',
    body: text,
  };
  if (!r.ok) return asCame;
  let doc: Row;
  try {
    doc = JSON.parse(text) as Row;
  } catch {
    return asCame;
  }
  if (doc === null || typeof doc !== 'object' || !Array.isArray(doc.primitives)) return asCame;

  const primitives = [...(doc.primitives as Row[])];
  const listed = new Set(primitives.map((p) => p?.kind).filter((k): k is string => typeof k === 'string'));
  const own = primitives.length;
  const unreachable: string[] = [];
  const answers = await Promise.all(
    [...apis].map(async ([domain, bind]) => ({ domain, found: await primitivesOf(bind, fetchImpl) })),
  );
  for (const { domain, found } of answers) {
    if (!found) {
      unreachable.push(domain);
      continue;
    }
    for (const p of found) {
      if (typeof p.kind !== 'string' || listed.has(p.kind)) continue;
      listed.add(p.kind);
      primitives.push(typeof p.collection === 'string' ? { ...p, collection: `/v1${p.collection}` } : p);
    }
  }
  const added = primitives.length - own;
  const merged: Row = {
    ...doc,
    count: typeof doc.count === 'number' ? doc.count + added : primitives.length,
    primitives,
  };
  if (unreachable.length > 0) merged.unreachable = unreachable.sort();
  return { status: r.status, contentType: 'application/json', body: JSON.stringify(merged) };
}

/** 200 the address names something in the model; 404 it does not; 502 the model could not be read to say. */
export type ViewStatus = 200 | 404 | 502;

/**
 * #5034 — the HTTP status of a model-view address (/domains/<domain>[/<instance>]).
 * The template is one file for every path, so the status is the only thing that can
 * say whether the address is real: it answered 200 for /domains/nosuch/thing.
 *
 * A domain is real when athena-make's Domain rows name it. An instance is real when
 * one of the domain's definesVocabulary kinds has a row of that name — the kind's
 * collection comes from the merged discovery, and the row is read at the upstream
 * routeOwl gives for it (athena-make, or the domain's own generated API).
 * Anything that could not be read is 502: never 200 on a guess, never 404 on a guess.
 */
export async function domainsViewStatus(
  domain: string | undefined,
  instance: string | undefined,
  owlBase: string,
  fetchImpl: FetchLike,
  ownPort: number,
): Promise<ViewStatus> {
  if (!domain) return 200;
  let row: Row | undefined;
  try {
    row = (await readRows(`${owlBase}/domains`, fetchImpl)).find((r) => r.name === domain);
  } catch {
    return 502;
  }
  if (!row) return 404;
  if (!instance) return 200;

  const apis = await generatedApis(owlBase, fetchImpl, ownPort);
  let doc: { primitives?: unknown; unreachable?: unknown };
  try {
    const d = await mergeDiscovery('/', apis, owlBase, fetchImpl);
    if (d.status !== 200) return 502;
    doc = JSON.parse(d.body) as typeof doc;
  } catch {
    return 502;
  }
  if (!Array.isArray(doc.primitives)) return 502;
  const primitives = doc.primitives as Row[];
  // this domain's own API did not answer: its kinds are missing from the list, not absent from the model
  let undecided = strings(doc.unreachable).includes(domain);
  for (const kind of strings(row.definesVocabulary).map((k) => k.split(/[#/]/).pop() ?? '')) {
    const collection = primitives.find((p) => p?.kind === kind)?.collection;
    if (typeof collection !== 'string') continue;
    const route = routeOwl(`${collection}/${encodeURIComponent(instance)}`, apis, owlBase);
    try {
      const r = await fetchImpl(route.target, {
        headers: { Accept: 'application/json' },
        signal: bounded(),
      });
      if (r.ok) return 200;
      if (r.status !== 404) undecided = true;
    } catch {
      undecided = true;
    }
  }
  return undecided ? 502 : 404;
}
