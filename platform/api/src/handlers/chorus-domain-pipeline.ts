/**
 * GET /api/chorus/domain/:name/pipeline — the five steps a domain goes through, each counted
 * from the same read as the fold beside it on /athena/domain.html (#2188; rewritten by #5044).
 *
 * #5044 — Jeff, on his phone, 2026-11-25 15:51: "pipeline section does not make sense to me".
 * Log evidence: the page said shape "No cards" beside 16 cards and ship "Nothing shipped" above
 * 5 releases. Three rows read the old domain view (/api/chorus/domain/:name), which answers 25
 * fixed names only (principles got a 404, turned silently into "nothing") and drops Done cards
 * before ship counted the Done ones: ship could not move for any domain. Design asked for
 * "<name>-service", which scores 0% for any name. Build read links nothing writes any more.
 *
 * Dependencies injected:
 *   fetcher       — async (url) => body | null   (this server's own routes, the /owl model proxy
 *                   among them; null = the read failed)
 *   envelope      — (query_name, data, duration_ms, extra) => wrapped body
 *   now           — () => number
 *
 * Each row, and the fold that reads the same thing:
 *   shape   the board's domain:<name> cards, Done included      (Cards)      5 or more: complete
 *   design  the Domain row's completeness, under its own name                80% or more: complete
 *   build   code files + test files + routes carrying hasDomain (Code, Tests, API Contract)   3 or more: complete
 *   prove   alert rules                                         (Alerts)     1 or more: complete
 *   ship    releases                                            (Releases)   1 or more: complete
 * A read that fails makes its row "could not read <what>", never "not started".
 */
import type { FetchResult } from './codebase-topology';

type Fetcher = (url: string) => Promise<unknown | null>;
type Envelope = (queryName: string, data: unknown, durationMs: number, extra?: Record<string, unknown>) => unknown;

export interface ChorusDomainPipelineDeps {
  fetcher: Fetcher;
  envelope: Envelope;
  now?: () => number;
}

type StageName = 'shape' | 'design' | 'build' | 'prove' | 'ship';
type StageStatus = 'not started' | 'in progress' | 'complete' | 'could not read';

interface Stage {
  name: StageName;
  status: StageStatus;
  evidence: number;
  detail: Record<string, unknown>;
  summary: string;
}

const CLOSED = new Set(['Done', "Won't Do"]);

function byCount(evidence: number, complete: number): StageStatus {
  if (evidence === 0) return 'not started';
  return evidence >= complete ? 'complete' : 'in progress';
}

function unread(name: StageName, what: string): Stage {
  return { name, status: 'could not read', evidence: 0, detail: { failed: what }, summary: `could not read ${what}` };
}

/** The list a read carries under `key`, at the top or inside `data`; null when it carries none. */
function listOf(res: unknown, key: string): unknown[] | null {
  // the named field of an object, looked up among its own entries (never obj[key])
  const field = (o: unknown): unknown => (o && typeof o === 'object' ? Object.entries(o).find(([k]) => k === key)?.[1] : undefined);
  const top = field(res);
  if (Array.isArray(top)) return top;
  const inner = field((res as { data?: unknown } | null)?.data);
  return Array.isArray(inner) ? inner : null;
}

/** Rows of the model door that name this domain: hasDomain is one name or several, bare or an IRI. */
function onDomain(rows: unknown[], name: string): unknown[] {
  const tail = (v: unknown): string => (typeof v === 'string' ? v : '').split('#').pop()!.replace(/^chorus:/, '');
  return rows.filter((r) => {
    const row = r as { hasDomain?: unknown; links?: { hasDomain?: unknown } };
    const named = row.hasDomain ?? row.links?.hasDomain ?? [];
    return (Array.isArray(named) ? named : [named]).some((v) => tail(v) === name);
  });
}

function buildShapeStage(cardsRes: unknown): Stage {
  const cards = listOf(cardsRes, 'cards') as Array<{ status?: string }> | null;
  if (!cards) return unread('shape', 'cards');
  const total = cards.length;
  const done = cards.filter((c) => c.status === 'Done').length;
  const open = cards.filter((c) => !CLOSED.has(c.status ?? '')).length;
  return {
    name: 'shape',
    status: byCount(total, 5),
    evidence: total,
    detail: { total_cards: total, open, done },
    summary: total === 0 ? 'No cards' : `${total} cards (${open} open, ${done} done)`,
  };
}

