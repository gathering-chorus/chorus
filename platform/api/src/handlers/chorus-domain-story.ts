/**
 * GET /api/chorus/domain-story/:domain — institutional memory for a domain (#2188).
 *
 * Dependencies injected:
 *   getCards — () => Array<{id, title, status, owner, tags}>
 *   db       — better-sqlite3 Database (nullable)
 *   readLog  — () => string | null
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
  readLog: () => string | null;
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

// eslint-disable-next-line max-lines-per-function -- #2288 pre-existing threshold violation, tracked for refactor
export function fetchChorusDomainStory(
  deps: ChorusDomainStoryDeps,
  domainParam: string,
  limitParam?: string,
): FetchResult {
  const domain = domainParam.toLowerCase();
  const limit = Math.min(parseInt(limitParam || '100', 10), 500);

  const cards: CardEntry[] = [];
  const mentions: MentionEntry[] = [];
  const timeline: TimelineEntry[] = [];

  for (const c of deps.getCards().filter((c) => c.tags.includes(`domain:${domain}`))) {
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
      card: parseInt(c.id, 10),
    });
  }

  if (deps.db) {
    try {
      const rows = deps.db.prepare(`
        SELECT author, content, timestamp, role
        FROM messages
        WHERE content LIKE ?
        ORDER BY timestamp ASC
        LIMIT ?
      `).all(`%${domain}%`, limit) as Array<{ author: string; content: string; timestamp: string; role: string }>;

      for (const m of rows) {
        const text = m.content.trim();
        if (text.startsWith('<system-reminder>')) continue;
        if (text.startsWith('Base directory for this skill:')) continue;
        if (text.length < 20) continue;

        const mention: MentionEntry = {
          timestamp: m.timestamp,
          role: m.author === 'user' ? 'jeff' : m.role,
          text: text.slice(0, 300),
        };
        mentions.push(mention);
        timeline.push({
          timestamp: m.timestamp,
          source: 'chorus-index',
          text: text.slice(0, 300),
          role: mention.role,
        });
      }
    } catch { /* db unavailable */ }
  }

  try {
    const log = deps.readLog();
    if (log !== null) {
      const cardIds = new Set(cards.map((c) => c.index));
      for (const line of log.split('\n')) {
        try {
          const parsed = JSON.parse(line);
          // eslint-disable-next-line max-depth -- #2288 pre-existing threshold violation, tracked for refactor
          if (!parsed.event || !String(parsed.event).startsWith('card.')) continue;
          const cardId = parseInt(parsed.card || '0', 10);
          // eslint-disable-next-line max-depth -- #2288 pre-existing threshold violation, tracked for refactor
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
    }
  } catch { /* log unreadable */ }

  timeline.sort((a, b) => (a.timestamp || '').localeCompare(b.timestamp || ''));

  return {
    status: 200,
    body: {
      domain,
      cards,
      mentions,
      timeline,
      count: timeline.length,
      card_count: cards.length,
      mention_count: mentions.length,
    },
  };
}
