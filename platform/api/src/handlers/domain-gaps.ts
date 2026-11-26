/**
 * #5045 — GET /api/chorus/domain/:name/gaps: the Gaps fold on /athena/domain.html. A gap on a
 * domain is the board card that will close it: the Domain row holds the card's number
 * (chorus:gapCard) and nothing else. This read joins those numbers to the board itself, so the
 * fold says each card's title, owner and standing as they are now, and a gap whose card is done
 * reads as closed. Before, the fold showed a sentence typed on the row (chorus:gaps) that nobody
 * kept true (Jeff, 2026-11-25 15:47: "how do we resolve gaps").
 * A store that cannot be read is an error (502), never an empty list: "no gaps" is a claim.
 */
// @domain: domains

import type { BoardCard } from './domain-cards';
import { isDomainName } from './util';

export interface GapsDeps {
  sparql: (query: string) => Promise<{ results: { bindings: Array<{ n: { value: string } }> } }>;
  getCards: () => BoardCard[];
}

const CLOSED = new Set(['Done', "Won't Do"]);

export async function fetchDomainGaps(deps: GapsDeps, name: string): Promise<{ status: number; body: unknown }> {
  if (!isDomainName(name)) return { status: 400, body: { error: 'domain must be a domain name, e.g. principles' } };
  let numbers: string[];
  try {
    const q = `PREFIX chorus: <https://jeffbridwell.com/chorus#>
SELECT DISTINCT ?n WHERE { GRAPH ?g { <https://jeffbridwell.com/chorus#${name}> chorus:gapCard ?n } }`;
    numbers = (await deps.sparql(q)).results.bindings.map((b) => String(b.n.value));
  } catch (err) {
    return { status: 502, body: { domain: name, error: `the model could not be read: ${err instanceof Error ? err.message : String(err)}` } };
  }
  const board = new Map(deps.getCards().map((c) => [String(c.id), c]));
  const gaps = numbers.map((id) => {
    const c = board.get(id);
    if (!c) return { id, title: null, owner: null, status: null, state: 'not on the board' };
    return { id, title: c.title, owner: c.owner, status: c.status, state: CLOSED.has(c.status) ? 'closed' : 'open' };
  });
  // open first, then by number
  const rank = (s: string): number => (s === 'open' ? 0 : s === 'not on the board' ? 1 : 2);
  gaps.sort((a, b) => rank(a.state) - rank(b.state) || Number(a.id) - Number(b.id));
  return { status: 200, body: { domain: name, gaps } };
}
