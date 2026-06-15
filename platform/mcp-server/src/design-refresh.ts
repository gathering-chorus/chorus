/* eslint-disable security/detect-unsafe-regex, security/detect-non-literal-regexp -- #3429: regexes built from internal, fixed design-doc patterns (not untrusted input); security flags are false positives on this internal tooling */
// #2900 — chorus_design_refresh MCP execute function.
//
// Reads a service-design HTML, validates it against the canonical template
// (designing/templates/service-design.html), pulls current card statuses
// from the cards CLI for every #NNNN reference, and regenerates the
// cite-density sections marked with `data-section` attributes on their
// <h2> headings.
//
// Human-authored sections (summary block, Overview, As-Is, To-Be,
// per-domain blocks) are never touched. The MCP enforces structural
// compliance up front (refuses summary-missing / template-violation) so
// drift between designs stays bounded.

import * as path from 'path';
import { execFile as execFileCb } from 'child_process';
import { promisify } from 'util';

const execFileAsync = promisify(execFileCb);

export type Role = 'kade' | 'wren' | 'silas';

export interface DesignRefreshArgs {
  role: Role;
  design_name: string;
}

export interface CardStatus {
  card_id: number;
  status: 'Done' | 'WIP' | 'Next' | 'Later' | 'Won\'t Do' | 'Unknown';
  title?: string;
}

export interface DesignRefreshResult {
  design_name: string;
  sections_regenerated: string[];
  cards_referenced: number[];
  diff_lines: number;
  cards_by_status: Record<string, number[]>;
}

export type EmitSpine = (event: string, fields: Record<string, unknown>) => void;

export interface DesignRefreshDeps {
  readFile: (p: string) => string;
  writeFile: (p: string, content: string) => void;
  cardsPath: string;
  designsDir: string;
  emit: EmitSpine;
  execFile?: typeof execFileAsync;
}

// Card-id reference pattern: #NNNN where NNNN is 1+ digits. Used for both
// extraction and the references section regeneration.
const CARD_REF_RE = /#(\d{2,5})\b/g;

// Sections marked with data-section attribute on <h2> are MCP-regenerated.
const MCP_SECTIONS = [
  'references',
  'path-to-close',
  'gaps',
] as const;

export class DesignRefreshError extends Error {
  constructor(
    public reason:
      | 'design-not-found'
      | 'template-violation'
      | 'summary-missing'
      | 'manifest-missing'
      | 'regenerate-fail',
    public detail: string,
  ) {
    super(`chorus_design_refresh refused: ${reason} — ${detail}`);
    this.name = 'DesignRefreshError';
  }
}

/**
 * Resolve the design file path, refusing if it doesn't exist.
 */
function resolveDesignPath(designsDir: string, designName: string, readFile: (p: string) => string): string {
  // Normalize: strip leading paths or .html suffix; we want just the stem.
  const stem = designName.replace(/\.html$/, '').split('/').pop() ?? designName;
  const candidate = path.join(designsDir, `${stem}.html`);
  try {
    readFile(candidate);
    return candidate;
  } catch {
    throw new DesignRefreshError('design-not-found', `${candidate} does not exist or is unreadable`);
  }
}

/**
 * #2900 — Auto-conform: scaffold the canonical structure onto a doc that
 * lacks it. Inserts a placeholder summary-block after the date line (if
 * missing) and adds `data-section` attributes to standard H2 headings.
 * Returns the modified HTML plus a list of what was scaffolded — empty
 * list means the doc was already template-compliant.
 *
 * The scaffold uses `{{PLACEHOLDER}}` tokens for human-authored content;
 * the human fills them in after auto-conform and re-runs refresh.
 */
