/* eslint-disable security/detect-object-injection -- Indexing on validated source/domain keys. */
/**
 * GET /api/chorus/crawl/:domain — Multi-source crawler (#1956, extracted #2189).
 *
 * Pulls domain context from 10 sources (cards, Fuseki triples, session mentions,
 * spine events, OWL classes, launchctl agents, memory feedback, code-file graph,
 * Loki logs, alerting rules) into one unified response. Each source fails
 * silently on error so a partial crawl still returns partial data.
 *
 * Dependencies injected — no filesystem, no Fuseki, no Loki in unit tests.
 */
import type Database from 'better-sqlite3';
import * as pathMod from 'path';
import { toFtsMatchQuery } from './chorus-search';

export type FetchFn = (
  url: string,
  init?: { method?: string; headers?: Record<string, string>; body?: string; signal?: AbortSignal },
) => Promise<{ ok: boolean; status: number; json: () => Promise<unknown>; text?: () => Promise<string> }>;

export type ExecAsyncFn = (
  cmd: string,
  opts?: { encoding?: BufferEncoding; timeout?: number },
) => Promise<{ stdout: string }>;

export type AthenaSparqlFn = (query: string) => Promise<{
  results: { bindings: Array<Record<string, { value: string }>> };
}>;

export interface BoardCard {
  id: string;
  title: string;
  status: string;
  owner: string;
  tags: string;
}

export interface CrawlDeps {
  db: Database.Database | null;
  getBoardCards: () => BoardCard[];
  fetchFn: FetchFn;
  athenaSparqlQuery: AthenaSparqlFn;
  execAsync: ExecAsyncFn;
  readFile: (p: string, enc: BufferEncoding) => string;
  exists: (p: string) => boolean;
  readdir: (p: string) => string[];
  now?: () => number;
  fusekiUrl?: string;
  chorusLogPath: string;
  memoryDir: string;
  alertDir: string;
  lokiBaseUrl?: string;
  mentionScanCap?: number; // #3055: cap the FTS mention scan (default MENTION_SCAN_CAP); injectable for tests.
}

export interface CrawlResult {
  status: number;
  body: Record<string, unknown>;
}

const KNOWN_DOMAINS = [
  'photos', 'music', 'people', 'books', 'cooking', 'reading', 'watching', 'property',
  'stories', 'notes', 'blog', 'gallery', 'social', 'glimmers', 'ideas', 'seeds', 'self',
  'chorus', 'clearing', 'pulse', 'spine', 'interactions', 'memory', 'infrastructure',
  'observability', 'loom', 'search',
];

type CardEntry = { index: number; title: string; status: string; owner: string };
type TimelineEntry = { timestamp: string; source: string; text: string; role?: string; card?: number };
type SpineEntry = { timestamp: string; event: string; role: string; card: number };
type RdfBucket = { classes: string[]; instances: number; count: number; relationships: string[] };
type OwlBucket = { properties: string[]; relationships: string[] };
type InfraBucket = { launchagents: string[]; endpoints: string[]; monitoring: string[] };
type HistoryBucket = { unresolved: CardEntry[]; feedback: string[]; trust_score: number; health: string };
type LogEntry = { timestamp: string; level: string; message: string; component: string };
type AlertEntry = { name: string; severity: string; file: string };

async function sparqlPost(
  fetchFn: FetchFn,
  fusekiUrl: string,
  sparql: string,
): Promise<Array<Partial<Record<string, { value?: string }>>>> {
  // #2620: dataset is /pods, not /gathering. The /gathering endpoint returns
  // 404 silently → sparqlPost returned [] for every call → rdf/owl always empty.
  const resp = await fetchFn(`${fusekiUrl}/pods/sparql`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/x-www-form-urlencoded', Accept: 'application/sparql-results+json' },
    body: `query=${encodeURIComponent(sparql)}`,
  });
  if (!resp.ok) return [];
  const result = (await resp.json()) as { results?: { bindings?: Array<Partial<Record<string, { value?: string }>>> } };
  return result.results?.bindings || [];
}

