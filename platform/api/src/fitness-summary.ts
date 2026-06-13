/* eslint-disable security/detect-non-literal-fs-filename, security/detect-object-injection --
 * Server-controlled paths from env constants; indexing on validated typed keys.
 */
/**
 * Fitness Functions — #2099 per-page migration from Gathering.
 *
 * Reads chorus.log, builds session windows per role, then computes 4
 * per-role fitness metrics (JDI rate, decision-gate rate, search-hierarchy
 * rate, retry rate). Semantics match
 * jeff-bridwell-personal-site/src/handlers/fitness-functions.handler.ts —
 * same events, same retry clustering (30s window), same per-role trend
 * calculation across 7d vs previous 7d.
 */

import { readFileTail } from './lib/log-reader';

const CHORUS_LOG = process.env.CHORUS_LOG_PATH || `${process.env.HOME}/.chorus/chorus.log`;
// #3406 sibling-audit — chorus.log grows unbounded (535MB+). Reading it whole on
// the request path is the freeze/OOM-crash root (same ReadFileUtf8 stack the spine
// endpoint hit). Bound this summary's read to a generous recent tail.
const CHORUS_LOG_TAIL_BYTES = Number(process.env.CHORUS_LOG_TAIL_BYTES) || 16 * 1024 * 1024;

const ROLES = ['silas', 'wren', 'kade'] as const;
type Role = typeof ROLES[number];

interface ChorusEvent {
  timestamp: string;
  event: string;
  role: string;
  [key: string]: unknown;
}

interface SessionWindow {
  role: Role;
  start: string;
  events: ChorusEvent[];
}

interface RoleMetric {
  sessions: number;
  events: number;
  rate: number;
  trend: number;
}

export interface FitnessFunction {
  id: string;
  label: string;
  description: string;
  direction: 'lower-is-better' | 'higher-is-better';
  byRole: Record<string, RoleMetric>;
  trend7d: number;
  overall7d: number;
  overallToday: number;
  recentEvents: ChorusEvent[];
}

export interface FitnessSummaryResponse {
  functions: FitnessFunction[];
}

function loadChorusEvents(): ChorusEvent[] {
  try {
    const raw = readFileTail(CHORUS_LOG, CHORUS_LOG_TAIL_BYTES);
    if (raw === null) return [];
    // tail may start mid-line; the per-line JSON.parse below discards the partial.
    const lines = raw.trim().split('\n').filter(Boolean);
    const events: ChorusEvent[] = [];
    for (const line of lines) {
      try {
        const obj = JSON.parse(line);
        if (obj.timestamp && obj.event) events.push(obj as ChorusEvent);
      } catch { /* skip */ }
    }
    events.sort((a, b) => a.timestamp.localeCompare(b.timestamp));
    return events;
  } catch {
    return [];
  }
}

// Open-ended session windows extend until the next same-role start, or
// forever if none exists. Using `new Date()` as a cap was incorrect: it
// dropped events with timestamps past wall-clock now (clock skew, fixtures
// generated relative to a fixed anchor). The session is "current" — it
// should claim all subsequent same-role events regardless of clock.
const OPEN_END = '9999-12-31T23:59:59.999Z';

// Direction literal — extracted to const so the value-position uses (#2603
// sonarjs no-duplicate-string threshold 5) don't trip the gate.
const LOWER_IS_BETTER = 'lower-is-better' as const;

function buildSessionWindows(events: ChorusEvent[]): SessionWindow[] {
  const starts = events.filter((e) => e.event === 'session.role.started' && (ROLES as readonly string[]).includes(e.role));
  return starts.map((s, i) => {
    const role = s.role as Role;
    const startTime = s.timestamp;
    const nextStart = starts.slice(i + 1).find((n) => n.role === role);
    const endTime = nextStart?.timestamp || OPEN_END;
    const sessionEvents = events.filter((e) => e.role === role && e.timestamp >= startTime && e.timestamp < endTime);
    return { role, start: startTime, events: sessionEvents };
  });
}

