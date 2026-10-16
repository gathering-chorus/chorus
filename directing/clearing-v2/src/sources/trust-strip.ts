// @domain: messages
// Trust strip: one chip per reading pulse's /api/readings returns (#4563), plus v2's own relay
// connection (rooms), which only v2 can see. v2 probes no other source and computes no as-of or lag.

import { TrustStripSource } from './models';
import { fetchReadings, toTrustChip, unmeasuredChip } from './pulse';

export interface TrustStripOptions {
  pulseUrl?: string;
}

export interface TrustStripContext {
  pulseUrl: string;
  relayConfigured: boolean;
  relayConnected: () => boolean;
}

function roomsChip(ctx: TrustStripContext): TrustStripSource {
  if (!ctx.relayConfigured) return { source: 'rooms', status: 'failed', reason: 'relay not configured' };
  if (!ctx.relayConnected()) return { source: 'rooms', status: 'failed', reason: 'relay disconnected' };
  return { source: 'rooms', status: 'ok', asOf: 'live' };
}

/** rooms (v2's relay), then each reading and each unmeasured entry exactly as pulse sent them. */
export async function buildTrustStripSources(ctx: TrustStripContext, options?: TrustStripOptions): Promise<TrustStripSource[]> {
  const rooms = roomsChip(ctx);
  const read = await fetchReadings(options?.pulseUrl || ctx.pulseUrl);
  if (!read.ok) return [rooms, { source: 'readings', status: 'unmeasured', reason: read.reason }];
  // one chip per source (display de-duplication, no fact computed): rooms is v2's own relay,
  // then pulse's readings, then unmeasured entries for sources pulse sent no reading for
  const readings = (read.value.readings || []).filter((r) => r.source !== 'rooms');
  const seen = new Set<string>(['rooms', ...readings.map((r) => r.source)]);
  const unread = (read.value.unmeasured || []).filter((u) => !seen.has(u.source));
  return [rooms, ...readings.map(toTrustChip), ...unread.map(unmeasuredChip)];
}
