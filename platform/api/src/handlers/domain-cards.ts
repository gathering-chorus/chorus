/**
 * #5021 — GET /api/chorus/domain/:name/cards: the Cards fold on /athena/domain.html. The board's
 * cards carrying the `domain:<name>` label, open first, then by number, newest first. Any domain
 * name is asked of the board itself, never of a list kept here: the fold read
 * /api/chorus/domain/:name, which answers only the 25 names of the v1 registry (principles got a
 * 404) and carries its cards as an object the fold could not read (every domain showed none).
 */
// @domain: cards

export interface BoardCard {
  id: string;
  title: string;
  status: string;
  owner: string;
  priority: string;
  tags: string;
}

const NAME = /^[a-z0-9]+(-[a-z0-9]+)*$/;
const CLOSED = new Set(['Done', "Won't Do"]);

export function fetchDomainCards(getCards: () => BoardCard[], name: string): { status: number; body: unknown } {
  if (!NAME.test(name)) return { status: 400, body: { error: 'domain must be a domain name, e.g. principles' } };
  // the board cache keeps a card's labels as `cards list` prints them: Owner|P2|domain:x|type:fix
  const label = new RegExp(`(^|[|\\s,])domain:${name}(?=$|[|\\s,])`);
  const cards = getCards()
    .filter((c) => label.test(c.tags))
    .sort((a, b) => Number(CLOSED.has(a.status)) - Number(CLOSED.has(b.status)) || Number(b.id) - Number(a.id))
    .map(({ id, title, owner, status, priority }) => ({ id, title, owner, status, priority }));
  return { status: 200, body: { domain: name, cards } };
}