function collectCards(
  getBoardCards: () => BoardCard[],
  domain: string,
  timeline: TimelineEntry[],
  history: HistoryBucket,
): CardEntry[] {
  const cards: CardEntry[] = [];
  try {
    for (const c of getBoardCards().filter((c) => c.tags.includes(`domain:${domain}`))) {
      const card: CardEntry = { index: parseInt(c.id, 10), title: c.title, status: c.status, owner: c.owner };
      cards.push(card);
      timeline.push({ timestamp: '', source: 'card', text: `#${c.id} ${c.title} [${c.status}]`, role: c.owner, card: card.index });
      if (c.status !== 'Done' && c.status !== "Won't Do") history.unresolved.push(card);
    }
  } catch { /* cache empty */ }
  return cards;
}

async function collectRdf(fetchFn: FetchFn, fusekiUrl: string, domain: string): Promise<RdfBucket> {
  const rdf: RdfBucket = { classes: [], instances: 0, count: 0, relationships: [] };
  try {
    // #2620: query named graph urn:gathering:<domain>, group instances by class.
    // Previous shape pulled ?s ?p ?o with LCASE graph filter — over-counted
    // (every triple as an "instance") and miscategorised classes (string-match
    // on predicate). This shape returns one row per class with a real count.
    const graph = `urn:gathering:${domain}`;
    const sparql = `SELECT ?class (COUNT(DISTINCT ?s) AS ?n) WHERE { GRAPH <${graph}> { ?s a ?class } } GROUP BY ?class LIMIT 50`;
    const bindings = await sparqlPost(fetchFn, fusekiUrl, sparql);
    let total = 0;
    for (const b of bindings) {
      const cls = b.class?.value;
      const n = parseInt(b.n?.value || '0', 10);
      if (cls) {
        rdf.classes.push(cls);
        total += n;
      }
    }
    rdf.count = bindings.length;
    rdf.instances = total;
    // Cross-graph relationships: when a subject in this graph references
    // something that exists in another urn:gathering:* graph, that's a domain
    // link. Approximate with predicate-object matching across graphs.
    const relSparql = `SELECT DISTINCT ?og WHERE { GRAPH <${graph}> { ?s ?p ?o } GRAPH ?og { ?o ?p2 ?o2 } FILTER(?og != <${graph}>) FILTER(STRSTARTS(STR(?og), 'urn:gathering:')) } LIMIT 30`;
    for (const b of await sparqlPost(fetchFn, fusekiUrl, relSparql)) {
      const og = b.og?.value;
      if (og) rdf.relationships.push(og.replace('urn:gathering:', ''));
    }
  } catch { /* fuseki down */ }
  return rdf;
}

function extractMentionText(content: string): string | null {
  const text = content.trim();
  if (text.startsWith('<system-reminder>')) return null;
  if (text.startsWith('Base directory for this skill:')) return null;
  if (text.length < 20) return null;
  return text.slice(0, 300);
}

function tallyRelatedDomains(text: string, domain: string, counts: Record<string, number>): void {
  for (const ref of text.match(/domain:(\w+)/g) || []) {
    const d = ref.replace('domain:', '');
    if (d !== domain) counts[d] = (counts[d] || 0) + 1;
  }
  const lower = text.toLowerCase();
  for (const kd of KNOWN_DOMAINS) {
    if (kd !== domain && lower.includes(kd)) counts[kd] = (counts[kd] || 0) + 1;
  }
}

