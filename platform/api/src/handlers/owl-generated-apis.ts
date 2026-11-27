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
/** how long a failed map read is remembered before athena-make is asked again */
const FAILURE_TTL_MS = 5_000;
/** the most pages one listing may take; more than this is a failed read */
const MAX_PAGES = 20;

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

/** what came back from an upstream is JSON of any shape until it is looked at */
const isRow = (v: unknown): v is Row => v !== null && typeof v === 'object';
/** the objects in a list, or null when the value is not a list */
const rowsIn = (v: unknown): Row[] | null => (Array.isArray(v) ? (v as unknown[]).filter(isRow) : null);

const rowsOf = (body: unknown): Row[] => rowsIn(isRow(body) ? body.data : undefined) ?? [];

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
  const binds = bindRows(properties);
  const apis: GeneratedApis = new Map();
  const skipped: SkippedBind[] = [];
  for (const d of domains) {
    if (typeof d.name !== 'string' || !DOMAIN.test(d.name)) continue;
    const bind = allowedBind(strings(d.hasProperty), binds, (value) => bindRefusal(value, rule, ownPort), skipped);
    if (bind) apis.set(d.name, bind);
  }
  return { apis, skipped, ruleProblem: 'missing' in rule ? rule.missing : null };
}

type BindRow = { row: string; value: string };

/** Every server.bind Property row, by its name with the `property-` prefix dropped. */
function bindRows(properties: Row[]): Map<string, BindRow> {
  const binds = new Map<string, BindRow>();
  for (const p of properties) {
    if (p.propertyKey !== BIND_KEY || typeof p.name !== 'string') continue;
    const value = typeof p.propertyValue === 'string' ? p.propertyValue.trim() : '';
    binds.set(propertyName(p.name), { row: p.name, value });
  }
  return binds;
}

/** The first bind a Domain row names that the rule allows; each one it refuses is added to `skipped`. */
function allowedBind(
  named: string[],
  binds: Map<string, BindRow>,
  refusal: (value: string) => string | null,
  skipped: SkippedBind[],
): string | null {
  for (const name of named) {
    const bind = binds.get(propertyName(name));
    if (!bind) continue;
    const reason = refusal(bind.value);
    if (!reason) return bind.value;
    skipped.push({ row: bind.row, reason });
  }
  return null;
}

/** athena-make's collection envelope names the next page as `links.next`, a path on the same upstream */
const nextPage = (body: unknown): string | null => {
  const links = isRow(body) ? body.links : undefined;
  const next = isRow(links) ? links.next : undefined;
  return typeof next === 'string' && next ? next : null;
};

/**
 * Every row of an athena-make collection: the first page at `path`, then each
 * `links.next` until there is none (the default page is 100 rows). A listing that
 * does not end within MAX_PAGES is a failed read — never a silently short one, since
 * a short Domain or Property listing would quietly drop a generated API or the bind rule.
 */
