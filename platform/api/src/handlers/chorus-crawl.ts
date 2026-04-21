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

// eslint-disable-next-line complexity -- #2288 pre-existing threshold violation, tracked for refactor
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

  const cards: Array<{ index: number; title: string; status: string; owner: string }> = [];
  const rdf = { classes: [] as string[], instances: 0, count: 0, relationships: [] as string[] };
  const owl = { properties: [] as string[], relationships: [] as string[] };
  const mentions: Array<{ timestamp: string; role: string; text: string }> = [];
  const spine: Array<{ timestamp: string; event: string; role: string; card: number }> = [];
  const code = { files: [] as string[] };
  const infra = { launchagents: [] as string[], endpoints: [] as string[], monitoring: [] as string[] };
  const links: unknown[] = [];
  const related: Array<{ domain: string; strength: number }> = [];
  const history = { unresolved: [] as typeof cards, feedback: [] as string[], trust_score: 0, health: '' };
  const timeline: Array<{ timestamp: string; source: string; text: string; role?: string; card?: number }> = [];

  // Source 1: Cards tagged domain:<x>
  try {
    const allCached = getBoardCards();
    for (const c of allCached.filter((c) => c.tags.includes(`domain:${domain}`))) {
      const card = {
        index: parseInt(c.id, 10),
        title: c.title,
        status: c.status,
        owner: c.owner,
      };
      cards.push(card);
      timeline.push({ timestamp: '', source: 'card', text: `#${c.id} ${c.title} [${c.status}]`, role: c.owner, card: card.index });
      if (c.status !== 'Done' && c.status !== "Won't Do") history.unresolved.push(card);
    }
  } catch { /* cache empty */ }

  // Source 2: Fuseki RDF triples
  try {
    const sparql = `SELECT ?s ?p ?o WHERE { GRAPH ?g { ?s ?p ?o } FILTER(CONTAINS(LCASE(STR(?g)), '${domain}')) } LIMIT 200`;
    const resp = await fetchFn(`${fusekiUrl}/gathering/sparql`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/x-www-form-urlencoded', Accept: 'application/sparql-results+json' },
      body: `query=${encodeURIComponent(sparql)}`,
    });
    if (resp.ok) {
      const result = (await resp.json()) as { results?: { bindings?: Array<Record<string, { value?: string }>> } };
      const bindings = result.results?.bindings || [];
      rdf.count = bindings.length;
      const classes = new Set<string>();
      for (const b of bindings) {
        if (b.p?.value?.includes('type') || b.p?.value?.includes('Type')) classes.add(b.o?.value || '');
      }
      rdf.classes = [...classes].filter((c) => c.length > 0);
      rdf.instances = bindings.length;
    }
  } catch { /* fuseki down */ }

  // Source 3: Chorus index session mentions + related-domain tally
  const relatedDomainCounts: Record<string, number> = {};
  try {
    if (db) {
      const rows = db
        .prepare(
          'SELECT author, content, timestamp, role FROM messages WHERE content LIKE ? ORDER BY timestamp ASC LIMIT 100',
        )
        .all(`%${domain}%`) as Array<{ author: string; content: string; timestamp: string; role: string }>;
      for (const m of rows) {
        const text = m.content.trim();
        if (text.startsWith('<system-reminder>')) continue;
        if (text.startsWith('Base directory for this skill:')) continue;
        if (text.length < 20) continue;
        const speaker = m.author === 'user' ? 'jeff' : m.role;
        mentions.push({ timestamp: m.timestamp, role: speaker, text: text.slice(0, 300) });
        timeline.push({ timestamp: m.timestamp, source: 'chorus-index', text: text.slice(0, 300), role: speaker });

        const domainRefs = text.match(/domain:(\w+)/g) || [];
        for (const ref of domainRefs) {
          const d = ref.replace('domain:', '');
          if (d !== domain) relatedDomainCounts[d] = (relatedDomainCounts[d] || 0) + 1;
        }
        const lower = text.toLowerCase();
        for (const kd of KNOWN_DOMAINS) {
          if (kd !== domain && lower.includes(kd)) {
            relatedDomainCounts[kd] = (relatedDomainCounts[kd] || 0) + 1;
          }
        }
      }
    }
  } catch { /* db unavailable */ }

  for (const [d, count] of Object.entries(relatedDomainCounts)) {
    related.push({ domain: d, strength: count });
  }
  related.sort((a, b) => b.strength - a.strength);

  // Source 4: Spine events
  try {
    if (exists(chorusLogPath)) {
      const cardIds = new Set(cards.map((c) => c.index));
      const lines = readFile(chorusLogPath, 'utf-8').split('\n');
      for (const line of lines) {
        try {
          const parsed = JSON.parse(line);
          if (!parsed.event || !parsed.event.startsWith('card.')) continue;
          const cardId = parseInt(parsed.card || '0', 10);
          if (!cardIds.has(cardId)) continue;
          spine.push({ timestamp: parsed.timestamp, event: parsed.event, role: parsed.role, card: cardId });
          timeline.push({ timestamp: parsed.timestamp, source: 'spine', text: parsed.event, role: parsed.role, card: cardId });
        } catch { /* skip malformed */ }
      }
    }
  } catch { /* log unreadable */ }

  // Source 5: OWL
  try {
    const owlSparql = `
      PREFIX owl: <http://www.w3.org/2002/07/owl#>
      PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
      SELECT ?class ?prop ?range WHERE {
        ?class a owl:Class .
        FILTER(CONTAINS(LCASE(STR(?class)), '${domain}'))
        OPTIONAL { ?prop rdfs:domain ?class . ?prop rdfs:range ?range . }
      } LIMIT 50`;
    const resp = await fetchFn(`${fusekiUrl}/gathering/sparql`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/x-www-form-urlencoded', Accept: 'application/sparql-results+json' },
      body: `query=${encodeURIComponent(owlSparql)}`,
    });
    if (resp.ok) {
      const result = (await resp.json()) as { results?: { bindings?: Array<Record<string, { value?: string }>> } };
      const bindings = result.results?.bindings || [];
      for (const b of bindings) {
        if (b.prop?.value) owl.properties.push(b.prop.value);
        if (b.range?.value) owl.relationships.push(b.range.value);
      }
    }
  } catch { /* OWL query failed */ }

  // Source 6: Infrastructure (LaunchAgents + hardcoded endpoints)
  try {
    const domainStem = domain.replace(/s$/, '');
    const { stdout } = await execAsync(
      `launchctl list 2>/dev/null | grep -iE "${domain}|${domainStem}" | grep "com.chorus" || true`,
      { encoding: 'utf-8', timeout: 5000 },
    );
    infra.launchagents = stdout.split('\n').filter((l: string) => l.trim()).map((l: string) => l.trim());
    if (domain === 'seeds') {
      infra.endpoints = ['GET /api/chorus/seeds (3340)', 'POST /webhook/sms (3000)', 'GET /seeds (3000)'];
      infra.monitoring = ['seed-probe LaunchAgent'];
    } else if (domain === 'chorus') {
      infra.endpoints = ['GET /api/chorus/* (3340)', 'Socket.IO (3470)', 'POST /api/nudge (3475)'];
      infra.monitoring = ['heartbeat LaunchAgent', 'alert-notifier LaunchAgent'];
    }
  } catch { /* infra lookup failed */ }

  // Source 7: Memory feedback
  try {
    if (exists(memoryDir)) {
      const files = readdir(memoryDir).filter((f) => f.startsWith('feedback_'));
      for (const file of files) {
        const content = readFile(pathMod.join(memoryDir, file), 'utf-8');
        if (content.toLowerCase().includes(domain)) {
          const nameMatch = content.match(/^name:\s*(.+)/m);
          history.feedback.push(nameMatch ? nameMatch[1] : file);
        }
      }
    }
  } catch { /* memory dir unreadable */ }

  // Source 8: Code files from instances graph
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

  // Source 9: Loki logs
  const logs: Array<{ timestamp: string; level: string; message: string; component: string }> = [];
  try {
    const domainStem = domain.replace(/s$/, '');
    const lokiQuery = encodeURIComponent(`{job="gathering-app"} |~ "${domain}|${domainStem}" | json`);
    const nowSec = Math.floor(now() / 1000);
    const start = nowSec - 86400;
    const resp = await fetchFn(
      `${lokiBaseUrl}/loki/api/v1/query_range?query=${lokiQuery}&start=${start}&end=${nowSec}&limit=20`,
    );
    if (resp.ok) {
      const data = (await resp.json()) as { data?: { result?: Array<{ values?: Array<[string, string]> }> } };
      for (const stream of data.data?.result || []) {
        for (const [_ts, line] of stream.values || []) {
          try {
            const entry = JSON.parse(line);
            logs.push({
              timestamp: entry.timestamp || '',
              level: entry.level || 'info',
              message: (entry.message || '').slice(0, 200),
              component: entry.component || '',
            });
          } catch { /* non-JSON */ }
        }
      }
      const levelOrder: Record<string, number> = { error: 0, warn: 1, info: 2 };
      logs.sort((a, b) => {
        const la = levelOrder[a.level] ?? 3;
        const lb = levelOrder[b.level] ?? 3;
        if (la !== lb) return la - lb;
        return b.timestamp.localeCompare(a.timestamp);
      });
    }
  } catch { /* loki down */ }

  // Source 10: Alerting rules
  const alerts: Array<{ name: string; severity: string; file: string }> = [];
  try {
    if (exists(alertDir)) {
      const domainStem = domain.replace(/s$/, '');
      const files = readdir(alertDir).filter((f) => f.endsWith('.yml') || f.endsWith('.yaml'));
      for (const file of files) {
        const content = readFile(pathMod.join(alertDir, file), 'utf-8');
        const lower = content.toLowerCase();
        if (
          lower.includes(domain) ||
          lower.includes(domainStem) ||
          file.toLowerCase().includes(domain) ||
          file.toLowerCase().includes(domainStem)
        ) {
          const nameMatch = content.match(/alert:\s*(.+)/);
          const sevMatch = content.match(/severity:\s*(.+)/);
          alerts.push({
            name: nameMatch ? nameMatch[1].trim() : file.replace(/\.ya?ml$/, ''),
            severity: sevMatch ? sevMatch[1].trim() : 'unknown',
            file,
          });
        }
      }
    }
  } catch { /* alerting unreadable */ }

  // Trust score
  const unresolvedCount = history.unresolved.length;
  const feedbackCount = history.feedback.length;
  const doneCount = cards.filter((c) => c.status === 'Done').length;
  const recentSpine = spine.filter((s) => s.event === 'card.accepted').length;
  const activityBonus = Math.min(40, doneCount * 10 + recentSpine * 5);
  history.trust_score = Math.max(0, Math.min(100, 50 + activityBonus - unresolvedCount * 10 - feedbackCount * 5));
  history.health = history.trust_score >= 60 ? 'healthy' : history.trust_score >= 30 ? 'attention' : 'concern';

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
      code,
      codeScan,
      infra,
      logs,
      alerts,
      links,
      related,
      history,
      timeline,
      count: timeline.length,
    },
  };
}