function resolveUnknownRoles(events: ChorusEvent[], sessions: SessionWindow[]): void {
  for (const event of events) {
    if (event.role !== 'unknown') continue;
    const ts = event.timestamp;
    const match = sessions.find((s) => {
      const nextSameRole = sessions.find((n) => n.role === s.role && n.start > s.start);
      const endTime = nextSameRole?.start || OPEN_END;
      return ts >= s.start && ts < endTime;
    });
    if (match) event.role = match.role;
  }
}

const RETRY_RULES: ReadonlyArray<readonly [(s: string) => boolean, string]> = [
  [(s) => s.includes('board-ts') || s.includes('cards'), 'board'],
  [(s) => s.includes('git-queue'), 'git-queue'],
  [(s) => s.includes('curl') && (s.includes('3030') || s.includes('fuseki')), 'fuseki'],
  [(s) => s.includes('curl') && (s.includes('3100') || s.includes('3102')), 'loki-grafana'],
  [(s) => s.includes('curl') && s.includes('localhost'), 'endpoint'],
  [(s) => s.includes('app-state') || s.includes('launchctl'), 'deploy'],
  [(s) => s.includes('chorus-log'), 'chorus-log'],
  [(s) => s.includes('role-state'), 'role-state'],
];

function categorizeRetry(summary: string): string {
  const s = summary.toLowerCase();
  for (const [match, category] of RETRY_RULES) if (match(s)) return category;
  return 'other';
}

function isRetryPair(e1: ChorusEvent, e2: ChorusEvent): boolean {
  if (e1.role !== e2.role || e1.action !== e2.action) return false;
  const s1 = typeof e1.summary === 'string' ? e1.summary : '';
  const s2 = typeof e2.summary === 'string' ? e2.summary : '';
  if (s1 === s2) return false;
  try {
    const gap = new Date(e2.timestamp).getTime() - new Date(e1.timestamp).getTime();
    return gap <= 30000;
  } catch {
    return false;
  }
}

function detectRetryClusters(allEvents: ChorusEvent[]): ChorusEvent[] {
  const toolEvents = allEvents.filter((e) => e.event === 'session_tool');
  const clusters: ChorusEvent[] = [];
  for (let i = 0; i < toolEvents.length - 1; i++) {
    const e1 = toolEvents[i];
    const e2 = toolEvents[i + 1];
    if (!isRetryPair(e1, e2)) continue;
    const s1 = typeof e1.summary === 'string' ? e1.summary : '';
    const category = categorizeRetry(s1);
    clusters.push({
      timestamp: e1.timestamp,
      event: 'tool.retry_cluster',
      role: e1.role,
      action: String(e1.action),
      category,
      summary: `${category}: ${s1.slice(0, 80)}`,
    });
  }
  return clusters;
}

interface BuildOpts {
  id: string;
  label: string;
  description: string;
  direction: 'lower-is-better' | 'higher-is-better';
  matchEvents: string[];
  filterFn?: (e: ChorusEvent) => boolean;
  allEvents: ChorusEvent[];
  sessions: SessionWindow[];
  weekAgo: Date;
  twoWeeksAgo: Date;
  todayStr: string;
}

