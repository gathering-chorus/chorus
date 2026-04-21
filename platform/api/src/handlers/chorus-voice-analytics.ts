/**
 * GET /api/chorus/voice-analytics — Jeff tone + vocab analytics (#2188).
 *
 * Dependencies injected:
 *   db    — better-sqlite3 Database
 *   isEDT — (dateStr) => boolean (for Boston hour-of-day conversion)
 *   now   — () => number (default Date.now)
 *
 * Query:
 *   days — clamp [1, 365], default 30
 *
 * Pipeline:
 *   1. Load user-author claude messages in wren/silas/kade sessions within cutoff
 *   2. Filter noise (system reminders, JSON/XML, paths, single-word, skill injections)
 *   3. Tone classify each into 8 buckets
 *   4. Aggregate: headline %, toneByRole %, weekly tone trend, role attention weekly,
 *      msg length trend, hour-of-day, bigrams, corrective words, distinctive vocab
 */
import type Database from 'better-sqlite3';
import type { FetchResult } from './codebase-topology';

const CORRECTIVE_PATTERNS = /\bdon'?t\b|\bstop\s|\bno\s|\bnever\b|\bshouldn'?t\b|\bwrong\b|\bthat'?s not\b/i;
const COLLABORATIVE_PATTERNS = /\blet'?s\b|\bwe could\b|\bwhat if\b|\bwhat do you think\b|\bagree\b/i;
const ACKNOWLEDGMENT_PATTERNS = /^(ok|okay|cool|yep|yup|yeah|sure|right|sounds good|makes sense|got it|ack|perfect|great|nice|good|fair|fine|agreed|copy|roger|indeed|exactly|absolutely|certainly|totally|100%)\b/i;
const ROUTING_PATTERNS = /\btake a look\b|\bcheck out\b|\bgo to\b|\bchat w|\btalk to\b|\bloop .* in\b|^\/\w+\s|@\w+|\btail\s|\.sh\b|\.html\b|\.md\b|localhost/i;
const STATUS_PATTERNS = /\bi (just|already|was|am|have been|went|did|tried|restarted|stopped|started|rebooted|logged|walked|finished|completed|shipped)\b/i;

const IMPERATIVE_VERBS = new Set([
  'add','build','check','clean','close','commit','configure','create','debug','delete',
  'deploy','do','edit','enable','ensure','execute','export','extract','fetch','fix',
  'generate','get','grep','implement','import','install','kill','list','load','look',
  'make','merge','move','open','pipe','pull','push','read','refactor','remove',
  'rename','replace','restart','restore','review','revert','run','save','scan','search',
  'send','set','setup','ship','show','skip','sort','start','stop','strip','switch',
  'tag','test','try','update','upgrade','use','verify','view','wire','write',
]);

const STOP_WORDS = new Set([
  'the','a','an','and','or','but','in','on','at','to','for','of','is','it','i','you',
  'that','this','with','be','are','was','were','been','have','has','had','do','does',
  'did','will','would','could','should','can','may','might','shall','not','no','so',
  'if','then','than','just','also','very','too','up','out','about','into','over',
  'from','by','as','my','me','we','our','your','its','they','them','their','he','she',
  'his','her','him','what','which','who','how','when','where','why','all','each',
  'some','any','more','most','other','there','here','now','get','got','go','going',
  'like','thats','dont','im','ive','youre','hes','shes','were','theyre','wont',
  'cant','didnt','doesnt','isnt','arent','wasnt','havent','hasnt','lets','need',
  'know','think','want','see','look','make','good','new','one','two','well','way',
  'back','still','thing','things','right','take','come','been','being','much','said',
  'users','jeffbridwell','cascadeprojects','messages','scripts','workflow','bash',
  'tmp','opt','homebrew','usr','bin','local','node','npm','src','dist','var',
  'http','https','localhost','com','json','html','css','tsx','ts','js','md',
  'git','api','app','log','err','true','false','null','undefined',
]);