export function autoConform(html: string): { html: string; scaffolded: string[] } {
  let working = html;
  const scaffolded: string[] = [];

  // 1. Insert summary-block if missing.
  if (!/<div\s+class="summary-block"/i.test(working)) {
    const summaryBlock = `
<!-- ===== SUMMARY BLOCK — auto-inserted by chorus_design_refresh (#2900) =====
     Fill in the {{PLACEHOLDER}} tokens, then re-run /design-refresh. -->
<div class="summary-block">
  <h2>At a Glance</h2>

  <pre class="mermaid">
flowchart LR
  A[Actor or input] --> B[Domain surface]
  B --> C[Output or effect]
  B --> D[Spine emit]
  </pre>
  <p style="font-size:0.8em;color:#999;font-style:italic;">Replace node labels above with the real actors / surfaces / outputs / spine events for this domain.</p>

  <table class="gap-and-next">
    <thead><tr><th>Status</th><th>Where</th><th>Next</th></tr></thead>
    <tbody>
      <tr><td>{{STATUS_1}}</td><td>{{WHERE_1}}</td><td>{{NEXT_1}}</td></tr>
      <tr><td>{{STATUS_2}}</td><td>{{WHERE_2}}</td><td>{{NEXT_2}}</td></tr>
      <tr><td>{{STATUS_3}}</td><td>{{WHERE_3}}</td><td>{{NEXT_3}}</td></tr>
    </tbody>
  </table>

  <p class="invariants"><strong>Invariants:</strong> {{INVARIANT_PARAGRAPH}}</p>
</div>
`;
    // Try to insert right after the date line. Fall back to right after <body>.
    const dateLineRe = /(<p\s+class="date"[^>]*>[\s\S]*?<\/p>)/i;
    if (dateLineRe.test(working)) {
      working = working.replace(dateLineRe, `$1\n${summaryBlock}`);
    } else {
      // No date line — insert right after <body>.
      working = working.replace(/(<body[^>]*>)/i, `$1\n${summaryBlock}`);
    }
    scaffolded.push('summary-block');
  }

  // 2. Add data-section attributes to standard H2 headings that lack them.
  const sectionMap: Array<{ section: McpSection; pattern: RegExp }> = [
    { section: 'path-to-close', pattern: /<h2(?![^>]*data-section=)(\s[^>]*)?>(\s*Path to close[^<]*)<\/h2>/i },
    { section: 'gaps', pattern: /<h2(?![^>]*data-section=)(\s[^>]*)?>(\s*Gaps to close[^<]*)<\/h2>/i },
    { section: 'references', pattern: /<h2(?![^>]*data-section=)(\s[^>]*)?>(\s*References[^<]*)<\/h2>/i },
  ];
  for (const { section, pattern } of sectionMap) {
    if (pattern.test(working)) {
      working = working.replace(pattern, (_m, attrs: string | undefined, text: string) => {
        const existingAttrs = attrs ? attrs.trim() : '';
        return `<h2 data-section="${section}"${existingAttrs ? ' ' + existingAttrs : ''}>${text}</h2>`;
      });
      scaffolded.push(`${section}.data-section`);
    }
  }

  return { html: working, scaffolded };
}

type McpSection = (typeof MCP_SECTIONS)[number];

/**
 * Template compliance — refuse early so we never regenerate a doc that
 * doesn't follow the canonical structure.
 */
export function validateTemplate(html: string): void {
  if (!/<div\s+class="summary-block"/i.test(html)) {
    throw new DesignRefreshError(
      'summary-missing',
      'no <div class="summary-block"> found — every design must lead with a one-page skim layer',
    );
  }
  // Confirm the three required sub-elements are present inside the summary block.
  // We don't enforce content quality (that's human-authored), just structural presence.
  if (!/<pre[^>]*class="[^"]*mermaid/i.test(html)) {
    throw new DesignRefreshError(
      'summary-missing',
      'summary block has no <pre class="mermaid"> diagram — first element must be the at-a-glance diagram',
    );
  }
  if (!/class="gap-and-next"/i.test(html)) {
    throw new DesignRefreshError(
      'summary-missing',
      'summary block has no <table class="gap-and-next"> — second element must be the ≤6-row status table',
    );
  }
  if (!/class="invariants"/i.test(html)) {
    throw new DesignRefreshError(
      'summary-missing',
      'summary block has no <p class="invariants"> — third element must be the invariants paragraph',
    );
  }
  // At least one MCP-regenerated section must be present, otherwise nothing
  // to do — likely a malformed template.
  const hasAnyDataSection = MCP_SECTIONS.some((s) =>
    new RegExp(`<h2\\s+data-section=["']${s}["']`, 'i').test(html),
  );
  if (!hasAnyDataSection) {
    throw new DesignRefreshError(
      'template-violation',
      `no <h2 data-section="..."> headings found — expected at least one of: ${MCP_SECTIONS.join(', ')}`,
    );
  }
}