function buildFunction(opts: BuildOpts): FitnessFunction {
  const { id, label, description, direction, matchEvents, filterFn, allEvents, sessions, weekAgo, twoWeeksAgo, todayStr } = opts;
  const isMatch = (e: ChorusEvent) => matchEvents.includes(e.event) && (!filterFn || filterFn(e));

  const matched = allEvents.filter(isMatch);
  const matched7d = matched.filter((e) => new Date(e.timestamp) >= weekAgo);
  const matchedPrev7d = matched.filter((e) => {
    const d = new Date(e.timestamp);
    return d >= twoWeeksAgo && d < weekAgo;
  });
  const matchedToday = matched.filter((e) => e.timestamp.startsWith(todayStr));

  const byRole: Record<string, RoleMetric> = {};
  for (const role of ROLES) {
    const roleSessions7d = sessions.filter((s) => s.role === role && new Date(s.start) >= weekAgo);
    const roleSessionsPrev = sessions.filter((s) => s.role === role && new Date(s.start) >= twoWeeksAgo && new Date(s.start) < weekAgo);
    const roleEvents7d = matched7d.filter((e) => e.role === role);
    const roleEventsPrev = matchedPrev7d.filter((e) => e.role === role);

    const rate7d = roleSessions7d.length > 0 ? roleEvents7d.length / roleSessions7d.length : 0;
    const ratePrev = roleSessionsPrev.length > 0 ? roleEventsPrev.length / roleSessionsPrev.length : 0;

    const trend = direction === LOWER_IS_BETTER ? ratePrev - rate7d : rate7d - ratePrev;

    byRole[role] = {
      sessions: roleSessions7d.length,
      events: roleEvents7d.length,
      rate: Math.round(rate7d * 100) / 100,
      trend: Math.round(trend * 100) / 100,
    };
  }

  const totalSessions7d = sessions.filter((s) => new Date(s.start) >= weekAgo).length;
  const totalSessionsPrev = sessions.filter((s) => new Date(s.start) >= twoWeeksAgo && new Date(s.start) < weekAgo).length;
  const overallRate7d = totalSessions7d > 0 ? matched7d.length / totalSessions7d : 0;
  const overallRatePrev = totalSessionsPrev > 0 ? matchedPrev7d.length / totalSessionsPrev : 0;
  const trend7d = direction === LOWER_IS_BETTER ? overallRatePrev - overallRate7d : overallRate7d - overallRatePrev;

  return {
    id,
    label,
    description,
    direction,
    byRole,
    trend7d: Math.round(trend7d * 100) / 100,
    overall7d: matched7d.length,
    overallToday: matchedToday.length,
    recentEvents: matched.slice(-8).reverse(),
  };
}

export function getFitnessSummary(): FitnessSummaryResponse {
  const allEvents = loadChorusEvents();
  const now = new Date();
  const weekAgo = new Date(now.getTime() - 7 * 24 * 60 * 60 * 1000);
  const twoWeeksAgo = new Date(now.getTime() - 14 * 24 * 60 * 60 * 1000);
  const todayStr = now.toISOString().slice(0, 10);

  const sessions = buildSessionWindows(allEvents);
  resolveUnknownRoles(allEvents, sessions);
  const retryEvents = detectRetryClusters(allEvents);

  const functions: FitnessFunction[] = [
    buildFunction({
      id: 'jdi-rate',
      label: 'JDI Rate',
      description: 'Permission-seeking per session — text_leak (blocked by hook) + jdi_override (Jeff typed "jdi"). Every event = a role asked when it should have executed. Trend toward zero = autonomy improving.',
      direction: LOWER_IS_BETTER,
      matchEvents: ['decision.gate.text_leak', 'decision.gate.jdi_override'],
      allEvents, sessions, weekAgo, twoWeeksAgo, todayStr,
    }),
    buildFunction({
      id: 'decision-gate-rate',
      label: 'Decision Gate Rate',
      description: 'Questions matching known Jeff preferences — role asked something with a predictable answer. Lower = roles internalized preferences.',
      direction: LOWER_IS_BETTER,
      matchEvents: ['decision.gate.matched'],
      allEvents, sessions, weekAgo, twoWeeksAgo, todayStr,
    }),
    buildFunction({
      id: 'search-hierarchy-rate',
      label: 'Search Hierarchy Rate',
      description: 'Exploratory filesystem searches that should have started with Chorus or codebase graph (DEC-074). Excludes legitimate code lookups (imports, specific file paths).',
      direction: LOWER_IS_BETTER,
      matchEvents: ['search.hierarchy.filesystem_used'],
      filterFn: (e) => e.code_lookup !== 'true',
      allEvents, sessions, weekAgo, twoWeeksAgo, todayStr,
    }),
    buildFunction({
      id: 'retry-rate',
      label: 'Retry Rate',
      description: 'Trial-and-error clusters — same tool called 2+ times in <30s with different args. Each cluster = guessing at syntax, paths, or endpoints.',
      direction: LOWER_IS_BETTER,
      matchEvents: ['tool.retry_cluster'],
      allEvents: retryEvents, sessions, weekAgo, twoWeeksAgo, todayStr,
    }),
  ];

  return { functions };
}
