/**
 * POST /api/athena/subdomains/:id/owner — re-assign owner of a SubDomain (#2508).
 *
 * Owner is in the ontology graph (urn:chorus:ontology), seeded from
 * roles/silas/ontology/chorus.ttl. So the write must:
 *   1. Patch chorus.ttl on disk so reload preserves the change
 *   2. DELETE/INSERT the chorus:ownedBy triple in the live ontology graph
 *
 * Replace-semantics (not append): one owner per subdomain.
 */
import type { FetchResult } from './codebase-topology';

const CHORUS_PREFIX = 'https://jeffbridwell.com/chorus#';
const ONTOLOGY_GRAPH = 'urn:chorus:ontology';

const VALID_ID = /^[a-z0-9][a-z0-9._-]*$/i;
const VALID_OWNERS = new Set(['jeff', 'wren', 'silas', 'kade']);

export interface AthenaOwnerWriteDeps {
  sparqlUpdate: (update: string) => Promise<void>;
  readTtl: () => string;
  writeTtl: (content: string) => void;
}

export interface OwnerWriteRequest {
  subdomainId: string;
  body?: { owner?: unknown };
}

function sanitizeId(id: string | undefined | null): string | null {
  if (!id || typeof id !== 'string') return null;
  if (!VALID_ID.test(id)) return null;
  return id;
}

function normalizeOwner(raw: unknown): string | null {
  if (typeof raw !== 'string') return null;
  const lower = raw.trim().toLowerCase();
  return VALID_OWNERS.has(lower) ? lower : null;
}

/**
 * Skip past a Turtle quoted string literal starting at idx (which points at the
 * opening `"`). Returns the index of the character AFTER the closing `"`, or
 * ttl.length if unterminated. Handles backslash escaping.
 */
function skipQuotedString(ttl: string, idx: number): number {
  let i = idx + 1;
  while (i < ttl.length) {
    const c = ttl.charAt(i);
    if (c === '\\') {
      i += 2;
      continue;
    }
    if (c === '"') return i + 1;
    i += 1;
  }
  return ttl.length;
}

/**
 * If ttl at idx is a Turtle block terminator (` .` followed by optional spaces
 * then newline), return the index after the terminator. Otherwise -1.
 */
function matchTerminator(ttl: string, idx: number, startIdx: number): number {
  if (ttl.charAt(idx) !== '.' || idx <= startIdx) return -1;
  const prev = ttl.charAt(idx - 1);
  if (prev !== ' ' && prev !== '\t' && prev !== '\n') return -1;
  let j = idx + 1;
  while (j < ttl.length) {
    const ch = ttl.charAt(j);
    if (ch === ' ' || ch === '\t') {
      j += 1;
      continue;
    }
    if (ch === '\r' && ttl.charAt(j + 1) === '\n') return j + 2;
    if (ch === '\n' || ch === '\r') return j + 1;
    return -1;
  }
  return -1;
}

/**
 * Find the end-of-block index ('.\n') for a Turtle block starting at startIdx,
 * skipping over quoted string literals so embedded periods don't terminate early.
 * Returns the index AFTER the terminator, or -1 if no terminator found.
 */
export function findBlockTerminator(ttl: string, startIdx: number): number {
  let i = startIdx;
  while (i < ttl.length) {
    const c = ttl.charAt(i);
    if (c === '"') {
      i = skipQuotedString(ttl, i);
      continue;
    }
    if (c === '.') {
      const after = matchTerminator(ttl, i, startIdx);
      if (after !== -1) return after;
    }
    i += 1;
  }
  return -1;
}

/**
 * Patch the chorus:ownedBy line within a specific SubDomain block in chorus.ttl.
 * Returns the patched content, or null if the subdomain block isn't found.
 */
export function patchTtlOwner(ttl: string, subdomainId: string, owner: string): string | null {
  // subdomainId is sanitized upstream via VALID_ID; safe in regex.
  // eslint-disable-next-line security/detect-non-literal-regexp
  const blockStart = new RegExp(`^chorus:${subdomainId}\\s+a\\s+chorus:SubDomain\\s*;`, 'm');
  const startMatch = blockStart.exec(ttl);
  if (!startMatch) return null;

  const blockStartIdx = startMatch.index;
  const blockEndIdx = findBlockTerminator(ttl, blockStartIdx);
  if (blockEndIdx === -1) return null;

  const block = ttl.slice(blockStartIdx, blockEndIdx);
  const ownerLineRe = /(\s+chorus:ownedBy\s+)chorus:[a-z]+(\s*;)/;
  if (!ownerLineRe.test(block)) return null;
  const newBlock = block.replace(ownerLineRe, `$1chorus:${owner}$2`);
  return ttl.slice(0, blockStartIdx) + newBlock + ttl.slice(blockEndIdx);
}

export async function setSubdomainOwner(
  deps: AthenaOwnerWriteDeps,
  req: OwnerWriteRequest,
): Promise<FetchResult> {
  const sub = sanitizeId(req.subdomainId);
  if (!sub) {
    return { status: 400, body: { error: 'Invalid subdomainId' } };
  }
  const owner = normalizeOwner(req.body?.owner);
  if (!owner) {
    return {
      status: 400,
      body: { error: `body.owner must be one of: ${Array.from(VALID_OWNERS).join(', ')}` },
    };
  }

  const subjectUri = `${CHORUS_PREFIX}${sub}`;
  const ownerUri = `${CHORUS_PREFIX}${owner}`;

  // 1. Read TTL + compute patch in memory (no disk write yet).
  //    Resolves the 404-or-not check without mutating anything.
  let ttl: string;
  try {
    ttl = deps.readTtl();
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return { status: 500, body: { error: `Failed to read ontology TTL: ${message}` } };
  }

  const patched = patchTtlOwner(ttl, sub, owner);
  if (patched === null) {
    return { status: 404, body: { error: `SubDomain '${sub}' not found in ontology TTL` } };
  }

  // 2. SPARQL update first (gate:arch — Silas): if Fuseki rejects, disk is untouched.
  //    OPTIONAL on the WHERE clause (gate:code — Kade): handles the no-existing-owner case.
  //    Without OPTIONAL, WHERE matches 0 rows, both DELETE and INSERT no-op silently.
  const update = `PREFIX chorus: <${CHORUS_PREFIX}>
    DELETE { GRAPH <${ONTOLOGY_GRAPH}> { <${subjectUri}> chorus:ownedBy ?o } }
    INSERT { GRAPH <${ONTOLOGY_GRAPH}> { <${subjectUri}> chorus:ownedBy <${ownerUri}> } }
    WHERE  { OPTIONAL { GRAPH <${ONTOLOGY_GRAPH}> { <${subjectUri}> chorus:ownedBy ?o } } }`;

  try {
    await deps.sparqlUpdate(update);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return { status: 500, body: { error: `SPARQL update failed: ${message}` } };
  }

  // 3. TTL write last — on SPARQL success only. Failure here leaves live graph
  //    ahead of seed (recoverable: re-run is idempotent).
  try {
    deps.writeTtl(patched);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return { status: 500, body: { error: `Failed to write ontology TTL: ${message}` } };
  }

  return {
    status: 200,
    body: { ok: true, subdomain: sub, owner, subject: subjectUri, ownerUri },
  };
}
