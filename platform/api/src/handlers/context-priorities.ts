/**
 * GET /api/chorus/context/priorities?role=<role> (#3683 — the /sup data source).
 *
 * Answers: "What is this role's priorities walk?" — read from the GRAPH
 * (#3654 board domain), not a label-scrape. Chunks in roleSequence order,
 * cards in rank order within each chunk (the #3681 uniqueness primitive
 * guarantees both orders are tie-free). loomSequence carried where declared.
 *
 * Unsequenced (AC3, honest-visibility): open board cards owned by the role
 * that sit in NO chunk are listed after the walk, labeled with the scope they
 * were read from (the pulse mirror covers Now/WIP/Next/SWAT — #1881). If the
 * mirror is unavailable the walk still serves and the unsequenced block says
 * so instead of silently showing empty.
 *
 * DI mirrors context-board-wip: `sparql` (walk query + envelope stamp),
 * `readPulse` (board mirror). Pure; tests inject stubs.
 */

import {
  stampHeader,
  buildEnvelope,
  type StampSparqlClient,
  type ContextEnvelope,
} from '../lib/context-envelope';

const NS = 'https://jeffbridwell.com/chorus#';
const ROLES = ['wren', 'silas', 'kade', 'jeff'] as const;

export interface ContextPrioritiesDeps {
  sparql: StampSparqlClient;
  /** Returns the raw pulse-latest.json file contents, or null if missing. */
  readPulse: () => string | null;
  /** #3885 — statuses for the sequenced ids, so a landed card leaves the walk.
   *  Returns id → status ('Done', 'Won\'t Do', 'Next', 'Later', …). Absent ids
   *  are treated as still-open: dropping what we cannot confirm is how parked
   *  work would silently disappear. */
  readCardStatuses?: (ids: number[]) => Promise<Map<number, string>>;
  /** #3686 — governed read path: fetch an athena-make GENERATED route (e.g.
   *  '/domains?limit=500') and return its parsed JSON. Levels with a generated
   *  route read through it (Jeff's governed-paths steer); levels without one
   *  (Role — route collision; Card — thin-shape skip) stay SPARQL, labeled. */
  owl: (path: string) => Promise<unknown>;
}

export interface RankedCard {
  id: number;
  title: string;
  rank: number;
}

export interface PriorityChunk {
  chunk: string;
  roleSequence: number;
  loomSequence?: number;
  cards: RankedCard[];
}

export interface UnsequencedBlock {
  /** Where the open-card set came from — or why it's empty. Never silent. */
  scope: string;
  cards: Array<{ id: number; title: string; priority?: string }>;
}

/** #3686 — an ownership level of the walk (products / domains): the role's
 *  things with ordinals first, things WITHOUT an ordinal listed after —
 *  visible, never hidden. `source` names the read path honestly. */
export interface OwnedLevel {
  source: string;
  ordered: Array<{ name: string; ownerSequence: number }>;
  unordered: string[];
  note?: string;
}

export interface RolePriorityLevel {
  source: string;
  /** All roles' hard ranks (rank asc) so the level reads as the team order. */
  ranks: Array<{ role: string; rolePriority: number }>;
  note?: string;
}

export interface PrioritiesData {
  role: string;
  rolePriority: RolePriorityLevel;
  products: OwnedLevel;
  domains: OwnedLevel;
  chunks: PriorityChunk[];
  unsequenced: UnsequencedBlock;
}

export interface ContextPrioritiesResponse {
  status: number;
  body: ContextEnvelope<PrioritiesData> | { error: string };
}

