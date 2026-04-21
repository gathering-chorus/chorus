/**
 * GET /api/chorus/reprompt-analytics — Jeff attention-cost analytics (#2188).
 *
 * Dependencies injected:
 *   db  — better-sqlite3 Database
 *   now — () => number (defaults to Date.now)
 *
 * Query:
 *   days — clamped to [1, 365], default 30
 *
 * Behavior:
 *   - Filters user-author claude messages in wren/silas/kade sessions within days cutoff
 *   - Strips system-reminder blocks, drops messages >500 chars, path-prefixed, single-word
 *   - Classifies each as reprompt | approval | correction | (none)
 *   - Returns headline counts, byRole buckets, daily trend with attentionCost, top 15 phrases, last 20 events reversed
 */
import type Database from 'better-sqlite3';
import type { FetchResult } from './codebase-topology';

const REPROMPT_KEYWORDS = /\bagain\b|\bstill\b|\balready said\b|\btold you\b|\bsame thing\b|\blike i said\b|\brepeat\b|\bi just said\b|\bjust told\b|\bsaid this\b/i;
const APPROVAL_BIGRAMS = /\byes please\b|\byes go\b|\byes do\b|\bgo ahead\b|\byes that\b|\byes card\b/i;
const CORRECTION_PATTERNS = /\bno[\s,]|\bwrong\b|\bthat'?s not\b|\bnot what i\b|\bstop\b|\bdon'?t\b|\bnever\b/i;

export interface ChorusRepromptAnalyticsDeps {
  db: Database.Database;
  now?: () => number;
}

export interface ChorusRepromptAnalyticsQuery {
  days?: string;
}

type EventType = 'reprompt' | 'approval' | 'correction';

interface RepromptEvent {
  text: string;
  role: string;
  timestamp: string;
  type: EventType;
}

function isNoiseContent(c: string): boolean {
  if (!c || c.length > 500) return true;
  if (c.startsWith('<') || c.startsWith('{') || c.startsWith('[')) return true;
  if (/^\/Users\/|^\/tmp\/|^\/opt\//.test(c)) return true;
  if (/^(exit|y|n|yes|no)$/i.test(c.trim())) return true;
  return false;
}

function loadFilteredMessages(db: ChorusRepromptAnalyticsDeps['db'], cutoff: string): Array<{ content: string; channel: string; timestamp: string }> {
  const rows = db.prepare(`
    SELECT content, channel, timestamp FROM messages
    WHERE author='user' AND source='claude'
      AND channel IN ('session:wren','session:silas','session:kade')
      AND timestamp >= ?
    ORDER BY timestamp ASC
  `).all(cutoff) as Array<{ content: string; channel: string; timestamp: string }>;

  return rows
    .map((r) => ({ ...r, content: r.content.replace(/<system-reminder>[\s\S]*?<\/system-reminder>/g, '').trim() }))
    .filter((r) => !isNoiseContent(r.content));
}

function classifyEventType(text: string): EventType | null {
  if (REPROMPT_KEYWORDS.test(text)) {
    if (/run.*(again|it)|try.*(again|it)|again\?$/.test(text)) return null;
    return 'reprompt';
  }
  if (APPROVAL_BIGRAMS.test(text)) return 'approval';
  if (CORRECTION_PATTERNS.test(text) && text.length < 100) return 'correction';
  return null;
}

export function fetchChorusRepromptAnalytics(
  deps: ChorusRepromptAnalyticsDeps,
  query: ChorusRepromptAnalyticsQuery,
): FetchResult {
  const now = deps.now ?? Date.now;
  const days = Math.min(Math.max(parseInt(query.days || '30', 10), 1), 365);
  const cutoff = new Date(now() - days * 24 * 60 * 60 * 1000).toISOString();

  const filtered = loadFilteredMessages(deps.db, cutoff);

  const events: RepromptEvent[] = [];
  const dailyCounts: Record<string, { reprompt: number; approval: number; correction: number; total: number }> = {};
  const roleCounts: Record<string, { reprompt: number; approval: number; correction: number }> = {
    wren: { reprompt: 0, approval: 0, correction: 0 },
    silas: { reprompt: 0, approval: 0, correction: 0 },
    kade: { reprompt: 0, approval: 0, correction: 0 },
  };

  for (const r of filtered) {
    const text = r.content.toLowerCase().trim();
    const role = r.channel.replace('session:', '');
    const day = r.timestamp.substring(0, 10);
    if (!dailyCounts[day]) dailyCounts[day] = { reprompt: 0, approval: 0, correction: 0, total: 0 };
    dailyCounts[day].total++;

    const type = classifyEventType(text);
    if (type) {
      events.push({ text: r.content.substring(0, 120), role, timestamp: r.timestamp, type });
      dailyCounts[day][type]++;
      if (roleCounts[role]) roleCounts[role][type]++;
    }
  }

  const sortedDays = Object.keys(dailyCounts).sort();
  const trend = sortedDays.map((day) => ({
    date: day,
    ...dailyCounts[day],
    attentionCost: dailyCounts[day].reprompt * 3 + dailyCounts[day].approval + dailyCounts[day].correction * 2,
  }));

  const totalMessages = filtered.length;
  const totalSignals = events.length;
  const repromptCount = events.filter((e) => e.type === 'reprompt').length;
  const approvalCount = events.filter((e) => e.type === 'approval').length;
  const correctionCount = events.filter((e) => e.type === 'correction').length;

  const phraseCount: Record<string, number> = {};
  for (const e of events) {
    const normalized = e.text.toLowerCase().trim().replace(/[^a-z0-9\s]/g, '').substring(0, 60);
    phraseCount[normalized] = (phraseCount[normalized] || 0) + 1;
  }
  const topPhrases = Object.entries(phraseCount)
    .sort((a, b) => b[1] - a[1])
    .slice(0, 15)
    .map(([phrase, count]) => ({ phrase, count }));

  return {
    status: 200,
    body: {
      headline: {
        totalMessages,
        totalSignals,
        signalRate: totalMessages > 0 ? Math.round((totalSignals / totalMessages) * 100) : 0,
        reprompt: repromptCount,
        approvalOverhead: approvalCount,
        correction: correctionCount,
      },
      byRole: roleCounts,
      trend,
      topPhrases,
      recentEvents: events.slice(-20).reverse(),
      meta: { days, cutoff, messagesAnalyzed: filtered.length },
    },
  };
}
