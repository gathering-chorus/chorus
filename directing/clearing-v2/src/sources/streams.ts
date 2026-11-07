// @domain: streams
// #4597 — Jeff 16:25: "i really need a streams menu option too". Each role's recent
// commands, as the spine stream the old Clearing serves them (/api/stream). v2 groups
// them by role and shows them; it derives nothing. An unreachable stream says so by name.
export interface StreamLine { ts: string; role: string; text: string; type?: string }
export type Streams =
  | { ok: true; roles: Array<{ role: string; lines: StreamLine[] }> }
  | { ok: false; reason: string };

/** Lines per role on the Streams tab. */
export const STREAM_LINES_PER_ROLE = 15;

export function groupByRole(lines: StreamLine[], perRole = STREAM_LINES_PER_ROLE): Array<{ role: string; lines: StreamLine[] }> {
  const byRole = new Map<string, StreamLine[]>();
  for (const l of lines) {
    if (!l || !l.role) continue;
    if (!byRole.has(l.role)) byRole.set(l.role, []);
    byRole.get(l.role)!.push(l);
  }
  // newest first within each role; roles in name order so the columns never jump
  return Array.from(byRole.keys()).sort().map((role) => ({ role, lines: byRole.get(role)!.slice(-perRole).reverse() }));
}

export async function fetchStreams(streamUrl: string): Promise<Streams> {
  if (!streamUrl) return { ok: false, reason: 'stream not configured' };
  try {
    const res = await fetch(`${streamUrl}?lines=200`, { signal: AbortSignal.timeout(5000) });
    if (!res.ok) return { ok: false, reason: `stream answered ${res.status}` };
    const rows = (await res.json()) as unknown;
    if (!Array.isArray(rows)) return { ok: false, reason: 'stream returned no list' };
    return { ok: true, roles: groupByRole(rows as StreamLine[]) };
  } catch (e) {
    return { ok: false, reason: `stream unreachable: ${(e instanceof Error ? e.message : String(e)).split('\n')[0]}` };
  }
}