export async function fetchContextPriorities(
  deps: ContextPrioritiesDeps,
  sourceUrl: string,
  role: string,
): Promise<ContextPrioritiesResponse> {
  const r = role.toLowerCase();
  if (!(ROLES as readonly string[]).includes(r)) {
    return { status: 400, body: { error: `unknown role '${role}' — one of ${ROLES.join('/')}` } };
  }

  // One walk query: the role's chunks + (optionally) their ranked memberships.
  // Sorting happens HERE — arrival order is not a contract.
  const walk = `PREFIX chorus: <${NS}> SELECT ?chunkLabel ?roleSeq ?loomSeq ?rank ?cardIri ?cardLabel WHERE { GRAPH <urn:chorus:instances> { ?chunk a chorus:Chunk ; chorus:ownedBy chorus:role-${r} ; chorus:roleSequence ?roleSeq ; chorus:label ?chunkLabel . OPTIONAL { ?chunk chorus:loomSequence ?loomSeq } OPTIONAL { ?m a chorus:ChunkMembership ; chorus:inChunk ?chunk ; chorus:rank ?rank ; chorus:hasCard ?cardIri . ?cardIri chorus:label ?cardLabel } } }`;
  const res = await deps.sparql.query(walk);
  const bindings = res.results?.bindings ?? [];

  const byChunk = new Map<string, PriorityChunk>();
  const sequencedIds = new Set<number>();
  for (const b of bindings) {
    // #3906 — bindings are read through a Map, not by indexing the parsed object.
    // The keys come from a query WE wrote, but the object itself is parsed from an
    // external response: an inherited key (constructor, __proto__) would resolve
    // as a binding and quietly become a chunk label. Object.entries takes own
    // enumerable keys only, so the inherited ones cannot be reached at all.
    const fields = new Map(Object.entries(b as Record<string, { value?: string } | undefined>));
    const val = (k: string): string | undefined => fields.get(k)?.value;
    const label = val('chunkLabel');
    const roleSeq = val('roleSeq');
    if (!label || roleSeq === undefined) continue;
    let chunk = byChunk.get(label);
    if (!chunk) {
      const loom = val('loomSeq');
      chunk = {
        chunk: label,
        roleSequence: Number(roleSeq),
        ...(loom !== undefined && { loomSequence: Number(loom) }),
        cards: [],
      };
      byChunk.set(label, chunk);
    }
    const rank = val('rank');
    const cardIri = val('cardIri');
    if (rank !== undefined && cardIri) {
      const id = vikunjaId(cardIri);
      if (id !== null) {
        sequencedIds.add(id);
        chunk.cards.push({ id, title: val('cardLabel') ?? '', rank: Number(rank) });
      }
    }
  }
  // #3885 — Jeff, 2026-08-17: the walk is "what's left, uncluttered by what is
  // done". Membership rows carry an id and a rank and NO status, so five landed
  // cards were being presented to Silas as open work and misordering his pulls.
  //
  // Closed statuses are named explicitly rather than inferred from absence: the
  // board mirror only carries wip/next/swat, so "not in the mirror" means Done OR
  // parked in Later, and dropping on absence would hide real work.
  await dropClosedCards(byChunk, sequencedIds, deps.readCardStatuses);

  const chunks = [...byChunk.values()].sort((a, b) => a.roleSequence - b.roleSequence);
  for (const c of chunks) c.cards.sort((a, b) => a.rank - b.rank);

  const unsequenced = readUnsequenced(deps.readPulse(), r, sequencedIds);

  // #3686 — the upper walk levels. products/domains via the GENERATED athena-make
  // routes; rolePriority via SPARQL until the Role route collision is fixed.
  const [products, domains, rolePriority] = await Promise.all([
    readOwnedLevel(deps, '/products?limit=500', r),
    readOwnedLevel(deps, '/domains?limit=500', r),
    readRolePriority(deps),
  ]);

  const header = await stampHeader(deps.sparql, 'chorus');
  const envelope = buildEnvelope(header, sourceUrl, {
    role: r,
    rolePriority,
    products,
    domains,
    chunks,
    unsequenced,
  });
  return { status: 200, body: envelope };
}

/** #3686 — one ownership level (products/domains) read from a GENERATED
 *  athena-make collection route. Filters to the role's things (ownedBy carries
 *  either the bare or role- prefixed form in the live data — match both),
 *  ordinals first (asc), the rest listed unordered. Read failure degrades the
 *  level with a note — the walk itself never 500s on a level source. */
async function readOwnedLevel(
  deps: ContextPrioritiesDeps,
  path: string,
  role: string,
): Promise<OwnedLevel> {
  const source = `athena-make:${path}`;
  let rows: Array<Record<string, unknown>>;
  try {
    const body = (await deps.owl(path)) as { data?: unknown };
    rows = Array.isArray(body.data)
      ? (body.data as unknown[]).filter((x): x is Record<string, unknown> => !!x && typeof x === 'object')
      : [];
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return { source, ordered: [], unordered: [], note: `generated route unavailable: ${message}` };
  }
  const mine = rows.filter((row) => {
    const o = typeof row.ownedBy === 'string' ? (row.ownedBy as string).toLowerCase() : '';
    return o === role || o === `role-${role}`;
  });
  const named = (row: Record<string, unknown>): string =>
    (typeof row.label === 'string' && row.label) || (typeof row.name === 'string' && row.name) || '';
  const ordered = mine
    .filter((row) => row.ownerSequence !== undefined && row.ownerSequence !== '')
    .map((row) => ({ name: named(row), ownerSequence: Number(row.ownerSequence) }))
    .sort((a, b) => a.ownerSequence - b.ownerSequence);
  const unordered = mine
    .filter((row) => row.ownerSequence === undefined || row.ownerSequence === '')
    .map(named)
    .sort();
  const note = rows.length === 0 ? 'route serves no data (instancesGraph decl pending on the shape)' : undefined;
  return { source, ordered, unordered, ...(note && { note }) };
}