/**
 * Extract all #NNNN card references from the document, deduplicated.
 */
export function extractCardRefs(html: string): number[] {
  const matches = html.matchAll(CARD_REF_RE);
  const ids = new Set<number>();
  for (const m of matches) {
    const id = Number(m[1]);
    if (!Number.isNaN(id) && id > 0) ids.add(id);
  }
  return Array.from(ids).sort((a, b) => a - b);
}

/**
 * Look up current card statuses via `cards view <id> --json`. Returns Unknown
 * for cards that don't view (deleted, never created). Never refuses — missing
 * cards just get tagged Unknown so the doc still regenerates.
 */
async function fetchCardStatuses(
  cardIds: number[],
  cardsPath: string,
  execFile: typeof execFileAsync,
): Promise<Map<number, CardStatus>> {
  const results = new Map<number, CardStatus>();
  for (const id of cardIds) {
    try {
      const { stdout } = await execFile(cardsPath, ['view', String(id), '--json'], { timeout: 8_000 });
      const parsed = JSON.parse(stdout) as { status?: string; title?: string };
      const rawStatus = parsed.status ?? 'Unknown';
      const status = normalizeStatus(rawStatus);
      results.set(id, { card_id: id, status, title: parsed.title });
    } catch {
      results.set(id, { card_id: id, status: 'Unknown' });
    }
  }
  return results;
}

function normalizeStatus(raw: string): CardStatus['status'] {
  const trimmed = raw.trim();
  if (/^done$/i.test(trimmed)) return 'Done';
  if (/^wip$/i.test(trimmed)) return 'WIP';
  if (/^next$/i.test(trimmed)) return 'Next';
  if (/^later$/i.test(trimmed)) return 'Later';
  if (/won.?t.?do/i.test(trimmed)) return "Won't Do";
  return 'Unknown';
}

/**
 * Regenerate the References section: add a (Status) suffix after each
 * #NNNN reference where status is known. Idempotent — re-running strips
 * any prior (Status) tag first to avoid stacking.
 */
