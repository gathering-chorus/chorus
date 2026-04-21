/**
 * GET /api/chorus/card-story/:id — memory timeline for a card (#2188).
 *
 * Dependencies injected:
 *   loadCard   — async (cardId) => card metadata + comments (null if unavailable)
 *   db         — better-sqlite3 Database for chorus-index mentions (null OK)
 *   readLog    — () => string | null — spine log contents
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
  readLog: () => string | null;
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

function collectIndexMentions(deps: ChorusCardStoryDeps, cardId: number, timeline: TimelineEntry[]): void {
  if (!deps.db) return;
  try {
    const rows = deps.db.prepare(
      'SELECT author, content, timestamp, role FROM messages WHERE content LIKE ? ORDER BY timestamp ASC LIMIT 50',
    ).all(`%#${cardId}%`) as Array<{ author: string; content: string; timestamp: string; role: string }>;
    for (const m of rows) {
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

function collectSpineEvents(deps: ChorusCardStoryDeps, cardId: number, timeline: TimelineEntry[]): void {
  let log: string | null;
  try { log = deps.readLog(); } catch { return; }
  if (log === null) return;
  for (const line of log.split('\n')) {
    if (!line.includes(`card=${cardId}`) && !line.includes(`"card":"${cardId}"`)) continue;
    try {
      const parsed = JSON.parse(line);
      if (parsed.event && String(parsed.event).startsWith('card.')) {
        timeline.push({
          timestamp: parsed.timestamp,
          source: 'spine',
          text: String(parsed.event),
          role: parsed.role,
          event: parsed.event,
        });
      }
    } catch { /* skip malformed line */ }
  }
}

async function collectNudges(deps: ChorusCardStoryDeps, cardId: number, timeline: TimelineEntry[]): Promise<void> {
  try {
    const nudges = await deps.loadNudges();
    for (const msg of nudges) {
      if (msg.text?.includes(`#${cardId}`)) {
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
  collectSpineEvents(deps, cardId, timeline);
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
    },
  };
}
