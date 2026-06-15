/* eslint-disable security/detect-object-injection -- indexes by a fixed known key list, never untrusted input (#3429) */
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
import { aggregateFlow, deriveWalkAway, FlowEvent, FlowReport, WalkAway } from './flow-report';

const LOKI = process.env.CHORUS_LOKI_URL || 'http://localhost:3102';
const PAGE_LIMIT = 5000;

/** Normalize one raw log line (either source) into a FlowEvent, or null. */
// eslint-disable-next-line sonarjs/cognitive-complexity -- cohesive line-shape parser: sequential field-extraction branches for two log formats; splitting fragments it (#3429)
export function normalizeLine(line: string): FlowEvent | null {
  const t = line.trim();
  if (!t.startsWith('{')) return null;
  let d: Record<string, unknown>;
  try {
    d = JSON.parse(t) as Record<string, unknown>;
  } catch {
    // #3266 — werk.jsonl witness lines were written with a malformed epoch
    // ("ts":17811076913N — BSD date lacks %3N) for their whole history. The
    // emitters are fixed, but the backlog only parses with the N stripped.
    try {
      d = JSON.parse(t.replace(/"ts":(\d+)N,/, '"ts":$1,')) as Record<string, unknown>;
    } catch {
      return null;
    }
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

// eslint-disable-next-line sonarjs/cognitive-complexity -- cohesive paginated Loki fetch loop (page cursor + truncation guard); splitting obscures the pagination (#3429)
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

/** Interactive HTML — data embedded as JSON, rendered client-side with sortable
 *  columns + a time filter over the loaded window. DOM-built (textContent), so
 *  log-sourced strings can't inject markup; the embedded JSON escapes `<` to
 *  block </script> breakout (the gate-quality concern, held in the rewrite). */
// eslint-disable-next-line max-lines-per-function -- single cohesive HTML-document template literal; extracting fragments would not improve clarity (#3429)
export function buildHtml(report: FlowReport & { generatedAt: string; windowHours: number; truncated: boolean; walkAway?: WalkAway }): string {
  const data = JSON.stringify(report).replace(/</g, '\\u003c');
  return `<!DOCTYPE html><html><head><meta charset="utf-8"><title>Card Cycle Report</title>
<style>body{font:13px/1.45 ui-monospace,Menlo,monospace;margin:1.5rem;background:#f7f4ee}table{border-collapse:collapse;width:100%;background:#fff}
th,td{text-align:left;padding:4px 8px;border-bottom:1px solid #eee}th{background:#222;color:#fff;font-size:11px;text-transform:uppercase;cursor:pointer;user-select:none;white-space:nowrap}
th:hover{background:#444}.err{color:#a33;font-size:12px;padding:1px 0}.sub{color:#888;font-size:12px}
.controls{margin:.5rem 0;display:flex;gap:8px;align-items:center}select,input{font:inherit;padding:3px 7px;border:1px solid #ccc;border-radius:6px}</style></head><body>
<h1>Card Cycle Report — cycle · step time · errors enumerated per card</h1>
<div id="head" class="sub"></div>
<div class="controls">show cards active in last
  <select id="tf"><option value="">whole window</option><option value="6">6h</option><option value="12">12h</option><option value="24">24h</option><option value="48">48h</option></select>
  <span class="sub">· click a column header to sort (click again to flip)</span></div>
<table><thead><tr id="hdr"></tr></thead><tbody id="rows"></tbody></table>
<h2>Error classes ranked</h2><ul id="classes"></ul>
<p class="sub">Standing instrument (#3269, chorus_flow_report) — regenerate via the MCP tool; the page is a snapshot of the last call.</p>
<script>
const DATA = ${data};
const COLS = [
  {k:'card',label:'card',v:c=>c.card},
  {k:'landed',label:'landed',v:c=>c.landed?1:0},
  {k:'cycleS',label:'CYCLE',v:c=>c.cycleS},
  {k:'workS',label:'work',v:c=>c.steps.workS},{k:'pushS',label:'push',v:c=>c.steps.pushS},
  {k:'buildS',label:'build',v:c=>c.steps.buildS},{k:'deployS',label:'deploy',v:c=>c.steps.deployS},
  {k:'demoS',label:'demo',v:c=>c.steps.demoS},{k:'mergeS',label:'merge',v:c=>c.steps.mergeS},
  {k:'finalS',label:'final',v:c=>c.steps.finalS},
  {k:'errors',label:'errors / warnings',v:c=>c.errors.length},
];
let sortKey = null, sortDir = -1;
const fmt = s => s===null||s===undefined ? '—' : s>=3600 ? (Math.round(s/360)/10)+'h' : s>=60 ? Math.round(s/60)+'m' : s+'s';
function render(){
  const cutoffH = document.getElementById('tf').value;
  const genMs = Date.parse(DATA.generatedAt);
  let cards = DATA.cards.slice();
  if (cutoffH) cards = cards.filter(c => (c.lastEventTs||genMs) >= genMs - Number(cutoffH)*3600e3);
  if (sortKey){
    const col = COLS.find(x=>x.k===sortKey);
    cards.sort((a,b)=>{
      const va = col.v(a), vb = col.v(b);
      if (va===null||va===undefined) return 1;
      if (vb===null||vb===undefined) return -1;
      return (va<vb?-1:va>vb?1:0)*sortDir;
    });
  }
  const cs = DATA.cycleStats||{};
  document.getElementById('head').textContent =
    'Generated '+DATA.generatedAt+' · '+cards.length+' of '+DATA.totals.cards+' cards ('+DATA.windowHours+'h window) · '
    +DATA.totals.errorEvents+' error/warning events · '+DATA.totals.landed+' landed · CYCLE median '+fmt(cs.medianS)+' / avg '+fmt(cs.avgS)+' / p90 '+fmt(cs.p90S)
    +(DATA.truncated?' · TRUNCATED at page cap':'')
    +(DATA.walkAway ? ' · WALK-AWAY: '+(DATA.walkAway.ready?'READY':'not ready')+' ('+DATA.walkAway.currentStreak+'/'+DATA.walkAway.k+' clean unattended lands)' : '');
  const hdr = document.getElementById('hdr'); hdr.textContent='';
  for (const col of COLS){
    const th = document.createElement('th');
    th.textContent = col.label + (sortKey===col.k ? (sortDir<0?' ▼':' ▲') : '');
    th.onclick = ()=>{ if(sortKey===col.k) sortDir*=-1; else {sortKey=col.k; sortDir=-1;} render(); };
    hdr.appendChild(th);
  }
  const tb = document.getElementById('rows'); tb.textContent='';
  for (const c of cards){
    const tr = document.createElement('tr');
    const cells = ['#'+c.card, c.landed?'✓':'✗', fmt(c.cycleS),
      fmt(c.steps.workS), fmt(c.steps.pushS), fmt(c.steps.buildS), fmt(c.steps.deployS),
      fmt(c.steps.demoS), fmt(c.steps.mergeS), fmt(c.steps.finalS)];
    for (const txt of cells){ const td=document.createElement('td'); td.textContent=txt; tr.appendChild(td); }
    const tde = document.createElement('td');
    if (c.errors.length){
      const det = document.createElement('details');
      const sum = document.createElement('summary'); sum.textContent = c.errors.length+' ▸'; det.appendChild(sum);
      for (const e of c.errors){
        const div = document.createElement('div'); div.className='err';
        div.textContent = new Date(e.ts).toLocaleString('en-US',{timeZone:'America/New_York'})+' · '+e.event+' · '+e.detail;
        det.appendChild(div);
      }
      tde.appendChild(det);
    } else tde.textContent='0';
    tr.appendChild(tde); tb.appendChild(tr);
  }
  const ul = document.getElementById('classes'); ul.textContent='';
  for (const e of DATA.errorClasses){
    const li=document.createElement('li'); const b=document.createElement('b');
    b.textContent=e.event; li.appendChild(b); li.appendChild(document.createTextNode(' × '+e.count)); ul.appendChild(li);
  }
}
document.getElementById('tf').onchange = render;
render();
</script></body></html>`;
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

  // #3266 — the walk-away bar rides the same event set. K default 10 (named with
  // Jeff; override with --k or CHORUS_WALKAWAY_K while the number settles).
  const kIdx = args.indexOf('--k');
  const k = kIdx >= 0 ? Number(args[kIdx + 1]) || 10 : Number(process.env.CHORUS_WALKAWAY_K) || 10;

  const report = {
    generatedAt: new Date().toISOString(),
    windowHours: hours,
    truncated: verbs.truncated || spine.truncated,
    ...aggregateFlow(events),
    walkAway: deriveWalkAway(events, k),
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