async function readRows(owlBase: string, path: string, fetchImpl: FetchLike): Promise<Row[]> {
  const rows: Row[] = [];
  let next: string | null = path;
  for (let page = 0; next; page++) {
    if (page >= MAX_PAGES) throw new Error(`athena-make ${path} did not end within ${MAX_PAGES} pages`);
    const url = owlBase + next;
    const r = await fetchImpl(url, { headers: { Accept: 'application/json' }, signal: bounded() });
    if (!r.ok) throw new Error(`athena-make answered HTTP ${r.status} on ${url}`);
    const body: unknown = await r.json();
    rows.push(...rowsOf(body));
    next = nextPage(body);
  }
  return rows;
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
    readRows(owlBase, '/domains', fetchImpl),
    readRows(owlBase, '/properties/properties?limit=2000', fetchImpl),
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
/** the last read that failed: inside FAILURE_TTL_MS of it no new read is made */
let failed: { owlBase: string; ownPort: number; at: number } | null = null;

/** Forget the cached map (tests; and nothing else needs it). */
export function resetGeneratedApisCache(): void {
  cache = null;
  noMapSaid = false;
  failed = null;
}

/**
 * The map, cached for a minute. When athena-make cannot be read the last good map
 * for the same upstream is served; with none, there are no generated APIs and every
 * path goes to athena-make, which then answers (or 502s) for itself.
 * `ownPort` is the port this server listens on: a bind on it is never used.
 * `now` is the clock (epoch ms), passed in so a test controls the map's age.
 * A stale map is never served silently: each failed read logs the map's age; a failed
 * read with no map at all is logged once until a read succeeds.
 * A failed read is remembered for FAILURE_TTL_MS: requests inside that window get the
 * same answer (the stale map, or none) without a new read or a new log line, so an
 * athena-make outage costs one read attempt per window, not two per request.
 */
export async function generatedApis(
  owlBase: string,
  fetchImpl: FetchLike,
  ownPort: number,
  now: number = Date.now(),
): Promise<GeneratedApis> {
  const known = cache && sameMap(cache, owlBase, ownPort) ? cache : null;
  if (known && now - known.at < CACHE_TTL_MS) return known.apis;
  // a read failed moments ago: the same answer, with no new read and no new line
  if (failed && sameMap(failed, owlBase, ownPort) && now - failed.at < FAILURE_TTL_MS) return lastGood(known);
  try {
    const apis = await readGeneratedApis(owlBase, fetchImpl, ownPort);
    cache = { owlBase, ownPort, at: now, apis };
    noMapSaid = false;
    failed = null;
    return apis;
  } catch (e) {
    failed = { owlBase, ownPort, at: now };
    sayReadFailed(known, now, errorText(e));
    return lastGood(known);
  }
}

const sameMap = (m: { owlBase: string; ownPort: number }, owlBase: string, ownPort: number): boolean =>
  m.owlBase === owlBase && m.ownPort === ownPort;

/** What is served when athena-make cannot be read: the last good map, or no generated API at all. */
const lastGood = (known: { apis: GeneratedApis } | null): GeneratedApis => (known ? known.apis : new Map());

/** One line per failed read attempt when a stale map is in use (with its age); once, when there is no map. */
function sayReadFailed(known: { at: number } | null, now: number, reason: string): void {
  if (known) {
    // Silas: keep last-good, but never silently
    logSkipped(`[owl-proxy] athena-make unreadable, serving the map read ${Math.round((now - known.at) / 1000)} s ago (${reason})`);
    return;
  }
  if (!noMapSaid) logSkipped(`[owl-proxy] athena-make unreadable, no map yet, no generated API is used (${reason})`);
  noMapSaid = true;
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
    const body: unknown = await r.json();
    return rowsIn(isRow(body) ? body.primitives : undefined);
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
  const doc = r.ok ? discoveryDoc(text) : null;
  if (!doc) return asCame;

  const primitives: unknown[] = [...doc.primitives];
  const answers = await Promise.all(
    [...apis].map(async ([domain, bind]) => ({ domain, found: await primitivesOf(bind, fetchImpl) })),
  );
  const unreachable = addGenerated(primitives, answers);
  const added = primitives.length - doc.primitives.length;
  const merged: Row = {
    ...doc.body,
    count: typeof doc.body.count === 'number' ? doc.body.count + added : primitives.length,
    primitives,
  };
  if (unreachable.length > 0) merged.unreachable = unreachable.sort();
  return { status: r.status, contentType: 'application/json', body: JSON.stringify(merged) };
}

/** A discovery document's body and its primitives list, or null when the text is not one. */
function discoveryDoc(text: string): { body: Row; primitives: unknown[] } | null {
  let body: unknown;
  try {
    body = JSON.parse(text);
  } catch {
    return null;
  }
  if (!isRow(body) || !Array.isArray(body.primitives)) return null;
  return { body, primitives: body.primitives as unknown[] };
}

/**
 * Add each generated API's primitives to the list (a kind already listed is not added
 * again; the collection gets the public /v1 path). Returns the domains whose API gave
 * no usable answer.
 */
function addGenerated(primitives: unknown[], answers: Array<{ domain: string; found: Row[] | null }>): string[] {
  const listed = new Set(primitives.filter(isRow).map((p) => p.kind).filter((k): k is string => typeof k === 'string'));
  const unreachable: string[] = [];
  for (const { domain, found } of answers) {
    if (!found) {
      unreachable.push(domain);
      continue;
    }
    for (const p of found) {
      if (typeof p.kind !== 'string' || listed.has(p.kind)) continue;
      listed.add(p.kind);
      // #5042 — the door's own OpenAPI route moves under /v1 with its collection, so the
      // merged entry names an address the model door answers, as athena-make's entries do
      const v1 = (r: unknown) => (typeof r === 'string' ? `/v1${r}` : r);
      primitives.push({ ...p, collection: v1(p.collection), ...(p.openapi === undefined ? {} : { openapi: v1(p.openapi) }) });
    }
  }
  return unreachable;
}

/** 200 the address names something in the model; 404 it does not; 502 the model could not be read to say. */
export type ViewStatus = 200 | 404 | 502;
/** the status, and for a 502 the reason the model could not be read */
export type ViewAnswer = { status: ViewStatus; reason?: string };

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
): Promise<ViewAnswer> {
  if (!domain) return { status: 200 };
  let row: Row | undefined;
  try {
    row = (await readRows(owlBase, '/domains', fetchImpl)).find((r) => r.name === domain);
  } catch (e) {
    return { status: 502, reason: `the Domain rows could not be read: ${errorText(e)}` };
  }
  if (!row) return { status: 404 };
  if (!instance) return { status: 200 };
  return instanceStatus(row, instance, await generatedApis(owlBase, fetchImpl, ownPort), owlBase, fetchImpl);
}