function buildDesignStage(compRes: unknown): Stage {
  const data = compRes as { percentage?: unknown; present?: unknown[]; missing?: unknown[] } | null;
  if (!data || typeof data.percentage !== 'number') return unread('design', 'completeness');
  const pct = data.percentage;
  const present = data.present ?? [];
  const missing = data.missing ?? [];
  return {
    name: 'design',
    status: byCount(pct, 80),
    evidence: pct,
    detail: { percentage: pct, present, missing },
    summary: `${pct}%: ${present.length} present, ${missing.length} missing`,
  };
}

function buildBuildStage(name: string, filesRes: unknown, testsRes: unknown, endpointsRes: unknown): Stage {
  const files = listOf(filesRes, 'data');
  if (!files) return unread('build', 'code files');
  const tests = (testsRes as { _meta?: { count?: unknown } } | null)?._meta?.count;
  if (typeof tests !== 'number') return unread('build', 'tests');
  const endpoints = listOf(endpointsRes, 'data');
  if (!endpoints) return unread('build', 'routes');
  const code = onDomain(files, name).length;
  const routes = onDomain(endpoints, name).length;
  const total = code + tests + routes;
  return {
    name: 'build',
    status: byCount(total, 3),
    evidence: total,
    detail: { code, tests, endpoints: routes },
    summary: total === 0 ? 'No code' : `${code} files, ${tests} test files, ${routes} routes`,
  };
}

function buildProveStage(alertsRes: unknown): Stage {
  const alerts = listOf(alertsRes, 'alerts');
  if (!alerts) return unread('prove', 'alerts');
  const count = alerts.length;
  return {
    name: 'prove',
    status: byCount(count, 1),
    evidence: count,
    detail: { alerts: count },
    summary: count === 0 ? 'No alert rules' : `${count} alert rules`,
  };
}

function buildShipStage(releasesRes: unknown): Stage {
  const releases = listOf(releasesRes, 'releases') as Array<{ cardId?: string }> | null;
  if (!releases) return unread('ship', 'releases');
  const count = releases.length;
  const newest = releases[0]?.cardId;   // the releases read answers newest first
  return {
    name: 'ship',
    status: byCount(count, 1),
    evidence: count,
    detail: { releases: count, newest: newest ?? null },
    summary: count === 0 ? 'No releases' : `${count} releases${newest ? `, newest #${newest}` : ''}`,
  };
}

export async function fetchChorusDomainPipeline(
  deps: ChorusDomainPipelineDeps,
  name: string,
): Promise<FetchResult> {
  const now = deps.now ?? Date.now;
  const start = now();

  const sdId = name.toLowerCase();
  const dom = `/api/chorus/domain/${sdId}`;
  // the same filter the folds send the model door (facets.js forDomain)
  const forDomain = `?hasDomain=${encodeURIComponent(sdId)}&limit=5000`;

  const [cardsRes, compRes, filesRes, testsRes, endpointsRes, alertsRes, releasesRes] = await Promise.all([
    deps.fetcher(`${dom}/cards`),
    // the Domain row scored against its shape, asked under the domain's own name
    deps.fetcher(`/owl/domains/domains/${encodeURIComponent(sdId)}/completeness`),
    deps.fetcher(`/owl/code/files${forDomain}`),
    deps.fetcher(`${dom}/tests`),
    deps.fetcher(`/owl/code/endpoints${forDomain}`),
    deps.fetcher(`${dom}/alerts`),
    deps.fetcher(`${dom}/releases`),
  ]);

  const stages: Stage[] = [
    buildShapeStage(cardsRes),
    buildDesignStage(compRes),
    buildBuildStage(sdId, filesRes, testsRes, endpointsRes),
    buildProveStage(alertsRes),
    buildShipStage(releasesRes),
  ];

  return {
    status: 200,
    body: deps.envelope('domain-pipeline', { domain: sdId, stages }, now() - start, { count: 5 }),
  };
}
