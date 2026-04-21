/**
 * GET /api/chorus/domain/:name/pipeline — 5-stage domain maturity view (#2188).
 *
 * Dependencies injected:
 *   fetcher            — async (url) => any | null   (HTTP aggregator over sibling endpoints)
 *   resolveSubdomainId — async (name) => string (throws if unresolvable)
 *   envelope           — (query_name, data, duration_ms, extra) => wrapped body
 *   now                — () => number
 *
 * Behavior:
 *   - Parallel fetch: cards, completeness, code, tests, services (endpoints), alerts
 *   - stage.shape:  by total_cards (≥5 complete)
 *   - stage.design: by completeness % (≥80 complete)
 *   - stage.build:  code+tests+endpoints (≥3 complete)
 *   - stage.prove:  alert count (≥1 complete)
 *   - stage.ship:   done/total cards (≥50% complete)
 *   - On resolveSubdomainId failure: return 5 "not_started" stages (not an error)
 */
import type { FetchResult } from './codebase-topology';

type Fetcher = (url: string) => Promise<unknown | null>;
type ResolveSubdomainId = (name: string) => Promise<string>;
type Envelope = (queryName: string, data: unknown, durationMs: number, extra?: Record<string, unknown>) => unknown;

export interface ChorusDomainPipelineDeps {
  fetcher: Fetcher;
  resolveSubdomainId: ResolveSubdomainId;
  envelope: Envelope;
  now?: () => number;
}

interface Stage {
  name: 'shape' | 'design' | 'build' | 'prove' | 'ship';
  status: 'not_started' | 'in_progress' | 'complete';
  evidence: number;
  detail: Record<string, unknown>;
  summary: string;
}

function stageStatus(evidence: number, threshold = 1): Stage['status'] {
  return evidence === 0 ? 'not_started' : evidence >= threshold ? 'complete' : 'in_progress';
}

function thresholdStatus(evidence: number, threshold: number): Stage['status'] {
  if (evidence === 0) return 'not_started';
  return evidence >= threshold ? 'complete' : 'in_progress';
}

function emptyStages(): Stage[] {
  return (['shape', 'design', 'build', 'prove', 'ship'] as const).map((name) => ({
    name,
    status: 'not_started' as const,
    evidence: 0,
    detail: {},
    summary: 'No data',
  }));
}

type CardRow = { status?: string };

function extractCards(cardsRes: unknown): CardRow[] {
  const withCards = (cardsRes as { data?: { cards?: CardRow[] } } | null)?.data?.cards;
  if (withCards) return withCards;
  const rawData = (cardsRes as { data?: unknown } | null)?.data;
  return (rawData as CardRow[] | undefined) ?? [];
}

function buildShapeStage(cards: CardRow[]): Stage {
  const total = cards.length;
  const done = cards.filter((c) => c.status === 'Done').length;
  const wip = cards.filter((c) => c.status === 'WIP').length;
  return {
    name: 'shape',
    status: thresholdStatus(total, 5),
    evidence: total,
    detail: { total_cards: total, wip, done },
    summary: total === 0 ? 'No cards' : `${total} cards (${wip} WIP, ${done} done)`,
  };
}

function buildDesignStage(compRes: unknown): Stage {
  const data = (compRes as { data?: { percentage?: number; present?: unknown[]; missing?: unknown[] } } | null)?.data;
  const pct = data?.percentage ?? 0;
  const present = data?.present ?? [];
  const missing = data?.missing ?? [];
  return {
    name: 'design',
    status: thresholdStatus(pct, 80),
    evidence: pct,
    detail: { percentage: pct, present, missing },
    summary: pct === 0 ? 'Not started' : `${pct}% — ${present.length} present, ${missing.length} missing`,
  };
}

function metaCount(res: unknown, key: 'count' | 'source_count' = 'count'): number {
  return (res as { _meta?: Record<string, number> } | null)?._meta?.[key] ?? 0;
}

function buildBuildStage(codeRes: unknown, testsRes: unknown, endpointsRes: unknown): Stage {
  const code = metaCount(codeRes, 'source_count');
  const tests = metaCount(testsRes);
  const endpoints = metaCount(endpointsRes);
  const total = code + tests + endpoints;
  return {
    name: 'build',
    status: stageStatus(total, 3),
    evidence: total,
    detail: { code, tests, endpoints },
    summary: total === 0 ? 'No code discovered' : `${code} source, ${tests} tests, ${endpoints} endpoints`,
  };
}

function buildProveStage(alertsRes: unknown): Stage {
  const count = metaCount(alertsRes);
  return {
    name: 'prove',
    status: stageStatus(count),
    evidence: count,
    detail: { alerts: count },
    summary: count === 0 ? 'No alert coverage' : `${count} alert rules`,
  };
}

function buildShipStage(cards: CardRow[]): Stage {
  const total = cards.length;
  const done = cards.filter((c) => c.status === 'Done').length;
  const ratio = total > 0 ? Math.round((done / total) * 100) : 0;
  let status: Stage['status'] = 'in_progress';
  if (done === 0) status = 'not_started';
  else if (done >= total * 0.5) status = 'complete';
  return {
    name: 'ship',
    status,
    evidence: done,
    detail: { done, total, ratio },
    summary: done === 0 ? 'Nothing shipped' : `${done}/${total} cards shipped (${ratio}%)`,
  };
}

export async function fetchChorusDomainPipeline(
  deps: ChorusDomainPipelineDeps,
  name: string,
): Promise<FetchResult> {
  const now = deps.now ?? Date.now;
  const start = now();

  let sdId: string;
  try {
    sdId = await deps.resolveSubdomainId(name);
  } catch {
    return {
      status: 200,
      body: deps.envelope(
        'domain-pipeline',
        { subdomain: name, stages: emptyStages() },
        now() - start,
        { count: 5 },
      ),
    };
  }

  const [cardsRes, compRes, codeRes, testsRes, endpointsRes, alertsRes] = await Promise.all([
    deps.fetcher(`/api/athena/subdomains/${sdId}/cards`),
    deps.fetcher(`/api/athena/subdomains/${sdId}/completeness`),
    deps.fetcher(`/api/chorus/domain/${name}/code`),
    deps.fetcher(`/api/chorus/domain/${name}/tests`),
    deps.fetcher(`/api/chorus/domain/${name}/services`),
    deps.fetcher(`/api/chorus/domain/${name}/alerts`),
  ]);

  const cards = extractCards(cardsRes);
  const stages: Stage[] = [
    buildShapeStage(cards),
    buildDesignStage(compRes),
    buildBuildStage(codeRes, testsRes, endpointsRes),
    buildProveStage(alertsRes),
    buildShipStage(cards),
  ];

  return {
    status: 200,
    body: deps.envelope('domain-pipeline', { subdomain: sdId, stages }, now() - start, { count: 5 }),
  };
}