function collectMentions(
  db: Database.Database | null,
  domain: string,
  timeline: TimelineEntry[],
  scanCap: number = MENTION_SCAN_CAP,
): { mentions: Array<{ timestamp: string; role: string; text: string }>; related: Array<{ domain: string; strength: number }> } {
  const mentions: Array<{ timestamp: string; role: string; text: string }> = [];
  const counts: Record<string, number> = {};
  try {
    if (db) {
      // #3054: FTS5 MATCH (quote-safe), NOT a `%domain%` wildcard scan over content — that
      // is an unindexed full scan over 1.26M rows (~2.8s sync, freezes the spine; #3051 class).
      const matchQuery = toFtsMatchQuery(domain);
      // #3055: bound the FTS scan, then sort. A common term like "photos" matches
      // ~16.7K rows; `ORDER BY m.timestamp` (or `ORDER BY rank`) over the full
      // match set is a 400ms synchronous block — the dominant crawl loop-block.
      // Measured fix: cap the FTS scan to MENTION_SCAN_CAP (early-terminates, ~5ms),
      // then sort that bounded sample by timestamp and take 100. Bounded regardless
      // of how common the domain term is. Result shape unchanged (oldest-first):
      // FTS rowid is insertion-chronological for this append-only message log, so
      // the first-CAP-by-rowid are the oldest, and the 100 oldest of those == the
      // 100 oldest overall. (Would skew only if rows were BACKFILLED out of order.)
      const rows = matchQuery
        ? (db
            .prepare(
              `SELECT m.author, m.content, m.timestamp, m.role
               FROM (SELECT rowid FROM messages_fts WHERE messages_fts MATCH ? LIMIT ?) f
               JOIN messages m ON f.rowid = m.id
               ORDER BY m.timestamp ASC LIMIT 100`,
            )
            .all(matchQuery, Math.max(1, Math.floor(scanCap))) as Array<{ author: string; content: string; timestamp: string; role: string }>)
        : [];
      for (const m of rows) {
        const text = extractMentionText(m.content);
        if (!text) continue;
        const speaker = m.author === 'user' ? 'jeff' : m.role;
        mentions.push({ timestamp: m.timestamp, role: speaker, text });
        timeline.push({ timestamp: m.timestamp, source: 'chorus-index', text, role: speaker });
        tallyRelatedDomains(text, domain, counts);
      }
    }
  } catch { /* db unavailable */ }
  const related = Object.entries(counts)
    .map(([d, strength]) => ({ domain: d, strength }))
    .sort((a, b) => b.strength - a.strength);
  return { mentions, related };
}

// #3055: cap the per-crawl FTS mention scan so a common domain term (e.g. "photos",
// ~16.7K matches) can't turn collectMentions into a ~400ms synchronous loop block.
const MENTION_SCAN_CAP = 300;

