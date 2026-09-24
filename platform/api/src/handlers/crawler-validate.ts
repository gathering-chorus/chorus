/**
 * #4290 — crawler-validate: the control report. Jeff, 2026-09-24: "its our
 * data quality control for crawler - git and graph in tight synch on all
 * domains generated from crawler - not just tests"; "its not an ad hoc query
 * its a control report that shows gaps in both directions"; "kinda like wren
 * has athena-validate this is crawler-validate".
 *
 * The crawler's read-only reconcile (`chorus-crawl --reconcile`) keeps one JSON
 * record per pass under ~/.chorus/crawler-validate/ (CHORUS_VALIDATE_DIR). This
 * module reads those records and renders them: one row per crawler-generated
 * domain, both directions with the names, when the crawler last measured and
 * when main last moved, and the trend of past passes. Green only when every
 * row is measured and 0/0. Hermetic: the directory is a seam.
 */
import fs from 'fs';
import path from 'path';

export interface ValidateRow {
  domain: string;
  class: string;
  tree: number;
  graph: number;
  missing: string[];
  stale: string[];
  /** #4290 — in git, not counted, each with its reason (a file holding no test) */
  excluded: string[];
  measured: boolean;
}

export interface ValidateRecord {
  ts: string;
  head: string;
  headTime: string;
  watermark: string;
  /** when the scheduled crawl last wrote its log (UTC); '' when unknown */
  crawledAt: string;
  clean: boolean;
  gaps: number;
  rows: ValidateRow[];
}

export function validateDir(): string {
  return process.env.CHORUS_VALIDATE_DIR
    || path.join(process.env.HOME || '/tmp', '.chorus', 'crawler-validate');
}

const str = (v: unknown): string => (typeof v === 'string' ? v : '');
const num = (v: unknown): number => (typeof v === 'number' && Number.isFinite(v) ? v : 0);
const strs = (v: unknown): string[] => (Array.isArray(v) ? v.filter((x): x is string => typeof x === 'string') : []);

function asRow(x: unknown): ValidateRow | null {
  if (!x || typeof x !== 'object') return null;
  const y = x as Record<string, unknown>;
  return {
    domain: str(y.domain), class: str(y.class), tree: num(y.tree), graph: num(y.graph),
    missing: strs(y.missing), stale: strs(y.stale), excluded: strs(y.excluded), measured: y.measured === true,
  };
}

function asRecord(raw: unknown): ValidateRecord | null {
  if (!raw || typeof raw !== 'object') return null;
  const r = raw as Record<string, unknown>;
  if (typeof r.ts !== 'string' || !Array.isArray(r.rows)) return null;
  const rows = (r.rows as unknown[]).map(asRow);
  if (rows.some((x) => x === null)) return null;
  const ok = rows as ValidateRow[];
  // clean and gaps are DERIVED here, never trusted from the file: a record
  // that says clean over an unmeasured row is the hollow gate (#3734).
  const gaps = ok.reduce((n, x) => n + x.missing.length + x.stale.length, 0);
  const clean = ok.length > 0 && gaps === 0 && ok.every((x) => x.measured);
  return { ts: r.ts, head: str(r.head), headTime: str(r.headTime), watermark: str(r.watermark), crawledAt: str(r.crawledAt), clean, gaps, rows: ok };
}

/** Every kept pass, oldest first. `latest.json` is a copy and is skipped. */
export function readRecords(dir: string): ValidateRecord[] {
  let names: string[];
  // eslint-disable-next-line security/detect-non-literal-fs-filename -- the record dir is our own config (CHORUS_VALIDATE_DIR or ~/.chorus), never request input
  try { names = fs.readdirSync(dir); } catch { return []; }
  const out: ValidateRecord[] = [];
  for (const n of names) {
    if (!n.endsWith('.json') || n === 'latest.json') continue;
    try {
      // eslint-disable-next-line security/detect-non-literal-fs-filename -- a name listed from that same dir
      const rec = asRecord(JSON.parse(fs.readFileSync(path.join(dir, n), 'utf8')));
      if (rec) out.push(rec);
    } catch { /* an unreadable record is not a pass */ }
  }
  out.sort((a, b) => a.ts.localeCompare(b.ts));
  return out;
}

function esc(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
}

function nameList(label: string, names: string[]): string {
  if (names.length === 0) return '';
  const items = names.map((n) => `<li><code>${esc(n)}</code></li>`).join('');
  return `<details><summary>${names.length} ${esc(label)}</summary><ul>${items}</ul></details>`;
}

function rowHtml(r: ValidateRow): string {
  const gaps = r.missing.length + r.stale.length;
  const state = !r.measured ? '<span class="pill amber">unmeasured</span>'
    : gaps === 0 ? '<span class="pill green">0 / 0</span>'
      : `<span class="pill red">${r.missing.length} / ${r.stale.length}</span>`;
  return `<tr class="${!r.measured ? 'unmeasured' : gaps ? 'red' : 'green'}">`
    + `<td>${esc(r.domain)}</td><td>${esc(r.class)}</td>`
    + `<td class="n">${r.tree}</td><td class="n">${r.graph}</td>`
    + `<td>${state}</td>`
    + `<td>${nameList('in git, no row', r.missing)}${nameList('rows with no source', r.stale)}${nameList('not counted', r.excluded)}`
    + `${!r.measured ? '<span class="note">the graph did not answer for this class — nothing here was compared</span>' : ''}</td></tr>`;
}