/** #3686 — the role-vs-role hard rank. SPARQL residual: the Role class
 *  collection is shadowed by the roles DOMAIN vocab page (route collision), so
 *  there is no generated read for rolePriority yet — labeled, not hidden. */
async function readRolePriority(deps: ContextPrioritiesDeps): Promise<RolePriorityLevel> {
  const source = 'sparql (residual — Role collection route shadowed by the roles domain page collision)';
  const q = `PREFIX chorus: <${NS}> SELECT ?role ?pri WHERE { GRAPH <urn:chorus:instances> { ?s chorus:rolePriority ?pri . BIND(REPLACE(STR(?s), ".*#role-", "") AS ?role) } }`;
  try {
    const res = await deps.sparql.query(q);
    const ranks = (res.results?.bindings ?? [])
      .map((b) => {
        const row = b as Record<string, { value?: string } | undefined>;
        return { role: row.role?.value ?? '', rolePriority: Number(row.pri?.value ?? 0) };
      })
      .filter((x) => x.role)
      .sort((a, b) => a.rolePriority - b.rolePriority);
    return { source, ranks, ...(ranks.length === 0 && { note: 'no rolePriority set yet' }) };
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return { source, ranks: [], note: `read failed: ${message}` };
  }
}

/** card IRI `<NS>card-<vikunja-id>` → numeric id, or null if non-conformant. */
/**
 * #3885 — closed statuses are NAMED, never inferred from absence.
 *
 * The board mirror carries only wip/next/swat, so "missing" means Done OR parked
 * in Later. Of the five rows reported stale on 2026-08-17, two were Later — real
 * work an absence-based filter would have hidden. An unknown status therefore
 * counts as open: a failed read must not empty the walk.
 */
const CLOSED_STATUSES = new Set(['done', "won't do", 'wont do']);

function isOpenStatus(status: string | undefined): boolean {
  return status === undefined || !CLOSED_STATUSES.has(status.trim().toLowerCase());
}

/** #3885 — drop landed rows from every chunk. Kept out of the walk builder so
 *  that function does not grow another branch; the rule is in isOpenStatus. */
async function dropClosedCards(
  byChunk: Map<string, PriorityChunk>,
  ids: Set<number>,
  read?: (ids: number[]) => Promise<Map<number, string>>,
): Promise<void> {
  if (!read || ids.size === 0) return;
  const statuses = await read([...ids]);
  for (const chunk of byChunk.values()) {
    chunk.cards = chunk.cards.filter((c) => isOpenStatus(statuses.get(c.id)));
  }
}

function vikunjaId(iri: string): number | null {
  const m = /#card-(\d+)$/.exec(iri);
  return m ? Number(m[1]) : null;
}

function readUnsequenced(raw: string | null, role: string, sequenced: Set<number>): UnsequencedBlock {
  if (raw === null) {
    return { scope: 'board mirror unavailable — unsequenced set unknown, not empty', cards: [] };
  }
  let pulse: unknown;
  try {
    pulse = JSON.parse(raw);
  } catch {
    return { scope: 'board mirror unparseable — unsequenced set unknown, not empty', cards: [] };
  }
  const board = (pulse as { board?: Record<string, unknown> }).board ?? {};
  // #3906 — same treatment for the pulse mirror. The three keys are literals
  // today, so this one is provably safe; written this way so it STAYS safe if the
  // list ever becomes dynamic, which is how the first one got in.
  const lanes = new Map(Object.entries(board));
  const readList = (k: string): unknown[] => {
    const v = lanes.get(k);
    return Array.isArray(v) ? v : [];
  };
  const open = ['wip_cards', 'next_cards', 'swat_cards']
    .flatMap(readList)
    .filter((c): c is Record<string, unknown> => !!c && typeof c === 'object');

  const cards = open
    .filter((c) => typeof c.owner === 'string' && (c.owner as string).toLowerCase() === role)
    .filter((c) => typeof c.id === 'number' && !sequenced.has(c.id as number))
    .map((c) => ({
      id: c.id as number,
      title: typeof c.title === 'string' ? c.title : '',
      ...(typeof c.priority === 'string' && c.priority && { priority: c.priority }),
    }))
    .sort((a, b) => a.id - b.id);

  return { scope: 'open cards (Now/WIP/Next/SWAT via pulse mirror) not in any chunk', cards };
}