// #3088: read card.* events from Loki (the spine superset) instead of reading the
// 4MB chorus.log tail + JSON.parse-per-line — that was a ~1s SYNCHRONOUS block on the
// serving loop, EVERY crawl (54/hr; #3079 class). Loki is async I/O (~52ms, non-blocking),
// line-filters card.* server-side so we parse only the tiny result, and its retention
// (~24h) matches the old tail's span — verified equivalent (9==9, #3088). Eliminates the
// block AND the waste of re-parsing 24h of log per crawl, rather than just off-loading it.
export async function collectSpine(
  fetchFn: FetchFn,
  lokiBaseUrl: string,
  chorusLogPath: string,
  cards: CardEntry[],
  timeline: TimelineEntry[],
  now: () => number,
): Promise<SpineEntry[]> {
  const spine: SpineEntry[] = [];
  try {
    const cardIds = new Set(cards.map((c) => c.index));
    // Server-side line-filter for card.* events; time-bound to ~24h to match the old tail span.
    const lokiQuery = encodeURIComponent(`{filename="${chorusLogPath}"} |= "\\"event\\":\\"card."`);
    const nowSec = Math.floor(now() / 1000);
    // #3090: 5s timeout via AbortController. Without this a SLOW Loki (vs down)
    // hangs the crawl handler indefinitely — the try/catch around this block
    // only catches Loki *throwing* ("loki down → empty"), not *hanging*. On
    // AbortError the fetch throws → outer catch returns empty spine, honoring
    // the documented degrade-to-empty contract for slow-Loki too.
    const ctrl = new AbortController();
    const timeoutId = setTimeout(() => ctrl.abort(), 5000);
    let resp;
    try {
      resp = await fetchFn(
        `${lokiBaseUrl}/loki/api/v1/query_range?query=${lokiQuery}&start=${nowSec - 86400}&end=${nowSec}&limit=1000`,
        { signal: ctrl.signal },
      );
    } finally {
      clearTimeout(timeoutId);
    }
    if (!resp.ok) return spine;
    const data = (await resp.json()) as { data?: { result?: Array<{ values?: Array<[string, string]> }> } };
    const collected: SpineEntry[] = [];
    for (const stream of data.data?.result || []) {
      for (const [, line] of stream.values || []) {
        try {
          const parsed = JSON.parse(line);
          if (!parsed.event || !parsed.event.startsWith('card.')) continue;
          const cardId = parseInt(parsed.card || '0', 10);
          if (!cardIds.has(cardId)) continue;
          collected.push({ timestamp: parsed.timestamp, event: parsed.event, role: parsed.role, card: cardId });
        } catch { /* skip malformed */ }
      }
    }
    // Chronological asc — matches the old append-only-tail order so downstream is unchanged.
    collected.sort((a, b) => (a.timestamp < b.timestamp ? -1 : a.timestamp > b.timestamp ? 1 : 0));
    for (const e of collected) {
      spine.push(e);
      timeline.push({ timestamp: e.timestamp, source: 'spine', text: e.event, role: e.role, card: e.card });
    }
  } catch { /* loki down — degrade to empty, same as collectLogs */ }
  return spine;
}

// #2627: split into class-collect + relationship-collect helpers.
function classQuery(domainStem: string): string {
  return `
    PREFIX owl: <http://www.w3.org/2002/07/owl#>
    PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
    SELECT ?class ?p ?o WHERE {
      GRAPH <urn:jb:ontology> {
        ?class a owl:Class .
        FILTER(CONTAINS(LCASE(STR(?class)), '${domainStem}'))
        ?class ?p ?o .
      }
    } LIMIT 100`;
}

function ontologyOtherClassesQuery(domainStem: string): string {
  return `
    PREFIX owl: <http://www.w3.org/2002/07/owl#>
    SELECT DISTINCT ?other WHERE {
      GRAPH <urn:jb:ontology> { ?other a owl:Class }
      FILTER(!CONTAINS(LCASE(STR(?other)), '${domainStem}'))
    } LIMIT 20`;
}