const errorText = (e: unknown): string => (e instanceof Error ? e.message : String(e));

/** Whether one of the Domain row's kinds has a row named `instance`: 200 yes, 404 no, 502 could not be read (and why). */
async function instanceStatus(
  row: Row,
  instance: string,
  apis: GeneratedApis,
  owlBase: string,
  fetchImpl: FetchLike,
): Promise<ViewAnswer> {
  const served = await servedKinds(apis, owlBase, fetchImpl);
  if (typeof served === 'string') return { status: 502, reason: served };
  // this domain's own API did not answer: its kinds are missing from the list, not absent from the model
  let undecided = served.unreachable.includes(String(row.name))
    ? `${String(row.name)}-api did not answer its discovery`
    : null;
  for (const kind of strings(row.definesVocabulary).map((k) => k.split(/[#/]/).pop() ?? '')) {
    const collection = served.primitives.find((p) => p.kind === kind)?.collection;
    if (typeof collection !== 'string') continue;
    const route = routeOwl(`${collection}/${encodeURIComponent(instance)}`, apis, owlBase);
    const has = await hasRow(route.target, fetchImpl);
    if (has === true) return { status: 200 };
    if (has !== false) undecided = `${route.upstream} could not say whether it has the ${kind} row: ${has}`;
  }
  return undecided ? { status: 502, reason: undecided } : { status: 404 };
}

/**
 * The merged discovery's primitives and the domains whose API could not be asked, or —
 * as a string — the reason the discovery could not be read. One reader of "what is
 * served" for the model view and the class atlas.
 */
export async function servedKinds(
  apis: GeneratedApis,
  owlBase: string,
  fetchImpl: FetchLike,
): Promise<{ primitives: Row[]; unreachable: string[] } | string> {
  try {
    const d = await mergeDiscovery('/', apis, owlBase, fetchImpl);
    if (d.status !== 200) return `athena-make answered HTTP ${d.status} for its discovery`;
    const doc = discoveryDoc(d.body);
    return doc
      ? { primitives: doc.primitives.filter(isRow), unreachable: strings(doc.body.unreachable) }
      : 'athena-make answered something that is not a discovery document';
  } catch (e) {
    return `athena-make did not answer its discovery: ${errorText(e)}`;
  }
}

/** true the upstream has the row, false it answered 404; a string (why) when it could not say. */
async function hasRow(target: string, fetchImpl: FetchLike): Promise<boolean | string> {
  try {
    const r = await fetchImpl(target, { headers: { Accept: 'application/json' }, signal: bounded() });
    if (r.ok) return true;
    return r.status === 404 ? false : `HTTP ${r.status}`;
  } catch (e) {
    return errorText(e);
  }
}
