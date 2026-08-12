/**
 * GET /api/chorus/card-story/:id — memory timeline for a card (#2188).
 *
 * Dependencies injected:
 *   loadCard   — async (cardId) => card metadata + comments (null if unavailable)
 *   db         — better-sqlite3 Database for chorus-index mentions (null OK)
 *   scanSpine  — () => SpineScan — a bounded read of the spine's tail (#3819)
 *   loadNudges — async () => Array<{from, to, text, timestamp}> (empty OK)
 *
 * Behavior:
 *   - Invalid numeric id → 400
 *   - Each data source is optional; failures silently skip that section
 *   - Timeline sorted by timestamp ASC
 *   - sources dedup from timeline entries
 */
import type Database from 'better-sqlite3';
import type { FetchResult } from './codebase-topology';
import { parseMatching, type SpineScan } from './spine-scan';

export interface CardComment {
  text?: string;
  created?: string;
  author?: string;
}

export interface CardMeta {
  title?: string;
  owner?: string;
  status?: string;
  created?: string;
  domains?: string[];
  comments?: CardComment[];
}

export interface NudgeMessage {
  from: string;
  to: string;
  text: string;
  timestamp: string;
}

export interface ChorusCardStoryDeps {
  loadCard: (cardId: number) => Promise<CardMeta | null>;
  db: Database.Database | null;
  /** #3819 — a BOUNDED scan of the spine's tail. Replaces readLog(), which
   *  returned all 732MB of a never-rotated file and froze the event loop. */
  scanSpine: () => SpineScan;
  loadNudges: () => Promise<NudgeMessage[]>;
}

interface TimelineEntry {
  timestamp: string;
  source: string;
  text: string;
  role?: string;
  event?: string;
}

type StoryMeta = { title: string; owner: string; status: string; domain: string };

async function collectCardData(deps: ChorusCardStoryDeps, cardId: number, timeline: TimelineEntry[]): Promise<StoryMeta> {
  const meta: StoryMeta = { title: '', owner: '', status: '', domain: '' };
  try {
    const card = await deps.loadCard(cardId);
    if (!card) return meta;
    meta.title = card.title || '';
    meta.owner = (card.owner || '').toLowerCase();
    meta.status = card.status || '';
    for (const d of card.domains || []) {
      const m = d.match(/domain:(\w+)/i);
      if (m) meta.domain = m[1];
    }
    for (const c of card.comments || []) {
      if (c.text && c.text.length > 5) {
        timeline.push({
          timestamp: c.created || card.created || '',
          source: 'vikunja',
          text: c.text.slice(0, 500),
          role: c.author,
        });
      }
    }
  } catch { /* card unavailable */ }
  return meta;
}

/** How many FTS rows to scan before sorting. Bounded so a common card number
 *  costs the same as a rare one. */
export const MENTION_SCAN_CAP = 300;

/**
 * Messages mentioning this card.
 *
 * #3819 — this was `WHERE content LIKE '%#3810%'`: an unindexed scan over
 * 1.26M rows, synchronously, on every card story. THAT is the 3-8 second
 * event-loop freeze the alerts have been naming for days, not the spine read
 * one line below it — the profiler reports the enclosing frame, so the spine
 * call took the blame twice.
 *
 * The cure already existed in this codebase and nobody carried it across:
 * #3054 removed exactly this LIKE-scan from the crawl handler and #3055
 * bounded its replacement. Same query shape here, same fix, one file over.
 *
 * FTS5 MATCH over a bounded scan, then sort. The LIMIT inside the subquery is
 * what makes it early-terminate; sorting the full match set is itself the slow
 * part for a common term.
 *
 * The FTS tokenizer DROPS the '#', so a match for "#3810" also returns rows
 * containing a bare 3810 — measured: 15 of 50 rows were prose like "3798,
 * 3807, 3810, and Silas's 3805", which is not a mention of this card. So the
 * query is a cheap FILTER and the exact test happens below in JS. Shipping
 * without that check would have been faster AND wrong, which is the failure
 * this whole card keeps circling.
 */
