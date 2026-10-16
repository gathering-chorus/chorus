// @domain: messages
// Trust strip: one row per source, probed live. Each source has its own row builder.

import { TrustStripSource, BoardWipResponse } from './models';
import { fetchJson, probeUrl, failureReason } from './http';
import { formatHHMM } from './format';

export interface TrustStripOptions {
  rolesUrl?: string;
  boardUrl?: string;
  lokiUrl?: string;
  failSource?: string;
  lagSource?: string;
  lagMinutes?: number;
  fixture?: string;
}

export interface TrustStripContext {
  rolesUrl: string;
  boardUrl: string;
  lokiUrl: string;
  relayConfigured: boolean;
  relayConnected: () => boolean;
}

const CONNECTION_REFUSED = 'connection refused';
const DEFAULT_LAG_MINUTES = 34;
const PROBE_TIMEOUT_MS = 1500;
const KNOWN_SOURCES = ['board', 'rooms', 'decisions', 'roles', 'loki'];

function failedRow(source: string, now: Date, reason: string): TrustStripSource {
  return { source, lastWrite: now.toISOString(), asOf: 'failed', status: 'failed', reason };
}

function liveRow(source: string, now: Date): TrustStripSource {
  return { source, lastWrite: now.toISOString(), asOf: 'live', status: 'ok' };
}

function laggedRow(source: string, now: Date, lagMinutes: number | undefined): TrustStripSource {
  const lagMin = lagMinutes || DEFAULT_LAG_MINUTES;
  const lagDate = new Date(now.getTime() - lagMin * 60 * 1000);
  return {
    source,
    lastWrite: lagDate.toISOString(),
    asOf: `as of ${formatHHMM(lagDate)} (${lagMin}m)`,
    status: 'ok',
    lagMinutes: lagMin,
  };
}

async function boardRow(boardUrl: string, now: Date, o: TrustStripOptions): Promise<TrustStripSource> {
  if (o.failSource === 'board' || o.fixture === 'fail') return failedRow('board', now, CONNECTION_REFUSED);
  if (o.lagSource === 'board') return laggedRow('board', now, o.lagMinutes);
  const r = await fetchJson<BoardWipResponse | null>(`${boardUrl}/wip`, PROBE_TIMEOUT_MS);
  if (!r.ok) return failedRow('board', now, failureReason(r, CONNECTION_REFUSED));
  const ts = r.value?.timestamp || now.toISOString();
  return { source: 'board', lastWrite: ts, asOf: `as of ${formatHHMM(new Date(ts))}`, status: 'ok' };
}

function roomsRow(ctx: TrustStripContext, now: Date, o: TrustStripOptions): TrustStripSource {
  if (!ctx.relayConfigured) return failedRow('rooms', now, 'relay not configured');
  if (o.failSource === 'rooms' || !ctx.relayConnected()) return failedRow('rooms', now, 'relay disconnected');
  return liveRow('rooms', now);
}

// Decisions has no real source until Part 2 (Decision class unlanded).
// Honest red by name ("no source, part 2") per Wren 12:37 directive.
function decisionsRow(now: Date, o: TrustStripOptions): TrustStripSource {
  return failedRow('decisions', now, o.failSource === 'decisions' ? 'service unavailable' : 'no source, part 2');
}

async function rolesRow(rolesUrl: string, now: Date, o: TrustStripOptions): Promise<TrustStripSource> {
  if (o.failSource === 'roles' || o.fixture === 'fail') return failedRow('roles', now, 'door unreadable (500)');
  if (o.lagSource === 'roles') return laggedRow('roles', now, o.lagMinutes);
  const r = await probeUrl(rolesUrl, PROBE_TIMEOUT_MS);
  if (!r.ok) return failedRow('roles', now, `door unreadable (${r.status ?? (r.message || CONNECTION_REFUSED)})`);
  return liveRow('roles', now);
}

// Loki row (action presence). An unknown failSource takes this slot, red by its own name.
async function lokiRow(lokiUrl: string, now: Date, o: TrustStripOptions): Promise<TrustStripSource> {
  if (o.failSource === 'loki') return failedRow('loki', now, CONNECTION_REFUSED);
  if (o.failSource && !KNOWN_SOURCES.includes(o.failSource)) return failedRow(o.failSource, now, CONNECTION_REFUSED);
  const r = await probeUrl(`${lokiUrl}/ready`, PROBE_TIMEOUT_MS);
  if (!r.ok) return failedRow('loki', now, failureReason(r, CONNECTION_REFUSED));
  return liveRow('loki', now);
}

/** Probes trust strip sources in order: board, rooms, decisions, roles, loki. */
export async function buildTrustStripSources(ctx: TrustStripContext, options?: TrustStripOptions): Promise<TrustStripSource[]> {
  const o = options || {};
  const rolesUrl = o.rolesUrl || ctx.rolesUrl;
  const boardUrl = o.boardUrl || ctx.boardUrl;
  const lokiUrl = o.lokiUrl || ctx.lokiUrl;
  const now = new Date();

  const board = await boardRow(boardUrl, now, o);
  const rooms = roomsRow(ctx, now, o);
  const decisions = decisionsRow(now, o);
  const roles = await rolesRow(rolesUrl, now, o);
  const loki = await lokiRow(lokiUrl, now, o);
  return [board, rooms, decisions, roles, loki];
}