function trendHtml(history: ValidateRecord[]): string {
  if (history.length === 0) return '';
  const rows = [...history].reverse().slice(0, 30).map((h) =>
    `<tr><td>${esc(h.ts)}</td><td><code>${esc(h.head.slice(0, 9))}</code></td>`
    + `<td>${h.clean ? '<span class="pill green">clean</span>' : `<span class="pill red">${h.gaps} gap${h.gaps === 1 ? '' : 's'}</span>`}</td>`
    + `<td>${h.rows.filter((r) => !r.measured).length ? '<span class="pill amber">unmeasured rows</span>' : ''}</td></tr>`).join('');
  return `<h2>Past passes</h2><table class="trend"><thead><tr><th>measured at (UTC)</th><th>main</th><th>verdict</th><th></th></tr></thead><tbody>${rows}</tbody></table>`;
}

function bannerHtml(latest: ValidateRecord): string {
  const verdict = latest.clean
    ? 'CLEAN — git and the graph hold the same set on every crawler domain.'
    : `${latest.gaps} gap${latest.gaps === 1 ? '' : 's'} between git and the graph.`;
  const moved = latest.headTime ? ` (main last moved ${esc(latest.headTime)})` : '';
  const crawled = latest.crawledAt
    ? `; the crawler last ran ${esc(latest.crawledAt)}`
    : '; no crawler run found on this box';
  const wm = latest.watermark ? `, watermark <code>${esc(latest.watermark.slice(0, 9))}</code>` : '';
  return `<p class="banner ${latest.clean ? 'green' : 'red'}">${verdict}`
    + ` Measured ${esc(latest.ts)} against <code>${esc(latest.head.slice(0, 9))}</code>${moved}${crawled}${wm}.</p>`;
}

function latestHtml(latest: ValidateRecord | null): string {
  if (latest === null) {
    return '<p class="empty">No pass has been kept yet. The record is written by <code>chorus-crawl --reconcile</code>, which the nightly runs as its first lane.</p>';
  }
  return bannerHtml(latest)
    + '<table class="rows"><thead><tr><th>domain</th><th>class</th><th>in git</th><th>in graph</th><th>missing / stale</th><th>names</th></tr></thead>'
    + `<tbody>${latest.rows.map(rowHtml).join('')}</tbody></table>`;
}

export function renderCrawlerValidatePage(latest: ValidateRecord | null, history: ValidateRecord[]): string {
  const body = latestHtml(latest);
  return `<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>crawler-validate</title>
<style>
:root{--bg:#fafaf7;--ink:#1f2823;--muted:#5d6a63;--rule:#d8ddd6;--green:#2f7a55;--green-bg:#e3f1e8;--red:#a9431f;--red-bg:#f6e1d8;--amber:#9a6b0a;--amber-bg:#f7edd2}
@media (prefers-color-scheme:dark){:root:not([data-theme=light]){--bg:#151917;--ink:#e6eae4;--muted:#9aa69f;--rule:#333c37;--green:#7fcf9f;--green-bg:#1e3a2b;--red:#f09873;--red-bg:#452318;--amber:#e2b657;--amber-bg:#3d3117}}
:root[data-theme=dark]{--bg:#151917;--ink:#e6eae4;--muted:#9aa69f;--rule:#333c37;--green:#7fcf9f;--green-bg:#1e3a2b;--red:#f09873;--red-bg:#452318;--amber:#e2b657;--amber-bg:#3d3117}
body{margin:0;padding:24px 16px 48px;background:var(--bg);color:var(--ink);font:15px/1.45 -apple-system,BlinkMacSystemFont,"IBM Plex Sans",sans-serif}
main{max-width:1100px;margin:0 auto}h1{font-size:24px;margin:0 0 4px}h2{font-size:16px;margin:28px 0 8px}
.sub{color:var(--muted);margin:0 0 18px;max-width:78ch}
.banner{padding:12px 14px;border-radius:6px;margin:0 0 16px}.banner.green{background:var(--green-bg);color:var(--green)}.banner.red{background:var(--red-bg);color:var(--red)}
table{border-collapse:collapse;width:100%}th,td{text-align:left;vertical-align:top;padding:8px 8px;border-bottom:1px solid var(--rule)}th{font-size:12px;letter-spacing:.06em;text-transform:uppercase;color:var(--muted)}
td.n{text-align:right;font-variant-numeric:tabular-nums}
.pill{display:inline-block;padding:2px 9px;border-radius:999px;font-family:ui-monospace,Menlo,monospace;font-size:12px;white-space:nowrap}
.pill.green{background:var(--green-bg);color:var(--green)}.pill.red{background:var(--red-bg);color:var(--red)}.pill.amber{background:var(--amber-bg);color:var(--amber)}
details summary{cursor:pointer;color:var(--muted)}details ul{margin:6px 0 10px;padding-left:18px}code{font-family:ui-monospace,Menlo,monospace;font-size:12.5px}
.note{color:var(--amber);font-size:13px}.empty{color:var(--muted)}
@media (max-width:760px){th:nth-child(2),td:nth-child(2){display:none}}
</style></head><body><main>
<h1>crawler-validate</h1>
<p class="sub">Is the graph what git is? One row per domain the crawler generates: what is in git (or on the box) with no row, and what row has no source. Green only when every row reads 0 / 0. Beside <a href="/api/athena/validate">athena-validate</a>, which checks the model; this checks the data.</p>
${body}
${trendHtml(history)}
</main></body></html>`;
}
