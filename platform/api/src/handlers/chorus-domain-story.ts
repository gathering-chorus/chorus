/**
 * GET /api/chorus/domain-story/:domain — institutional memory for a domain (#2188).
 *
 * Dependencies injected:
 *   getCards — () => Array<{id, title, status, owner, tags}>
 *   db       — better-sqlite3 Database (nullable)
 *   scanSpine — () => SpineScan — bounded tail read of the spine (#3819)
 *
 * Behavior:
 *   - Lists cards tagged domain:<name>
 *   - Pulls up to `limit` message mentions (default 100, max 500)
 *   - Merges spine events for the extracted card ids
 *   - Each source silently skipped on failure
 *   - Timeline sorted by timestamp ASC (empty timestamp sorts first, matches legacy behavior)
 */
import type Database from 'better-sqlite3';
import type { FetchResult } from './codebase-topology';
import type { SpineScan } from './spine-scan';

export interface BoardCard {
  id: string;
  title: string;
  status: string;
  owner: string;
  tags: string; // space/comma-delimited; matched via substring
}

export interface ChorusDomainStoryDeps {
  getCards: () => BoardCard[];
  db: Database.Database | null;
  /** #3819 — bounded tail scan; replaces the whole-file readLog(). */
  scanSpine: () => SpineScan;
}

interface MentionEntry {
  timestamp: string;
  role: string;
  text: string;
}

interface CardEntry {
  index: number;
  title: string;
  status: string;
  owner: string;
  created: string;
}

interface TimelineEntry {
  timestamp: string;
  source: string;
  text: string;
  role?: string;
  card?: number;
}

function collectCardEntries(cardSources: ReturnType<ChorusDomainStoryDeps['getCards']>, domain: string, timeline: TimelineEntry[]): CardEntry[] {
  const cards: CardEntry[] = [];
  for (const c of cardSources.filter((c) => c.tags.includes(`domain:${domain}`))) {
    const card: CardEntry = {
      index: parseInt(c.id, 10),
      title: c.title,
      status: c.status,
      owner: c.owner,
      created: '',
    };
    cards.push(card);
    timeline.push({
      timestamp: '',
      source: 'card',
      text: `#${c.id} ${c.title} [${c.status}]`,
      role: c.owner,
      card: card.index,
    });
  }
  return cards;
}

function isStoryNoise(text: string): boolean {
  if (text.startsWith('<system-reminder>')) return true;
  if (text.startsWith('Base directory for this skill:')) return true;
  if (text.length < 20) return true;
  return false;
}

function collectMentions(db: ChorusDomainStoryDeps['db'], domain: string, limit: number, timeline: TimelineEntry[]): MentionEntry[] {
  const mentions: MentionEntry[] = [];
  if (!db) return mentions;
  try {
    const rows = db.prepare(`
      SELECT author, content, timestamp, role
      FROM messages
      WHERE content LIKE ?
      ORDER BY timestamp ASC
      LIMIT ?
    `).all(`%${domain}%`, limit) as Array<{ author: string; content: string; timestamp: string; role: string }>;
    for (const m of rows) {
      const text = m.content.trim();
      if (isStoryNoise(text)) continue;
      const mention: MentionEntry = {
        timestamp: m.timestamp,
        role: m.author === 'user' ? 'jeff' : m.role,
        text: text.slice(0, 300),
      };
      mentions.push(mention);
      timeline.push({
        timestamp: m.timestamp,
        source: 'chorus-index',
        text: mention.text,
        role: mention.role,
      });
    }
  } catch { /* db unavailable */ }
  return mentions;
}

// #3819 — bounded tail scan, same reason as chorus-card-story: this read the
// whole 732MB spine per request.
function collectSpineEvents(scanSpine: ChorusDomainStoryDeps['scanSpine'], cards: CardEntry[], timeline: TimelineEntry[]): boolean {
  try {
    const scan = scanSpine();
    const cardIds = new Set(cards.map((c) => c.index));
    for (const line of scan.lines) {
      try {
        const parsed = JSON.parse(line);
        if (!parsed.event || !String(parsed.event).startsWith('card.')) continue;
        // #3819 — card_id (number) is what the spine writes; `card` was never
        // present, so this matched nothing even when the read succeeded.
        const cardId = Number(parsed.card_id ?? parsed.card ?? 0);
        if (!cardIds.has(cardId)) continue;
        timeline.push({
          timestamp: parsed.timestamp,
          source: 'spine',
          text: parsed.event,
          role: parsed.role,
          card: cardId,
        });
      } catch { /* skip malformed */ }
    }
    return scan.truncated;
  } catch { /* log unreadable */ }
  return false;
}

export function fetchChorusDomainStory(
  deps: ChorusDomainStoryDeps,
  domainParam: string,
  limitParam?: string,
): FetchResult {
  const domain = domainParam.toLowerCase();
  const limit = Math.min(parseInt(limitParam || '100', 10), 500);
  const timeline: TimelineEntry[] = [];

  const cards = collectCardEntries(deps.getCards(), domain, timeline);
  const mentions = collectMentions(deps.db, domain, limit, timeline);
  const spineTruncated = collectSpineEvents(deps.scanSpine, cards, timeline);
  timeline.sort((a, b) => (a.timestamp || '').localeCompare(b.timestamp || ''));

  return {
    status: 200,
    body: {
      domain,
      cards,
      mentions,
      timeline,
      count: timeline.length,
      // #3819 — a shortened search must not look like an empty history.
      spineTruncated,
      card_count: cards.length,
      mention_count: mentions.length,
    },
  };
}
