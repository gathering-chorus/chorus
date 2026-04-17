/**
 * Cost summary — #2099 per-page migration from Gathering.
 *
 * Aggregates four cost sources: Claude (stats cache), Twilio (API, creds-gated),
 * Clearing (transcript files), Tunnel (cloudflared metrics). Ported from
 * jeff-bridwell-personal-site/src/handlers/cost.handler.ts with the same
 * semantics; chorus-api traffic stats are not tracked (Gathering's traffic
 * tracker is in-process and stays there).
 *
 * Twilio requires TWILIO_ACCOUNT_SID + TWILIO_AUTH_TOKEN in env. Until they
 * land in com.chorus.api.plist (Silas brief 2026-04-16), the function returns
 * { pending: true, totalCost: 0, records: [] } and the UI shows a badge.
 */

import * as fs from 'fs';
import * as path from 'path';
import * as http from 'http';
import * as https from 'https';
import * as os from 'os';

const CLAUDE_MONTHLY_RATE = parseInt(process.env.CLAUDE_MONTHLY_RATE || '200', 10);
const TWILIO_CACHE_TTL_MS = 5 * 60 * 1000;

// #2167 test seams — default to the real ~/.claude paths, overridable in tests.
const CLAUDE_STATS_CACHE = process.env.CLAUDE_STATS_CACHE
  || path.join(os.homedir(), '.claude', 'stats-cache.json');
const CLEARING_TRANSCRIPTS_DIR = process.env.CLEARING_TRANSCRIPTS_DIR
  || path.join(os.homedir(), 'CascadeProjects', 'chorus', 'clearing', 'transcripts');

interface DailyActivity {
  date: string;
  messageCount: number;
  sessionCount: number;
  toolCallCount: number;
}

interface TwilioUsageRecord {
  category: string;
  description: string;
  count: string;
  price: string;
  count_unit: string;
  price_unit: string;
}

interface ClearingSession {
  started?: string;
  ended?: string;
  model?: string;
  estimatedCost?: number;
  messageCount?: number;
}

export interface ClaudeStats {
  monthlyRate: number;
  daysInMonth: number;
  dayOfMonth: number;
  monthProgress: number;
  activeDays: number;
  usageIntensity: number;
  burnStatus: 'HOT' | 'SMOOTH' | 'COLD';
  totalSessions: number;
  totalMessages: number;
  dailyActivity: DailyActivity[];
  hourCounts: Record<string, number>;
  lastComputed: string | null;
}

export interface TwilioCostResult {
  records: TwilioUsageRecord[];
  totalCost: number;
  pending: boolean;
}

export interface TunnelStatus {
  status: 'UP' | 'DOWN' | 'UNKNOWN';
}

export interface ClearingCostResult {
  sessions: { date: string; cost: number; messages: number }[];
  totalCost: number;
}

export interface CostSummaryResponse {
  claude: ClaudeStats;
  twilio: TwilioCostResult;
  clearing: ClearingCostResult;
  tunnel: TunnelStatus;
  summary: { fixedCost: number; variableCost: number; totalCost: number };
  generatedAt: string;
}

let twilioCache: { data: TwilioUsageRecord[]; fetchedAt: number } | null = null;

function getClaudeStats(): ClaudeStats {
  try {
    const raw = fs.readFileSync(CLAUDE_STATS_CACHE, 'utf-8');
    const stats = JSON.parse(raw);

    const now = new Date();
    const year = now.getFullYear();
    const month = now.getMonth();
    const daysInMonth = new Date(year, month + 1, 0).getDate();
    const dayOfMonth = now.getDate();
    const monthProgress = dayOfMonth / daysInMonth;

    const monthPrefix = `${year}-${String(month + 1).padStart(2, '0')}`;
    const dailyActivity: DailyActivity[] = (stats.dailyActivity || []).filter(
      (d: DailyActivity) => d.date.startsWith(monthPrefix)
    );

    const totalSessions = dailyActivity.reduce((s, d) => s + d.sessionCount, 0);
    const totalMessages = dailyActivity.reduce((s, d) => s + d.messageCount, 0);
    const activeDays = dailyActivity.filter((d) => d.sessionCount > 0).length;
    const usageIntensity = dayOfMonth > 0 ? activeDays / dayOfMonth : 0;

    let burnStatus: 'HOT' | 'SMOOTH' | 'COLD';
    if (usageIntensity > 0.8) burnStatus = 'HOT';
    else if (usageIntensity > 0.4) burnStatus = 'SMOOTH';
    else burnStatus = 'COLD';

    return {
      monthlyRate: CLAUDE_MONTHLY_RATE,
      daysInMonth,
      dayOfMonth,
      monthProgress: Math.round(monthProgress * 100),
      activeDays,
      usageIntensity: Math.round(usageIntensity * 100),
      burnStatus,
      totalSessions,
      totalMessages,
      dailyActivity,
      hourCounts: stats.hourCounts || {},
      lastComputed: stats.lastComputedDate || null,
    };
  } catch {
    return {
      monthlyRate: CLAUDE_MONTHLY_RATE,
      daysInMonth: 0, dayOfMonth: 0, monthProgress: 0, activeDays: 0,
      usageIntensity: 0, burnStatus: 'COLD', totalSessions: 0, totalMessages: 0,
      dailyActivity: [], hourCounts: {}, lastComputed: null,
    };
  }
}