function formatOwlProperty(cls: string, p: string, o: string): string {
  const predLocal = p.split(/[#/]/).pop() || p;
  const valLocal = o.length > 80 ? o.slice(0, 77) + '...' : o;
  return `${cls.split(/[#/]/).pop()}::${predLocal}=${valLocal}`;
}

async function collectOwlClassProps(
  fetchFn: FetchFn,
  fusekiUrl: string,
  domainStem: string,
  matched: Set<string>,
): Promise<string[]> {
  const props: string[] = [];
  for (const b of await sparqlPost(fetchFn, fusekiUrl, classQuery(domainStem))) {
    const cls = b.class?.value;
    const p = b.p?.value || '';
    const o = b.o?.value || '';
    if (!cls) continue;
    matched.add(cls);
    if (p.endsWith('#type')) continue;
    props.push(formatOwlProperty(cls, p, o));
  }
  return props;
}

async function collectOwlRelations(fetchFn: FetchFn, fusekiUrl: string, domainStem: string): Promise<string[]> {
  const rels: string[] = [];
  for (const b of await sparqlPost(fetchFn, fusekiUrl, ontologyOtherClassesQuery(domainStem))) {
    const other = b.other?.value;
    if (other) rels.push(other);
  }
  return rels;
}

async function collectOwl(fetchFn: FetchFn, fusekiUrl: string, domain: string): Promise<OwlBucket> {
  const owl: OwlBucket = { properties: [], relationships: [] };
  try {
    const domainStem = domain.replace(/s$/, '');
    const matched = new Set<string>();
    owl.properties = await collectOwlClassProps(fetchFn, fusekiUrl, domainStem, matched);
    if (matched.size > 0) {
      owl.relationships = await collectOwlRelations(fetchFn, fusekiUrl, domainStem);
    }
  } catch { /* OWL query failed */ }
  return owl;
}

async function collectInfra(execAsync: ExecAsyncFn, domain: string): Promise<InfraBucket> {
  const infra: InfraBucket = { launchagents: [], endpoints: [], monitoring: [] };
  try {
    const domainStem = domain.replace(/s$/, '');
    const { stdout } = await execAsync(
      `launchctl list 2>/dev/null | grep -iE "${domain}|${domainStem}" | grep "com.chorus" || true`,
      { encoding: 'utf-8', timeout: 5000 },
    );
    infra.launchagents = stdout.split('\n').filter((l) => l.trim()).map((l) => l.trim());
    if (domain === 'seeds') {
      infra.endpoints = ['GET /api/chorus/seeds (3340)', 'POST /webhook/sms (3000)', 'GET /seeds (3000)'];
      infra.monitoring = ['seed-probe LaunchAgent'];
    } else if (domain === 'chorus') {
      infra.endpoints = ['GET /api/chorus/* (3340)', 'Socket.IO (3470)', 'POST /api/nudge (3475)'];
      infra.monitoring = ['heartbeat LaunchAgent', 'alert-notifier LaunchAgent'];
    }
  } catch { /* infra lookup failed */ }
  return infra;
}

function collectFeedback(
  exists: (p: string) => boolean,
  readdir: (p: string) => string[],
  readFile: (p: string, enc: BufferEncoding) => string,
  memoryDir: string,
  domain: string,
): string[] {
  const feedback: string[] = [];
  try {
    if (!exists(memoryDir)) return feedback;
    for (const file of readdir(memoryDir).filter((f) => f.startsWith('feedback_'))) {
      const content = readFile(pathMod.join(memoryDir, file), 'utf-8');
      if (content.toLowerCase().includes(domain)) {
        const nameMatch = content.match(/^name:\s*(.+)/m);
        feedback.push(nameMatch ? nameMatch[1] : file);
      }
    }
  } catch { /* memory dir unreadable */ }
  return feedback;
}

async function collectCodeScan(
  athenaSparqlQuery: AthenaSparqlFn,
  execAsync: ExecAsyncFn,
  domain: string,
): Promise<{ scanned: string[]; discovered: string[] }> {
  const codeScan = { scanned: [] as string[], discovered: [] as string[] };
  try {
    // scanned: graph-declared code files (chorus:hasCodeFile). When the graph
    // hasn't been populated for this domain, scanned stays empty.
    const domainSuffix = domain.endsWith('-domain') || domain.endsWith('-service') ? domain : `${domain}-domain`;
    const codeQuery = `PREFIX chorus: <https://jeffbridwell.com/chorus#> SELECT ?filePath WHERE { GRAPH <urn:chorus:instances> { <https://jeffbridwell.com/chorus#${domainSuffix}> chorus:hasCodeFile ?file . ?file chorus:filePath ?filePath . } }`;
    const codeResult = await athenaSparqlQuery(codeQuery);
    codeScan.scanned = codeResult.results.bindings.map((b) => b.filePath.value);
    if (codeScan.scanned.length === 0 && !domain.endsWith('-service')) {
      const svcQuery = `PREFIX chorus: <https://jeffbridwell.com/chorus#> SELECT ?filePath WHERE { GRAPH <urn:chorus:instances> { <https://jeffbridwell.com/chorus#${domain}-service> chorus:hasCodeFile ?file . ?file chorus:filePath ?filePath . } }`;
      const svcResult = await athenaSparqlQuery(svcQuery);
      codeScan.scanned = svcResult.results.bindings.map((b) => b.filePath.value);
    }
  } catch { /* graph query failed */ }
  // #2620: discovered = filesystem scan for files whose path mentions the
  // domain. Distinct from code.files (card/git-derived) and from scanned
  // (graph-derived). Cheap find with name-pattern, bounded depth.
  try {
    // The find -iname "*<stem>*" pattern matches both "<domain>" and "<stem>"
    // files because <stem> is a substring of <domain> ('seed' matches 'seeds.ts').
    // Single-pattern keeps the command shell-portable — earlier `\( -o \)` form
    // failed under node's exec/bin/sh because the backslashes don't survive
    // the shell-then-find round trip.
    const stem = domain.replace(/s$/, '');
    // Prune aggressively: huge subtrees (transcripts, briefs, coverage,
    // node_modules, build outputs) blow past a 5-8s timeout. Bumped timeout
    // to 12s as a backstop. Output capped at 50 entries by `head -50`.
    const { stdout } = await execAsync(
      `find /Users/jeffbridwell/CascadeProjects/chorus /Users/jeffbridwell/CascadeProjects/jeff-bridwell-personal-site -maxdepth 6 -type f -iname "*${stem}*" -not -path "*/node_modules/*" -not -path "*/.git/*" -not -path "*/dist/*" -not -path "*/target/*" -not -path "*/coverage/*" -not -path "*/transcripts/*" -not -path "*/briefs/*" -not -path "*/.claude/*" 2>/dev/null | head -50`,
      { encoding: 'utf-8', timeout: 12000 },
    );
    codeScan.discovered = stdout.split('\n').map((s) => s.trim()).filter(Boolean);
  } catch { /* find failed */ }
  return codeScan;
}

function parseLokiEntry(line: string): LogEntry | null {
  try {
    const entry = JSON.parse(line);
    return {
      timestamp: entry.timestamp || '',
      level: entry.level || 'info',
      message: (entry.message || '').slice(0, 200),
      component: entry.component || '',
    };
  } catch {
    return null;
  }
}

function sortLogs(logs: LogEntry[]): void {
  const levelOrder: Record<string, number> = { error: 0, warn: 1, info: 2 };
  logs.sort((a, b) => {
    const la = levelOrder[a.level] ?? 3;
    const lb = levelOrder[b.level] ?? 3;
    if (la !== lb) return la - lb;
    return b.timestamp.localeCompare(a.timestamp);
  });
}

async function collectLogs(fetchFn: FetchFn, lokiBaseUrl: string, domain: string, now: () => number): Promise<LogEntry[]> {
  const logs: LogEntry[] = [];
  try {
    const domainStem = domain.replace(/s$/, '');
    const lokiQuery = encodeURIComponent(`{job="gathering-app"} |~ "${domain}|${domainStem}" | json`);
    const nowSec = Math.floor(now() / 1000);
    const resp = await fetchFn(
      `${lokiBaseUrl}/loki/api/v1/query_range?query=${lokiQuery}&start=${nowSec - 86400}&end=${nowSec}&limit=20`,
    );
    if (!resp.ok) return logs;
    const data = (await resp.json()) as { data?: { result?: Array<{ values?: Array<[string, string]> }> } };
    for (const stream of data.data?.result || []) {
      for (const [, line] of stream.values || []) {
        const entry = parseLokiEntry(line);
        if (entry) logs.push(entry);
      }
    }
    sortLogs(logs);
  } catch { /* loki down */ }
  return logs;
}

// #2620: code.files comes from two sources combined: (1) blast-radius
// auto-comments on cards in this domain (the `cards move WIP` script writes
// these), and (2) git log path-mining for commits whose subject mentions
// either the domain tag or a card id from this domain. Distinct from
// codeScan.discovered which is a graph-driven filesystem scan.
async function collectCodeFiles(
  execAsync: ExecAsyncFn,
  cards: CardEntry[],
  domain: string,
): Promise<string[]> {
  const files = new Set<string>();
  try {
    if (cards.length === 0) {
      // Still try a domain-tag grep so empty-card domains aren't silently empty.
      const { stdout } = await execAsync(
        `git -C /Users/jeffbridwell/CascadeProjects/chorus log --all --grep="domain:${domain}" --name-only --pretty=format: 2>/dev/null | sort -u | head -50`,
        { encoding: 'utf-8', timeout: 5000 },
      );
      for (const f of stdout.split('\n').map((s) => s.trim()).filter(Boolean)) files.add(f);
      return [...files];
    }
    // Bound mining to the first 20 cards' ids to keep latency in check.
    const ids = cards.slice(0, 20).map((c) => `#${c.index}`).join('\\|');
    const { stdout } = await execAsync(
      `git -C /Users/jeffbridwell/CascadeProjects/chorus log --all --grep="${ids}\\|domain:${domain}" --name-only --pretty=format: 2>/dev/null | sort -u | head -100`,
      { encoding: 'utf-8', timeout: 8000 },
    );
    for (const f of stdout.split('\n').map((s) => s.trim()).filter(Boolean)) files.add(f);
  } catch { /* git unavailable or grep timeout */ }
  return [...files];
}

// #2620: connected-subgraph link phase. Until link discovery becomes a first-
// class graph traversal, surface the obvious edges so consumers see "this is
// connected, not parallel lists": card→code (each card links to all known
// files for the domain), code→class (when we have classes), card→domain
// (every card belongs to its domain). Bounded to keep the array sane.
type Link = { from_type: string; from: string; to_type: string; to: string };
function buildLinks(
  cards: CardEntry[],
  codeFiles: string[],
  owl: OwlBucket,
  related: Array<{ domain: string; strength: number }>,
): Link[] {
  const links: Link[] = [];
  const fileLimit = codeFiles.slice(0, 10);
  const cardLimit = cards.slice(0, 10);
  for (const c of cardLimit) {
    for (const f of fileLimit) {
      links.push({ from_type: 'card', from: `#${c.index}`, to_type: 'code', to: f });
    }
  }
  const classNames = ((owl.properties as string[] | undefined) || [])
    .map((p) => p.split('::')[0])
    .filter((n, i, arr) => n && arr.indexOf(n) === i)
    .slice(0, 5);
  for (const f of fileLimit) {
    for (const cls of classNames) {
      links.push({ from_type: 'code', from: f, to_type: 'class', to: cls });
    }
  }
  for (const r of related.slice(0, 5)) {
    links.push({ from_type: 'domain', from: 'self', to_type: 'domain', to: r.domain });
  }
  return links;
}

function alertMatchesDomain(content: string, file: string, domain: string, stem: string): boolean {
  const lower = content.toLowerCase();
  const fileLower = file.toLowerCase();
  return lower.includes(domain) || lower.includes(stem) || fileLower.includes(domain) || fileLower.includes(stem);
}

// #2627: per-file extraction split out of the loop body.
function extractAlertsFromContent(content: string, file: string): AlertEntry[] {
  const promMatches = [...content.matchAll(/^\s*-?\s*alert:\s*(.+)$/gm)];
  const grafTitleMatches = [...content.matchAll(/^\s*-\s*(?:uid:.*\n\s*)?title:\s*(.+)$/gm)];
  const sevDefault = (content.match(/severity:\s*(.+)/) || [, 'unknown'])[1]?.trim() || 'unknown';
  const found = promMatches.length > 0 ? promMatches : grafTitleMatches;
  if (found.length === 0) {
    return [{ name: file.replace(/\.ya?ml$/, ''), severity: sevDefault, file }];
  }
  return found.map((m) => ({ name: m[1].trim(), severity: sevDefault, file }));
}

function collectAlerts(
  exists: (p: string) => boolean,
  readdir: (p: string) => string[],
  readFile: (p: string, enc: BufferEncoding) => string,
  alertDir: string,
  domain: string,
): AlertEntry[] {
  const alerts: AlertEntry[] = [];
  try {
    if (!exists(alertDir)) return alerts;
    const stem = domain.replace(/s$/, '');
    for (const file of readdir(alertDir).filter((f) => f.endsWith('.yml') || f.endsWith('.yaml'))) {
      const content = readFile(pathMod.join(alertDir, file), 'utf-8');
      if (!alertMatchesDomain(content, file, domain, stem)) continue;
      alerts.push(...extractAlertsFromContent(content, file));
    }
  } catch { /* alerting unreadable */ }
  return alerts;
}

function computeTrust(cards: CardEntry[], spine: SpineEntry[], history: HistoryBucket): void {
  const doneCount = cards.filter((c) => c.status === 'Done').length;
  const recentSpine = spine.filter((s) => s.event === 'card.accepted').length;
  const activityBonus = Math.min(40, doneCount * 10 + recentSpine * 5);
  history.trust_score = Math.max(0, Math.min(100, 50 + activityBonus - history.unresolved.length * 10 - history.feedback.length * 5));
  history.health = history.trust_score >= 60 ? 'healthy' : history.trust_score >= 30 ? 'attention' : 'concern';
}

export async function fetchCrawl(
  domainRaw: string,
  {
    db,
    getBoardCards,
    fetchFn,
    athenaSparqlQuery,
    execAsync,
    readFile,
    exists,
    readdir,
    now = Date.now,
    fusekiUrl = 'http://localhost:3030',
    chorusLogPath,
    memoryDir,
    alertDir,
    lokiBaseUrl = 'http://localhost:3102',
    mentionScanCap = MENTION_SCAN_CAP,
  }: CrawlDeps,
): Promise<CrawlResult> {
  const domain = (domainRaw || '').toLowerCase();
  if (!KNOWN_DOMAINS.includes(domain)) {
    return {
      status: 404,
      body: {
        error: `Domain '${domain}' not found`,
        suggestion: 'Valid domains: ' + KNOWN_DOMAINS.join(', '),
        valid_count: KNOWN_DOMAINS.length,
      },
    };
  }

  const timeline: TimelineEntry[] = [];
  const history: HistoryBucket = { unresolved: [], feedback: [], trust_score: 0, health: '' };

  const cards = collectCards(getBoardCards, domain, timeline, history);
  const rdf = await collectRdf(fetchFn, fusekiUrl, domain);
  const { mentions, related } = collectMentions(db, domain, timeline, mentionScanCap);
  const spine = await collectSpine(fetchFn, lokiBaseUrl, chorusLogPath, cards, timeline, now);
  const owl = await collectOwl(fetchFn, fusekiUrl, domain);
  const infra = await collectInfra(execAsync, domain);
  history.feedback = collectFeedback(exists, readdir, readFile, memoryDir, domain);
  const codeScan = await collectCodeScan(athenaSparqlQuery, execAsync, domain);
  const codeFiles = await collectCodeFiles(execAsync, cards, domain);
  const logs = await collectLogs(fetchFn, lokiBaseUrl, domain, now);
  const alerts = collectAlerts(exists, readdir, readFile, alertDir, domain);

  computeTrust(cards, spine, history);
  timeline.sort((a, b) => (a.timestamp || '').localeCompare(b.timestamp || ''));

  return {
    status: 200,
    body: {
      domain,
      cards,
      rdf,
      owl,
      mentions,
      spine,
      code: { files: codeFiles },
      codeScan,
      infra,
      logs,
      alerts,
      links: buildLinks(cards, codeFiles, owl, related),
      related,
      history,
      timeline,
      count: timeline.length,
    },
  };
}
