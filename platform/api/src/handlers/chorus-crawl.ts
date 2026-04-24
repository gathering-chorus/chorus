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
  const resp = await fetchFn(`${fusekiUrl}/gathering/sparql`, {
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
    const sparql = `SELECT ?s ?p ?o WHERE { GRAPH ?g { ?s ?p ?o } FILTER(CONTAINS(LCASE(STR(?g)), '${domain}')) } LIMIT 200`;
    const bindings = await sparqlPost(fetchFn, fusekiUrl, sparql);
    rdf.count = bindings.length;
    rdf.instances = bindings.length;
    const classes = new Set<string>();
    for (const b of bindings) {
      if (b.p?.value?.includes('type') || b.p?.value?.includes('Type')) classes.add(b.o?.value || '');
    }
    rdf.classes = [...classes].filter((c) => c.length > 0);
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
): { mentions: Array<{ timestamp: string; role: string; text: string }>; related: Array<{ domain: string; strength: number }> } {
  const mentions: Array<{ timestamp: string; role: string; text: string }> = [];
  const counts: Record<string, number> = {};
  try {
    if (db) {
      const rows = db
        .prepare('SELECT author, content, timestamp, role FROM messages WHERE content LIKE ? ORDER BY timestamp ASC LIMIT 100')
        .all(`%${domain}%`) as Array<{ author: string; content: string; timestamp: string; role: string }>;
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

function collectSpine(
  exists: (p: string) => boolean,
  readFile: (p: string, enc: BufferEncoding) => string,
  chorusLogPath: string,
  cards: CardEntry[],
  timeline: TimelineEntry[],
): SpineEntry[] {
  const spine: SpineEntry[] = [];
  try {
    if (!exists(chorusLogPath)) return spine;
    const cardIds = new Set(cards.map((c) => c.index));
    for (const line of readFile(chorusLogPath, 'utf-8').split('\n')) {
      try {
        const parsed = JSON.parse(line);
        if (!parsed.event || !parsed.event.startsWith('card.')) continue;
        const cardId = parseInt(parsed.card || '0', 10);
        if (!cardIds.has(cardId)) continue;
        spine.push({ timestamp: parsed.timestamp, event: parsed.event, role: parsed.role, card: cardId });
        timeline.push({ timestamp: parsed.timestamp, source: 'spine', text: parsed.event, role: parsed.role, card: cardId });
      } catch { /* skip malformed */ }
    }
  } catch { /* log unreadable */ }
  return spine;
}

async function collectOwl(fetchFn: FetchFn, fusekiUrl: string, domain: string): Promise<OwlBucket> {
  const owl: OwlBucket = { properties: [], relationships: [] };
  try {
    const sparql = `
      PREFIX owl: <http://www.w3.org/2002/07/owl#>
      PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
      SELECT ?class ?prop ?range WHERE {
        ?class a owl:Class .
        FILTER(CONTAINS(LCASE(STR(?class)), '${domain}'))
        OPTIONAL { ?prop rdfs:domain ?class . ?prop rdfs:range ?range . }
      } LIMIT 50`;
    for (const b of await sparqlPost(fetchFn, fusekiUrl, sparql)) {
      if (b.prop?.value) owl.properties.push(b.prop.value);
      if (b.range?.value) owl.relationships.push(b.range.value);
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

async function collectCodeScan(athenaSparqlQuery: AthenaSparqlFn, domain: string): Promise<{ scanned: string[]; discovered: string[] }> {
  const codeScan = { scanned: [] as string[], discovered: [] as string[] };
  try {
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

function alertMatchesDomain(content: string, file: string, domain: string, stem: string): boolean {
  const lower = content.toLowerCase();
  const fileLower = file.toLowerCase();
  return lower.includes(domain) || lower.includes(stem) || fileLower.includes(domain) || fileLower.includes(stem);
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
      const nameMatch = content.match(/alert:\s*(.+)/);
      const sevMatch = content.match(/severity:\s*(.+)/);
      alerts.push({
        name: nameMatch ? nameMatch[1].trim() : file.replace(/\.ya?ml$/, ''),
        severity: sevMatch ? sevMatch[1].trim() : 'unknown',
        file,
      });
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
  const { mentions, related } = collectMentions(db, domain, timeline);
  const spine = collectSpine(exists, readFile, chorusLogPath, cards, timeline);
  const owl = await collectOwl(fetchFn, fusekiUrl, domain);
  const infra = await collectInfra(execAsync, domain);
  history.feedback = collectFeedback(exists, readdir, readFile, memoryDir, domain);
  const codeScan = await collectCodeScan(athenaSparqlQuery, domain);
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
      code: { files: [] as string[] },
      codeScan,
      infra,
      logs,
      alerts,
      links: [] as unknown[],
      related,
      history,
      timeline,
      count: timeline.length,
    },
  };
}
