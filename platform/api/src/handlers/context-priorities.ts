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

export interface PrioritiesData {
  role: string;
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
    const val = (k: string): string | undefined => (b as Record<string, { value?: string } | undefined>)[k]?.value;
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
  const chunks = [...byChunk.values()].sort((a, b) => a.roleSequence - b.roleSequence);
  for (const c of chunks) c.cards.sort((a, b) => a.rank - b.rank);

  const unsequenced = readUnsequenced(deps.readPulse(), r, sequencedIds);

  const header = await stampHeader(deps.sparql, 'chorus');
  const envelope = buildEnvelope(header, sourceUrl, { role: r, chunks, unsequenced });
  return { status: 200, body: envelope };
}

/** card IRI `<NS>card-<vikunja-id>` → numeric id, or null if non-conformant. */
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
  const open = ['wip_cards', 'next_cards', 'swat_cards']
    .flatMap((k) => (Array.isArray(board[k]) ? (board[k] as unknown[]) : []))
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
