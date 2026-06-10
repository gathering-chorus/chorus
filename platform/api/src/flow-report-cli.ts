/**
 * flow-report-cli — Loki → aggregateFlow → JSON stdout (#3269).
 *
 * The standing form of the 06-06 one-off report. Run OFF any serving loop
 * (the chorus_flow_report MCP tool execs this; a night-cycle can too):
 *
 *   node dist/flow-report-cli.js [--hours N] [--html /path/report.html]
 *
 * Sources (both Loki jobs, merged):
 *   werk-verbs       — the verb jsonl witnesses: {ts(ms), event, card_id, ...}
 *   platform-chorus  — the spine: {timestamp(ISO), event, card_id, ...},
 *                      server-side filtered to card-bound lines.
 *
 * No silent caps: if a Loki page hits its limit, the output carries
 * truncated=true so a consumer never mistakes a cap for completeness.
 */
import { aggregateFlow, FlowEvent, FlowReport } from './flow-report';

const LOKI = process.env.CHORUS_LOKI_URL || 'http://localhost:3102';
const PAGE_LIMIT = 5000;

/** Normalize one raw log line (either source) into a FlowEvent, or null. */
export function normalizeLine(line: string): FlowEvent | null {
  const t = line.trim();
  if (!t.startsWith('{')) return null;
  let d: Record<string, unknown>;
  try {
    d = JSON.parse(t) as Record<string, unknown>;
  } catch {
    return null;
  }
  const event = typeof d.event === 'string' ? d.event : '';
  if (!event) return null;
  const cardId = typeof d.card_id === 'number' ? d.card_id : undefined;
  let ts: number | null = null;
  if (typeof d.ts === 'number') ts = d.ts;
  else if (typeof d.timestamp === 'string') {
    const parsed = Date.parse(d.timestamp);
    ts = Number.isNaN(parsed) ? null : parsed;
  }
  if (ts === null) return null;
  const detail = ['reason', 'error', 'error_message', 'detail', 'name']
    .map((k) => (typeof d[k] === 'string' ? (d[k] as string) : ''))
    .filter(Boolean)
    .join(' ')
    .slice(0, 200);
  return { ts, event, card_id: cardId, role: typeof d.role === 'string' ? d.role : undefined, detail };
}

const MAX_PAGES = 8; // 40k lines per source — beyond this, flag truncated (no silent caps)

async function fetchJob(query: string, startNs: string, endNs: string): Promise<{ lines: string[]; truncated: boolean }> {
  const lines: string[] = [];
  let end = endNs;
  for (let page = 0; page < MAX_PAGES; page++) {
    const url = `${LOKI}/loki/api/v1/query_range?query=${encodeURIComponent(query)}&start=${startNs}&end=${end}&limit=${PAGE_LIMIT}&direction=backward`;
    const res = await fetch(url);
    if (!res.ok) throw new Error(`loki ${res.status} for ${query}`);
    const body = (await res.json()) as { data?: { result?: Array<{ values?: Array<[string, string]> }> } };
    let oldest = BigInt(end);
    let count = 0;
    for (const stream of body.data?.result ?? []) {
      for (const [tsNs, line] of stream.values ?? []) {
        lines.push(line);
        count++;
        const ts = BigInt(tsNs);
        if (ts < oldest) oldest = ts;
      }
    }
    if (count < PAGE_LIMIT) return { lines, truncated: false }; // window fully covered
    end = (oldest - 1n).toString(); // next page: everything older than this page
  }
  return { lines, truncated: true }; // hit the page cap — older events in window missing
}

/** Escape log-sourced text for HTML interpolation — detail/event come from
 *  arbitrary log JSON (gate-quality catch: never trust log content in markup). */
export function esc(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;').replace(/'/g, '&#39;');
}