export function regenerateReferences(
  html: string,
  cardStatuses: Map<number, CardStatus>,
): { html: string; changed: boolean } {
  const sectionRe =
    /(<h2\s+data-section=["']references["'][^>]*>[\s\S]*?<\/h2>[\s\S]*?<ul>)([\s\S]*?)(<\/ul>)/i;
  const m = html.match(sectionRe);
  if (!m) return { html, changed: false };

  const original = m[2];
  // Strip prior (Done|WIP|Next|Later|Won't Do|Unknown) tags on card refs.
  let updated = original.replace(
    /(#\d{2,5})\s*\((?:Done|WIP|Next|Later|Won't Do|Unknown)\)/g,
    '$1',
  );
  // Append current status after each #NNNN.
  updated = updated.replace(CARD_REF_RE, (match, idStr: string) => {
    const id = Number(idStr);
    const c = cardStatuses.get(id);
    if (!c || c.status === 'Unknown') return match;
    return `${match} (${c.status})`;
  });
  if (updated === original) return { html, changed: false };
  return { html: html.replace(sectionRe, `$1${updated}$3`), changed: true };
}

/**
 * Regenerate the Path-to-close section: add a status tag prefix to each <li>
 * that mentions a card #NNNN. Items that don't reference a card are left
 * alone (they may be unscoped or shipped without a card). The first card
 * reference in the item determines the status tag.
 *
 * Tag format: <strong>{STATUS_TAG}.</strong> where STATUS_TAG = SHIPPED |
 * WIP | NEXT | LATER | WON'T DO. Idempotent — strips prior tag before
 * adding.
 */
export function regeneratePathToClose(
  html: string,
  cardStatuses: Map<number, CardStatus>,
): { html: string; changed: boolean } {
  const sectionRe =
    /(<h2\s+data-section=["']path-to-close["'][^>]*>[\s\S]*?<\/h2>[\s\S]*?<ol>)([\s\S]*?)(<\/ol>)/i;
  const m = html.match(sectionRe);
  if (!m) return { html, changed: false };

  const original = m[2];
  // Process each <li> ... </li> block.
  const updated = original.replace(/<li>([\s\S]*?)<\/li>/g, (_full, inner: string) => {
    // Strip prior MCP-added status tag if present: <strong>SHIPPED.</strong>
    // (also handles WIP / NEXT / LATER / WON'T DO / UNKNOWN variants).
    const stripped = inner.replace(
      /^\s*<strong>(?:SHIPPED|WIP|NEXT|LATER|WON'T DO|UNKNOWN)\.<\/strong>\s*/i,
      '',
    );
    // If the item already contains a hand-authored status indicator
    // (SHIPPED 2026-05-NN, WON'T DO, NOT YET SHIPPED, PARTIALLY SHIPPED,
    // UNSCOPED, etc.), the author chose richer wording than we'd add —
    // leave it alone. The MCP's job here is correctness, not duplication.
    const hasAuthoredStatus =
      /\b(?:SHIPPED|NOT YET SHIPPED|PARTIALLY SHIPPED|WON'T DO|UNSCOPED|DEFERRED|WIP|NEXT|LATER)\b/.test(stripped);
    if (hasAuthoredStatus) return `<li>${stripped}</li>`;
    const cardMatch = stripped.match(CARD_REF_RE);
    if (!cardMatch) return `<li>${stripped}</li>`;
    const firstId = Number(cardMatch[0].slice(1));
    const c = cardStatuses.get(firstId);
    if (!c) return `<li>${stripped}</li>`;
    const tag = statusToPathTag(c.status);
    if (!tag) return `<li>${stripped}</li>`;
    return `<li><strong>${tag}.</strong> ${stripped}</li>`;
  });
  if (updated === original) return { html, changed: false };
  return { html: html.replace(sectionRe, `$1${updated}$3`), changed: true };
}

function statusToPathTag(status: CardStatus['status']): string | null {
  if (status === 'Done') return 'SHIPPED';
  if (status === 'WIP') return 'WIP';
  if (status === 'Next') return 'NEXT';
  if (status === 'Later') return 'LATER';
  if (status === "Won't Do") return "WON'T DO";
  return null;
}

/**
 * Regenerate Gaps section by appending a (Status) suffix to each #NNNN
 * mention. Reclassification across H3 sub-headings (Closed-since / Open /
 * Won't-do) is intentionally NOT done by this MVP — the human-authored
 * grouping is preserved. Card-status-as-suffix is the minimum surface that
 * keeps the doc current without overwriting editorial intent.
 */
export function regenerateGaps(
  html: string,
  cardStatuses: Map<number, CardStatus>,
): { html: string; changed: boolean } {
  const sectionRe =
    /(<h2\s+data-section=["']gaps["'][^>]*>[\s\S]*?<\/h2>)([\s\S]*?)(?=<h2)/i;
  const m = html.match(sectionRe);
  if (!m) return { html, changed: false };

  const original = m[2];
  let updated = original.replace(
    /(#\d{2,5})\s*\((?:Done|WIP|Next|Later|Won't Do|Unknown)\)/g,
    '$1',
  );
  updated = updated.replace(CARD_REF_RE, (match, idStr: string) => {
    const id = Number(idStr);
    const c = cardStatuses.get(id);
    if (!c || c.status === 'Unknown') return match;
    return `${match} (${c.status})`;
  });
  if (updated === original) return { html, changed: false };
  return { html: html.replace(sectionRe, `$1${updated}`), changed: true };
}

/**
 * Main execute function. Validates, fetches, regenerates, writes, emits.
 */
export async function executeDesignRefresh(
  args: DesignRefreshArgs,
  deps: DesignRefreshDeps,
): Promise<DesignRefreshResult> {
  const { role, design_name } = args;
  const execFile = deps.execFile ?? execFileAsync;

  deps.emit('design.refresh.started', {
    role,
    design_name,
    cards_referenced_count: 0,
  });

  let designPath: string;
  try {
    designPath = resolveDesignPath(deps.designsDir, design_name, deps.readFile);
  } catch (err) {
    if (err instanceof DesignRefreshError) {
      deps.emit('design.refresh.failed', { role, domain: 'chorus', design_name, reason: err.reason, detail: err.detail });
    }
    throw err;
  }

  const original = deps.readFile(designPath);

  // #2900 — Auto-conform: scaffold missing summary-block and/or data-section
  // attributes before validation. Refusal still fires for unrecoverable cases
  // (e.g. summary-block scaffold inserted but downstream structure is still
  // malformed). Single-command refresh works on any doc; the scaffold gets
  // {{PLACEHOLDER}} tokens the human fills before re-running.
  const conform = autoConform(original);
  let working = conform.html;
  if (conform.scaffolded.length > 0) {
    deps.emit('design.scaffold.inserted', {
      role,
      design_name,
      scaffolded: conform.scaffolded.join(','),
    });
    deps.writeFile(designPath, working);
  }

  try {
    validateTemplate(working);
  } catch (err) {
    if (err instanceof DesignRefreshError) {
      deps.emit('design.refresh.failed', { role, domain: 'chorus', design_name, reason: err.reason, detail: err.detail });
    }
    throw err;
  }

  const cardIds = extractCardRefs(working);
  const statuses = await fetchCardStatuses(cardIds, deps.cardsPath, execFile);

  const regenerated: string[] = [];

  const refs = regenerateReferences(working, statuses);
  if (refs.changed) {
    working = refs.html;
    regenerated.push('references');
  }
  const ptc = regeneratePathToClose(working, statuses);
  if (ptc.changed) {
    working = ptc.html;
    regenerated.push('path-to-close');
  }
  const gaps = regenerateGaps(working, statuses);
  if (gaps.changed) {
    working = gaps.html;
    regenerated.push('gaps');
  }

  const originalLines = original.split('\n').length;
  const newLines = working.split('\n').length;
  const diffLines = newLines - originalLines;

  if (working !== original) {
    try {
      deps.writeFile(designPath, working);
    } catch (err) {
      const detail = err instanceof Error ? err.message : String(err);
      deps.emit('design.refresh.failed', { role, domain: 'chorus', design_name, reason: 'regenerate-fail', detail });
      throw new DesignRefreshError('regenerate-fail', `write failed: ${detail}`);
    }
  }

  const cardsByStatus: Record<string, number[]> = {};
  for (const [id, c] of statuses) {
    cardsByStatus[c.status] = cardsByStatus[c.status] ?? [];
    cardsByStatus[c.status].push(id);
  }

  deps.emit('design.refreshed', {
    role,
    design_name,
    sections_regenerated: regenerated.join(','),
    cards_referenced: cardIds.join(','),
    diff_lines: String(diffLines),
  });

  return {
    design_name,
    sections_regenerated: regenerated,
    cards_referenced: cardIds,
    diff_lines: diffLines,
    cards_by_status: cardsByStatus,
  };
}
