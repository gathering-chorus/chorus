// Board card cache (#2096, extracted from server.ts for #2205).
//
// Same cache pattern as health endpoint (#1978): eliminates per-request
// execAsync('cards list') shell-outs. Evidence: /api/chorus/domain/:name
// was 344–536ms without cache; 8–29ms with.
//
// Extraction made the subsystem independently testable with a dep-injected
// runner (no shell, no cards binary, no network). Server.ts holds a single
// instance and starts the refresh interval.

export interface CachedCard {
  id: string;
  title: string;
  status: string;
  owner: string;
  type: string;
  priority: string;
  /** #3149-fix — the card's domain (from the `domain:X` tag). The live source for
   * read-time domain derivation: consumers join an event's card_id to this. */
  domain: string;
  tags: string;
}

export interface BoardCacheDeps {
  /** Run the `cards list` command (or equivalent) and return its stdout. */
  run: () => Promise<string>;
}

export interface BoardCache {
  /** Refresh the cache from `run()`. Swallows errors — keeps the last-good snapshot on failure. */
  refresh: () => Promise<void>;
  /** Return the current card snapshot. Triggers a background refresh on first call when cache is empty. */
  getCards: () => CachedCard[];
  /** Timestamp (Date.now()) of the last successful refresh, or 0 if none. */
  ageMs: () => number;
}

const STATUS_HEADER = /^(WIP|Blocked|Now|Next|Later|Done|Won't Do)\s*\(\d+\)/;
const CARD_ROW = /^(\d+)\s+(.+?)\s+\[([^\]]+)\]$/;

/** Parse one matched card row into a CachedCard. Extracted from the loop so the
 *  per-field tag extraction (owner/type/priority/domain) doesn't inflate the
 *  loop's cognitive complexity (#3149-fix). */
function cardFromRow(cardMatch: RegExpMatchArray, status: string): CachedCard {
  const tags = cardMatch[3];
  const ownerMatch = tags.match(/^(Wren|Silas|Kade|Jeff)/i);
  const typeMatch = tags.match(/type:(\w+)/);
  const priorityMatch = tags.match(/P([1-3])/);
  const domainMatch = tags.match(/domain:([\w-]+)/);
  return {
    id: cardMatch[1],
    title: cardMatch[2].trim(),
    status,
    owner: ownerMatch ? ownerMatch[1].toLowerCase() : '',
    type: typeMatch ? typeMatch[1] : '',
    priority: priorityMatch ? priorityMatch[0] : '',
    domain: domainMatch ? domainMatch[1] : '',
    tags,
  };
}

/**
 * Parse the stdout of `cards list` into a flat array of CachedCard.
 * Pure function — same input always yields the same output.
 */
export function parseCardsListOutput(stdout: string): CachedCard[] {
  const cards: CachedCard[] = [];
  let currentStatus = '';
  for (const line of stdout.split('\n')) {
    const statusMatch = line.match(STATUS_HEADER);
    if (statusMatch) {
      currentStatus = statusMatch[1];
      continue;
    }
    const cardMatch = line.trim().match(CARD_ROW);
    if (!cardMatch) continue;
    cards.push(cardFromRow(cardMatch, currentStatus));
  }
  return cards;
}

/**
 * Build a BoardCache with the supplied runner. State is closure-local — no
 * module-level mutable state, so tests can create as many instances as they
 * need without cross-contamination.
 */
export function createBoardCache(deps: BoardCacheDeps): BoardCache {
  let cards: CachedCard[] = [];
  let lastRefresh = 0;

  const refresh = async (): Promise<void> => {
    try {
      const stdout = await deps.run();
      cards = parseCardsListOutput(stdout);
      lastRefresh = Date.now();
    } catch {
      // Keep the previous snapshot on failure — better stale than empty.
    }
  };

  const getCards = (): CachedCard[] => {
    // Lazy first-refresh trigger: if the cache has never been populated and
    // no refresh has ever completed, kick one off in the background.
    if (cards.length === 0 && lastRefresh === 0) {
      void refresh();
    }
    return cards;
  };

  const ageMs = (): number => lastRefresh;

  return { refresh, getCards, ageMs };
}