/** Minimal HTML rendering — same columns as the 06-06 report it replaces. */
export function buildHtml(report: FlowReport & { generatedAt: string; windowHours: number; truncated: boolean }): string {
  const fmt = (s: number | null) =>
    s === null ? '—' : s >= 3600 ? `${Math.round(s / 360) / 10}h` : s >= 60 ? `${Math.round(s / 60)}m` : `${s}s`;
  const rows = report.cards
    .map((c) => {
      const errs = c.errors
        .map((e) => `<div class="err">${new Date(e.ts).toLocaleString('en-US', { timeZone: 'America/New_York' })} · ${esc(e.event)} · ${esc(e.detail)}</div>`)
        .join('');
      return `<tr><td>#${c.card}</td><td>${c.landed ? '✓' : '✗'}</td><td><b>${fmt(c.cycleS)}</b></td>
<td>${fmt(c.steps.workS)}</td><td>${fmt(c.steps.pushS)}</td><td>${fmt(c.steps.buildS)}</td><td>${fmt(c.steps.deployS)}</td><td>${fmt(c.steps.demoS)}</td><td>${fmt(c.steps.mergeS)}</td><td>${fmt(c.steps.finalS)}</td>
<td>${c.errors.length ? `<details><summary>${c.errors.length} ▸</summary>${errs}</details>` : '0'}</td></tr>`;
    })
    .join('\n');
  const classes = report.errorClasses.map((e) => `<li><b>${esc(e.event)}</b> × ${e.count}</li>`).join('');
  return `<!DOCTYPE html><html><head><meta charset="utf-8"><title>Card Cycle Report</title>
<style>body{font:13px/1.45 ui-monospace,Menlo,monospace;margin:1.5rem;background:#f7f4ee}table{border-collapse:collapse;width:100%;background:#fff}
th,td{text-align:left;padding:4px 8px;border-bottom:1px solid #eee}th{background:#222;color:#fff;font-size:11px;text-transform:uppercase}
.err{color:#a33;font-size:12px;padding:1px 0}</style></head><body>
<h1>Card Cycle Report — cycle · step time · errors enumerated per card</h1>
<p>Generated ${report.generatedAt} · ${report.totals.cards} cards (${report.windowHours}h) · ${report.totals.errorEvents} error/warning events · ${report.totals.landed} landed · CYCLE median ${fmt(report.cycleStats.medianS)} / avg ${fmt(report.cycleStats.avgS)} / p90 ${fmt(report.cycleStats.p90S)}${report.truncated ? ' · <b style="color:#b00">TRUNCATED at page limit</b>' : ''}</p>
<table><thead><tr><th>card</th><th>landed</th><th>CYCLE</th><th>work</th><th>push</th><th>build</th><th>deploy</th><th>demo</th><th>merge</th><th>final</th><th>errors / warnings</th></tr></thead>
<tbody>${rows}</tbody></table>
<h2>Error classes ranked</h2><ul>${classes}</ul>
<p style="color:#888">Standing instrument (#3269, chorus_flow_report) — regenerate via the MCP tool; replaces the 06-06 one-off.</p>
</body></html>`;
}

async function main(): Promise<void> {
  const args = process.argv.slice(2);
  const hoursIdx = args.indexOf('--hours');
  const hours = hoursIdx >= 0 ? Number(args[hoursIdx + 1]) || 120 : 120;
  const htmlIdx = args.indexOf('--html');
  const htmlPath = htmlIdx >= 0 ? args[htmlIdx + 1] : null;

  const end = Date.now();
  const start = end - hours * 3600 * 1000;
  const startNs = `${start}000000`;
  const endNs = `${end}000000`;

  const [verbs, spine] = await Promise.all([
    fetchJob('{job="werk-verbs"}', startNs, endNs),
    fetchJob('{job="platform-chorus"} |= "card_id"', startNs, endNs),
  ]);

  const events: FlowEvent[] = [];
  for (const line of [...verbs.lines, ...spine.lines]) {
    const e = normalizeLine(line);
    if (e) events.push(e);
  }

  const report = {
    generatedAt: new Date().toISOString(),
    windowHours: hours,
    truncated: verbs.truncated || spine.truncated,
    ...aggregateFlow(events),
  };

  if (htmlPath) {
    const fs = await import('fs');
    fs.writeFileSync(htmlPath, buildHtml(report));
  }
  process.stdout.write(JSON.stringify(report));
}

// Only run as a CLI, not on import (tests import the pure fns).
if (require.main === module) {
  main().catch((e) => {
    process.stderr.write(`flow-report-cli: ${e?.message ?? e}\n`);
    process.exit(1);
  });
}
