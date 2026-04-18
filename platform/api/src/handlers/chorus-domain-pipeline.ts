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

function emptyStages(): Stage[] {
  return (['shape', 'design', 'build', 'prove', 'ship'] as const).map((name) => ({
    name,
    status: 'not_started' as const,
    evidence: 0,
    detail: {},
    summary: 'No data',
  }));
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

  type CardRow = { status?: string };
  const cards: CardRow[] =
    ((cardsRes as { data?: { cards?: CardRow[] } } | null)?.data?.cards) ??
    (((cardsRes as { data?: unknown } | null)?.data) as CardRow[] | undefined) ??
    [];
  const totalCards = cards.length;
  const doneCards = cards.filter((c) => c.status === 'Done').length;
  const wipCards = cards.filter((c) => c.status === 'WIP').length;

  const compData = (compRes as { data?: { percentage?: number; present?: unknown[]; missing?: unknown[] } } | null)?.data;
  const completeness = compData?.percentage ?? 0;
  const compPresent = compData?.present ?? [];
  const compMissing = compData?.missing ?? [];

  const codeCount = (codeRes as { _meta?: { source_count?: number } } | null)?._meta?.source_count ?? 0;
  const testCount = (testsRes as { _meta?: { count?: number } } | null)?._meta?.count ?? 0;
  const endpointCount = (endpointsRes as { _meta?: { count?: number } } | null)?._meta?.count ?? 0;
  const alertCount = (alertsRes as { _meta?: { count?: number } } | null)?._meta?.count ?? 0;
  const buildEvidence = codeCount + testCount + endpointCount;

  const stages: Stage[] = [
    {
      name: 'shape',
      status: totalCards === 0 ? 'not_started' : totalCards >= 5 ? 'complete' : 'in_progress',
      evidence: totalCards,
      detail: { total_cards: totalCards, wip: wipCards, done: doneCards },
      summary: totalCards === 0 ? 'No cards' : `${totalCards} cards (${wipCards} WIP, ${doneCards} done)`,
    },
    {
      name: 'design',
      status: completeness === 0 ? 'not_started' : completeness >= 80 ? 'complete' : 'in_progress',
      evidence: completeness,
      detail: { percentage: completeness, present: compPresent, missing: compMissing },
      summary: completeness === 0 ? 'Not started' : `${completeness}% — ${compPresent.length} present, ${compMissing.length} missing`,
    },
    {
      name: 'build',
      status: stageStatus(buildEvidence, 3),
      evidence: buildEvidence,
      detail: { code: codeCount, tests: testCount, endpoints: endpointCount },
      summary: buildEvidence === 0 ? 'No code discovered' : `${codeCount} source, ${testCount} tests, ${endpointCount} endpoints`,
    },
    {
      name: 'prove',
      status: stageStatus(alertCount),
      evidence: alertCount,
      detail: { alerts: alertCount },
      summary: alertCount === 0 ? 'No alert coverage' : `${alertCount} alert rules`,
    },
    {
      name: 'ship',
      status:
        doneCards === 0
          ? 'not_started'
          : doneCards >= totalCards * 0.5
          ? 'complete'
          : 'in_progress',
      evidence: doneCards,
      detail: {
        done: doneCards,
        total: totalCards,
        ratio: totalCards > 0 ? Math.round((doneCards / totalCards) * 100) : 0,
      },
      summary: doneCards === 0
        ? 'Nothing shipped'
        : `${doneCards}/${totalCards} cards shipped (${totalCards > 0 ? Math.round((doneCards / totalCards) * 100) : 0}%)`,
    },
  ];

  return {
    status: 200,
    body: deps.envelope('domain-pipeline', { subdomain: sdId, stages }, now() - start, { count: 5 }),
  };
}
