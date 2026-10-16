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
  return [
    rooms,
    ...(read.value.readings || []).map(toTrustChip),
    ...(read.value.unmeasured || []).map(unmeasuredChip),
  ];
}