export const TONE_KEYS = ['directive', 'collaborative', 'question', 'corrective', 'acknowledgment', 'routing', 'status', 'narrative'] as const;
type Tone = typeof TONE_KEYS[number];

export interface ChorusVoiceAnalyticsDeps {
  db: Database.Database;
  isEDT: (dateStr: string) => boolean;
  now?: () => number;
}

export interface ChorusVoiceAnalyticsQuery {
  days?: string;
}

export function classifyTone(text: string): Tone {
  if (CORRECTIVE_PATTERNS.test(text)) return 'corrective';
  if (text.includes('?')) return 'question';
  const firstWord = text.trim().split(/\s+/)[0]?.toLowerCase().replace(/[^a-z]/g, '');
  if (firstWord && IMPERATIVE_VERBS.has(firstWord)) return 'directive';
  if (COLLABORATIVE_PATTERNS.test(text)) return 'collaborative';
  const trimmed = text.trim();
  if (ACKNOWLEDGMENT_PATTERNS.test(trimmed)) return 'acknowledgment';
  if (ROUTING_PATTERNS.test(trimmed)) return 'routing';
  if (STATUS_PATTERNS.test(trimmed)) return 'status';
  return 'narrative';
}

function isValidWord(w: string): boolean {
  return w.length > 2 && !STOP_WORDS.has(w) && !/^\d+$/.test(w);
}

function emptyTones(): Record<Tone, number> {
  return Object.fromEntries(TONE_KEYS.map((k) => [k, 0])) as Record<Tone, number>;
}

type Role = 'wren' | 'silas' | 'kade';
type Row = { content: string; channel: string; timestamp: string };

interface Aggregates {
  toneCount: Record<Tone, number>;
  toneByRole: Record<string, Record<Tone, number>>;
  weeklyTone: Record<string, Record<Tone, number>>;
  weeklyRole: Record<string, { wren: number; silas: number; kade: number }>;
  weeklyLength: Record<string, { total: number; count: number }>;
  hourByRole: Record<string, number[]>;
  bigramCount: Record<string, number>;
  correctiveWords: Record<string, number>;
  roleWordFreq: Record<string, Record<string, number>>;
}