function collectIndexMentions(deps: ChorusCardStoryDeps, cardId: number, timeline: TimelineEntry[]): void {
  if (!deps.db) return;
  try {
    const rows = deps.db.prepare(
      `SELECT m.author, m.content, m.timestamp, m.role
         FROM (SELECT rowid FROM messages_fts WHERE messages_fts MATCH ? LIMIT ?) f
         JOIN messages m ON f.rowid = m.id
        ORDER BY m.timestamp ASC LIMIT 50`,
    ).all(`"#${cardId}"`, MENTION_SCAN_CAP) as Array<{ author: string; content: string; timestamp: string; role: string }>;
    const mention = `#${cardId}`;
    for (const m of rows) {
      // The exact test the tokenizer cannot do.
      if (!m.content.includes(mention)) continue;
      const text = m.content.trim();
      if (text.startsWith('<system-reminder>') || text.startsWith('Base directory for this skill:') || text.length < 10) continue;
      timeline.push({
        timestamp: m.timestamp,
        source: 'chorus-index',
        text: text.slice(0, 500),
        role: m.author === 'user' ? 'jeff' : m.role,
      });
    }
  } catch { /* db read failed */ }
}

/**
 * #3819 — scan the spine's tail rather than reading all 732MB of it.
 *
 * This used to call deps.readLog(), which pulled the entire never-rotated spine
 * into a string and split it — 3-8 seconds of frozen API per request, growing
 * daily. Card events are sparse and recent, so a bounded tail answers the
 * question the caller actually asked.
 *
 * `spineTruncated` is returned to the caller, not swallowed: a story that
 * stopped looking must not be indistinguishable from a card that nothing
 * happened to.
 */
function collectSpineEvents(deps: ChorusCardStoryDeps, cardId: number, timeline: TimelineEntry[]): boolean {
  const scan = deps.scanSpine();
  // #3819 — the predicate matches the shape the spine ACTUALLY writes:
  // {"event":"card.demo.started", ..., "card_id":3813} — card_id, and a NUMBER.
  // The old predicate looked for `card=N` or `"card":"N"`, neither of which
  // appears in this log. So even before the file outgrew a JS string, a card's
  // story matched nothing: the handler spent seconds to return an empty list.
  // The `card=` form is kept because older lines carry it.
  const events = parseMatching(
    scan.lines,
    // The cheap string test may OVER-match: "card_id":38130 contains
    // "card_id":3813. That is fine and deliberate — it is a filter, not the
    // decision. The taker below re-checks the parsed number, so a prefix
    // collision costs one wasted JSON.parse instead of putting another card's
    // event in this card's story.
    (line) => line.includes(`"card_id":${cardId}`)
      || line.includes(`card=${cardId}`)
      || line.includes(`"card":"${cardId}"`),
    (parsed) => {
      // Narrow to string BEFORE using: these come off JSON, so a field could be
      // an object, and String({}) is "[object Object]" — an event name that
      // matches nothing and reads as data in the timeline.
      const event = typeof parsed.event === 'string' ? parsed.event : '';
      if (!event.startsWith('card.')) return null;
      const parsedId = Number(parsed.card_id ?? parsed.card ?? NaN);
      if (Number.isFinite(parsedId) && parsedId !== cardId) return null;
      return {
        timestamp: typeof parsed.timestamp === 'string' ? parsed.timestamp : '',
        source: 'spine' as const,
        text: event,
        role: typeof parsed.role === 'string' ? parsed.role : undefined,
        event,
      };
    },
  );
  for (const e of events) timeline.push(e as TimelineEntry);
  return scan.truncated;
}

async function collectNudges(deps: ChorusCardStoryDeps, cardId: number, timeline: TimelineEntry[]): Promise<void> {
  try {
    const nudges = await deps.loadNudges();
    for (const msg of nudges) {
      if (msg.text.includes(`#${cardId}`)) {
        timeline.push({ timestamp: msg.timestamp, source: 'nudge', text: msg.text.slice(0, 500), role: msg.from });
      }
    }
  } catch { /* messaging unavailable */ }
}

export async function fetchChorusCardStory(
  deps: ChorusCardStoryDeps,
  cardIdParam: string,
): Promise<FetchResult> {
  const cardId = parseInt(cardIdParam, 10);
  if (isNaN(cardId)) return { status: 400, body: { error: 'Invalid card ID' } };

  const timeline: TimelineEntry[] = [];
  const meta = await collectCardData(deps, cardId, timeline);
  collectIndexMentions(deps, cardId, timeline);
  const spineTruncated = collectSpineEvents(deps, cardId, timeline);
  await collectNudges(deps, cardId, timeline);
  timeline.sort((a, b) => a.timestamp.localeCompare(b.timestamp));

  return {
    status: 200,
    body: {
      card: cardId,
      ...meta,
      timeline,
      sources: [...new Set(timeline.map((e) => e.source))],
      count: timeline.length,
      // #3819 — say when the spine window did not reach the beginning. Without
      // this the caller cannot tell an empty history from a shortened search.
      spineTruncated,
    },
  };
}