function fetchTwilioUsage(sid: string, token: string, startDate: string, endDate: string): Promise<TwilioUsageRecord[]> {
  return new Promise((resolve, reject) => {
    const auth = Buffer.from(`${sid}:${token}`).toString('base64');
    const urlPath = `/2010-04-01/Accounts/${sid}/Usage/Records.json?StartDate=${startDate}&EndDate=${endDate}`;
    const req = https.request(
      { hostname: 'api.twilio.com', path: urlPath, method: 'GET', headers: { Authorization: `Basic ${auth}` } },
      (res) => {
        let data = '';
        res.on('data', (chunk) => (data += chunk));
        res.on('end', () => {
          try {
            const parsed = JSON.parse(data);
            const relevant = (parsed.usage_records || []).filter(
              (r: TwilioUsageRecord) => parseFloat(r.price || '0') !== 0 || parseInt(r.count || '0', 10) > 0
            );
            resolve(relevant);
          } catch {
            reject(new Error('Failed to parse Twilio response'));
          }
        });
      }
    );
    req.on('error', reject);
    req.setTimeout(10000, () => { req.destroy(); reject(new Error('Twilio API timeout')); });
    req.end();
  });
}

async function getTwilioCosts(): Promise<TwilioCostResult> {
  const sid = process.env.TWILIO_ACCOUNT_SID;
  const token = process.env.TWILIO_AUTH_TOKEN;
  if (!sid || !token) {
    return { records: [], totalCost: 0, pending: true };
  }
  if (twilioCache && Date.now() - twilioCache.fetchedAt < TWILIO_CACHE_TTL_MS) {
    const totalCost = twilioCache.data.reduce((s, r) => s + Math.abs(parseFloat(r.price || '0')), 0);
    return { records: twilioCache.data, totalCost, pending: false };
  }
  try {
    const now = new Date();
    const startDate = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, '0')}-01`;
    const endDate = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, '0')}-${String(now.getDate()).padStart(2, '0')}`;
    const records = await fetchTwilioUsage(sid, token, startDate, endDate);
    twilioCache = { data: records, fetchedAt: Date.now() };
    const totalCost = records.reduce((s, r) => s + Math.abs(parseFloat(r.price || '0')), 0);
    return { records, totalCost, pending: false };
  } catch {
    return { records: [], totalCost: 0, pending: false };
  }
}

function getTunnelStatus(): Promise<TunnelStatus> {
  return new Promise((resolve) => {
    const req = http.request(
      { hostname: '127.0.0.1', port: 20241, path: '/ready', method: 'GET', timeout: 3000 },
      (res) => {
        resolve({ status: res.statusCode === 200 ? 'UP' : 'DOWN' });
        res.resume();
      }
    );
    req.on('error', () => resolve({ status: 'UNKNOWN' }));
    req.on('timeout', () => { req.destroy(); resolve({ status: 'UNKNOWN' }); });
    req.end();
  });
}

function getClearingCosts(): ClearingCostResult {
  const transcriptDir = CLEARING_TRANSCRIPTS_DIR;
  try {
    if (!fs.existsSync(transcriptDir)) return { sessions: [], totalCost: 0 };
    const now = new Date();
    const monthPrefix = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, '0')}`;
    const files = fs.readdirSync(transcriptDir).filter((f) => f.endsWith('.json') && f.startsWith(monthPrefix));

    const sessions: { date: string; cost: number; messages: number }[] = [];
    let totalCost = 0;
    for (const file of files) {
      try {
        const raw = fs.readFileSync(path.join(transcriptDir, file), 'utf-8');
        const data = JSON.parse(raw);
        const session: ClearingSession | undefined = data.session;
        if (session) {
          const cost = session.estimatedCost || 0;
          sessions.push({ date: session.started || file.replace('.json', ''), cost, messages: session.messageCount || 0 });
          totalCost += cost;
        }
      } catch { /* skip malformed */ }
    }
    return { sessions, totalCost };
  } catch {
    return { sessions: [], totalCost: 0 };
  }
}

export async function getCostSummary(): Promise<CostSummaryResponse> {
  const [claude, twilio, tunnel] = await Promise.all([
    Promise.resolve(getClaudeStats()),
    getTwilioCosts(),
    getTunnelStatus(),
  ]);
  const clearing = getClearingCosts();

  const variableCost = twilio.totalCost + clearing.totalCost;
  return {
    claude,
    twilio,
    clearing,
    tunnel,
    summary: {
      fixedCost: CLAUDE_MONTHLY_RATE,
      variableCost,
      totalCost: CLAUDE_MONTHLY_RATE + variableCost,
    },
    generatedAt: new Date().toISOString(),
  };
}