function isNoise(c: string): boolean {
  if (!c || c.length > 500) return true;
  if (c.startsWith('<') || c.startsWith('{') || c.startsWith('[')) return true;
  if (/^\/Users\/|^\/tmp\/|^\/opt\//.test(c)) return true;
  if (/^(exit|y|n|yes|no)$/i.test(c.trim())) return true;
  if (c.includes('Base directory for this skill:')) return true;
  if (c.startsWith('# /') && c.includes('## ')) return true;
  if (/^<command-/.test(c)) return true;
  if (/^<task-notification>/.test(c)) return true;
  if (c.startsWith('This session is being continued from')) return true;
  if (/^ARGUMENTS:/.test(c)) return true;
  return false;
}

function loadMessages(db: Database.Database, cutoff: string): Row[] {
  const rows = db.prepare(`
    SELECT content, channel, timestamp FROM messages
    WHERE author='user' AND source='claude'
      AND channel IN ('session:wren','session:silas','session:kade')
      AND timestamp >= ?
    ORDER BY timestamp ASC
  `).all(cutoff) as Row[];
  return rows
    .map((r) => ({ ...r, content: r.content.replace(/<system-reminder>[\s\S]*?<\/system-reminder>/g, '').trim() }))
    .filter((r) => !isNoise(r.content));
}

function weekKeyFor(d: Date): string {
  const dayOfYear = Math.floor((d.getTime() - new Date(d.getFullYear(), 0, 1).getTime()) / 86400000);
  const weekNum = Math.ceil((dayOfYear + new Date(d.getFullYear(), 0, 1).getDay() + 1) / 7);
  return `${d.getFullYear()}-W${String(weekNum).padStart(2, '0')}`;
}

function newAggregates(): Aggregates {
  return {
    toneCount: emptyTones(),
    toneByRole: { wren: emptyTones(), silas: emptyTones(), kade: emptyTones() },
    weeklyTone: {},
    weeklyRole: {},
    weeklyLength: {},
    hourByRole: {
      wren: new Array(24).fill(0), silas: new Array(24).fill(0), kade: new Array(24).fill(0),
    },
    bigramCount: {},
    correctiveWords: {},
    roleWordFreq: { wren: {}, silas: {}, kade: {} },
  };
}

function tokenize(text: string): string[] {
  return text.toLowerCase().replace(/[^a-z0-9\s'-]/g, ' ').split(/\s+/).filter((w) => w.length > 1);
}

function recordBigrams(words: string[], into: Record<string, number>): void {
  for (let i = 0; i < words.length - 1; i++) {
    if (isValidWord(words[i]) && isValidWord(words[i + 1])) {
      const bg = `${words[i]} ${words[i + 1]}`;
      into[bg] = (into[bg] || 0) + 1;
    }
  }
}

function incrementWordMap(words: string[], into: Record<string, number>): void {
  for (const w of words) {
    if (isValidWord(w)) into[w] = (into[w] || 0) + 1;
  }
}

function updateWeekly(agg: Aggregates, weekKey: string, role: Role, tone: Tone, wordCount: number): void {
  if (!agg.weeklyTone[weekKey]) agg.weeklyTone[weekKey] = emptyTones();
  agg.weeklyTone[weekKey][tone]++;

  if (!agg.weeklyRole[weekKey]) agg.weeklyRole[weekKey] = { wren: 0, silas: 0, kade: 0 };
  agg.weeklyRole[weekKey][role]++;

  if (!agg.weeklyLength[weekKey]) agg.weeklyLength[weekKey] = { total: 0, count: 0 };
  agg.weeklyLength[weekKey].total += wordCount;
  agg.weeklyLength[weekKey].count++;
}

function bostonHour(d: Date, isEDT: (dateStr: string) => boolean): number {
  const offset = isEDT(d.toISOString().slice(0, 10)) ? 4 : 5;
  return (d.getUTCHours() - offset + 24) % 24;
}

function processRow(agg: Aggregates, row: Row, isEDT: (dateStr: string) => boolean, seen: Set<string>): void {
  const role = row.channel.replace('session:', '') as Role;
  const tone = classifyTone(row.content);
  const words = tokenize(row.content);

  agg.toneCount[tone]++;
  if (agg.toneByRole[role]) agg.toneByRole[role][tone]++;

  const d = new Date(row.timestamp);
  updateWeekly(agg, weekKeyFor(d), role, tone, words.length);
  agg.hourByRole[role][bostonHour(d, isEDT)]++;

  if (!seen.has(row.content)) {
    seen.add(row.content);
    recordBigrams(words, agg.bigramCount);
  }
  if (tone === 'corrective') incrementWordMap(words, agg.correctiveWords);
  incrementWordMap(words, agg.roleWordFreq[role]);
}

function aggregate(rows: Row[], isEDT: (dateStr: string) => boolean): Aggregates {
  const agg = newAggregates();
  const seen = new Set<string>();
  for (const row of rows) processRow(agg, row, isEDT, seen);
  return agg;
}

function headlinePct(toneCount: Record<Tone, number>, total: number): Record<string, number> {
  const out: Record<string, number> = {};
  for (const t of TONE_KEYS) {
    out[t] = total > 0 ? Math.round((toneCount[t] / total) * 100) : 0;
  }
  return out;
}

function toneByRolePercentages(toneByRole: Record<string, Record<Tone, number>>): Record<string, Record<string, number>> {
  const out: Record<string, Record<string, number>> = {};
  for (const [r, counts] of Object.entries(toneByRole)) {
    const roleTotal = Object.values(counts).reduce((a, b) => a + b, 0);
    out[r] = {};
    for (const [t, c] of Object.entries(counts)) {
      out[r][t] = roleTotal > 0 ? Math.round((c / roleTotal) * 100) : 0;
    }
  }
  return out;
}

function buildToneTrend(weeklyTone: Record<string, Record<Tone, number>>, weeks: string[]): Record<string, unknown> {
  const out: Record<string, unknown> = { weeks };
  for (const t of TONE_KEYS) out[t] = weeks.map((w) => weeklyTone[w]?.[t] || 0);
  return out;
}

function buildRoleAttention(weeklyRole: Aggregates['weeklyRole'], weeks: string[]) {
  return {
    weeks,
    wren: weeks.map((w) => weeklyRole[w]?.wren || 0),
    silas: weeks.map((w) => weeklyRole[w]?.silas || 0),
    kade: weeks.map((w) => weeklyRole[w]?.kade || 0),
  };
}

function buildLengthTrend(weeklyLength: Aggregates['weeklyLength'], weeks: string[]) {
  return {
    weeks,
    avgWords: weeks.map((w) => {
      const wl = weeklyLength[w];
      return wl && wl.count > 0 ? Math.round(wl.total / wl.count) : 0;
    }),
  };
}

function topBy<T>(entries: Array<[string, number]>, limit: number, map: (k: string, v: number) => T): T[] {
  return entries.sort((a, b) => b[1] - a[1]).slice(0, limit).map(([k, v]) => map(k, v));
}

function buildDistinctiveVocab(roleWordFreq: Record<string, Record<string, number>>): Record<string, Array<{ word: string; count: number }>> {
  const totalWordFreq: Record<string, number> = {};
  for (const freq of Object.values(roleWordFreq)) {
    for (const [w, c] of Object.entries(freq)) totalWordFreq[w] = (totalWordFreq[w] || 0) + c;
  }
  const globalTotal = Object.values(totalWordFreq).reduce((a, b) => a + b, 0);

  const out: Record<string, Array<{ word: string; count: number }>> = {};
  for (const [role, freq] of Object.entries(roleWordFreq)) {
    const roleTotal = Object.values(freq).reduce((a, b) => a + b, 0);
    out[role] = Object.entries(freq)
      .filter(([w, c]) => c >= 5 && totalWordFreq[w] >= 5)
      .map(([w, c]) => ({ word: w, count: c, ratio: (c / roleTotal) / (totalWordFreq[w] / globalTotal) }))
      .sort((a, b) => b.ratio - a.ratio)
      .slice(0, 10)
      .map(({ word, count }) => ({ word, count }));
  }
  return out;
}

export function fetchChorusVoiceAnalytics(
  deps: ChorusVoiceAnalyticsDeps,
  query: ChorusVoiceAnalyticsQuery,
): FetchResult {
  const now = deps.now ?? Date.now;
  const days = Math.min(Math.max(parseInt(query.days || '30', 10), 1), 365);
  const cutoff = new Date(now() - days * 24 * 60 * 60 * 1000).toISOString();

  const filtered = loadMessages(deps.db, cutoff);
  const agg = aggregate(filtered, deps.isEDT);
  const total = filtered.length;
  const weeks = Object.keys(agg.weeklyTone).sort();

  const dateRange = filtered.length > 0
    ? { from: filtered[0].timestamp.split('T')[0], to: filtered[filtered.length - 1].timestamp.split('T')[0] }
    : { from: null, to: null };

  return {
    status: 200,
    body: {
      meta: { messages: total, days, dateRange },
      headline: headlinePct(agg.toneCount, total),
      toneByRole: toneByRolePercentages(agg.toneByRole),
      toneTrend: buildToneTrend(agg.weeklyTone, weeks),
      roleAttention: buildRoleAttention(agg.weeklyRole, weeks),
      messageLengthTrend: buildLengthTrend(agg.weeklyLength, weeks),
      hourOfDay: agg.hourByRole,
      bigrams: topBy(Object.entries(agg.bigramCount), 25, (phrase, count) => ({ phrase, count })),
      correctiveWords: topBy(Object.entries(agg.correctiveWords), 15, (word, count) => ({ word, count })),
      distinctiveVocab: buildDistinctiveVocab(agg.roleWordFreq),
    },
  };
}
