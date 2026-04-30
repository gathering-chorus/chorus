/**
 * GET /api/chorus/domain/:name/releases — ACP commits touching this domain (#2188).
 *
 * Dependencies injected:
 *   gitLog   — () => string (git log --format="%H|%aI|%s", one commit per line)
 *   getCards — () => Array<BoardCard-ish with id + tags string>
 *   envelope — (queryName, data, duration, extra) => wrapped body
 *   now      — () => number
 *
 * Behavior:
 *   - Domain name stripped of -domain/-service/-analytics suffix
 *   - ACP pattern: role: acp #<cardId> ... — <title>
 *   - Cross-ref each ACP to cards' domain:/sequence: tags — exact match → gates "passed"
 *   - Cards without tag match but title contains domain name → gates "unknown"
 *   - Any throw → empty envelope, original subdomain name preserved
 */
import type { FetchResult } from './codebase-topology';
import { resolveDomainIdentity } from './domain-identity';

export interface ReleasesBoardCard {
  id: string;
  tags: string;
}

type GitLogFn = () => string;
type GetCards = () => ReleasesBoardCard[];
type Envelope = (queryName: string, data: unknown, durationMs: number, extra?: Record<string, unknown>) => unknown;

export interface ChorusDomainReleasesDeps {
  gitLog: GitLogFn;
  getCards: GetCards;
  envelope: Envelope;
  now?: () => number;
}

const ACP_PATTERN = /^([a-f0-9]+)\|(.+?)\|(\w+): acp #(\d+)(?:.*?)— (.+)$/;

interface AcpEntry {
  commit: string;
  timestamp: string;
  role: string;
  cardId: string;
  title: string;
}

interface ReleaseEntry extends AcpEntry {
  gates: 'passed' | 'unknown';
}

// #2627: extracted parsing + indexing + matching into helpers; orchestrator
// becomes a linear pipeline.

function parseAcpLog(gitLog: string): AcpEntry[] {
  const out: AcpEntry[] = [];
  for (const line of gitLog.split('\n')) {
    const m = line.match(ACP_PATTERN);
    if (m) {
      out.push({
        commit: m[1].slice(0, 8),
        timestamp: m[2],
        role: m[3],
        cardId: m[4],
        title: m[5].trim(),
      });
    }
  }
  return out;
}

function indexCardsByDomain(cards: Array<{ id: string; tags: string }>): Map<string, string[]> {
  const idx = new Map<string, string[]>();
  for (const c of cards) {
    const domains: string[] = [];
    for (const dm of c.tags.matchAll(/domain:(\w[\w-]*)/g)) domains.push(dm[1]);
    for (const sm of c.tags.matchAll(/sequence:(\w[\w-]*)/g)) domains.push(sm[1]);
    if (domains.length > 0) idx.set(c.id, domains);
  }
  return idx;
}

function matchAcpToDomain(
  acp: AcpEntry,
  cardDomains: string[] | undefined,
  searchTerms: string[],
): ReleaseEntry | null {
  if (cardDomains?.some((d) => searchTerms.includes(d))) {
    return { ...acp, gates: 'passed' };
  }
  if (!cardDomains && searchTerms.some((t) => acp.title.toLowerCase().includes(t))) {
    return { ...acp, gates: 'unknown' };
  }
  return null;
}

export function fetchChorusDomainReleases(
  deps: ChorusDomainReleasesDeps,
  name: string,
): FetchResult {
  const now = deps.now ?? Date.now;
  const start = now();
  // #2430: shared resolver. Pre-refactor this had the same -analytics/-service
  // strip bug cards.ts had AND no alias support — loom-principles releases
  // always returned 0 because commits are tagged sequence:loom, not
  // sequence:loom-principles, and the handler only did exact includes().
  const identity = resolveDomainIdentity(name);
  const searchTerms = [identity.primary, ...identity.aliases];

  try {
    const allAcps = parseAcpLog(deps.gitLog());
    const cardsDomainIndex = indexCardsByDomain(deps.getCards());
    const releases: ReleaseEntry[] = [];
    for (const acp of allAcps) {
      const r = matchAcpToDomain(acp, cardsDomainIndex.get(acp.cardId), searchTerms);
      if (r) releases.push(r);
    }
    return {
      status: 200,
      body: deps.envelope(
        'domain-releases',
        { subdomain: name, releases },
        now() - start,
        { count: releases.length, total_acps: allAcps.length },
      ),
    };
  } catch {
    return {
      status: 200,
      body: deps.envelope(
        'domain-releases',
        { subdomain: name, releases: [] },
        now() - start,
        { count: 0 },
      ),
    };
  }
}
