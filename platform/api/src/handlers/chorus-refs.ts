/**
 * GET /api/chorus/refs — cross-entity reference lookup (#2188).
 *
 * Query filters (at least one required):
 *   card     — card ID (with or without leading #); stored with # prefix
 *   wf       — workflow ID (with or without WF- prefix); stored with WF-
 *   type     — raw entity_type
 *   entityId — raw entity_id
 *
 * Dependencies injected:
 *   db — better-sqlite3 Database handle (in-memory acceptable for tests)
 *
 * Behavior:
 *   - Missing all filters → 400
 *   - Card/wf filters normalize their prefix before matching
 *   - Card/wf take precedence over raw type/entityId
 *   - Results ordered by message timestamp DESC, limit 50
 *   - message.content truncated to 500 chars; other message fields pass through
 */
import type Database from 'better-sqlite3';
import type { FetchResult } from './codebase-topology';

export interface ChorusRefsQuery {
  card?: string;
  wf?: string;
  type?: string;
  entityId?: string;
}

export interface ChorusRefsDeps {
  db: Database.Database;
}

interface RefRow {
  entity_type: string;
  entity_id: string;
  relationship: string;
  content: string | null;
  timestamp: string;
  role: string;
  source: string;
  channel: string;
}

// #2627: WHERE-clause + params build extracted; orchestrator becomes flat.
function buildWhereAndParams(query: ChorusRefsQuery): { where: string[]; params: unknown[] } {
  const where: string[] = [];
  const params: unknown[] = [];
  const { card, wf, type, entityId } = query;
  if (card) {
    where.push('r.entity_type = ? AND r.entity_id = ?');
    params.push('card', card.startsWith('#') ? card : `#${card}`);
    return { where, params };
  }
  if (wf) {
    where.push('r.entity_type = ? AND r.entity_id = ?');
    params.push('workflow', wf.startsWith('WF-') ? wf : `WF-${wf}`);
    return { where, params };
  }
  if (type) { where.push('r.entity_type = ?'); params.push(type); }
  if (entityId) { where.push('r.entity_id = ?'); params.push(entityId); }
  return { where, params };
}

export function fetchChorusRefs(deps: ChorusRefsDeps, query: ChorusRefsQuery): FetchResult {
  const { card, wf, type, entityId } = query;
  if (!card && !wf && !type && !entityId) {
    return { status: 400, body: { error: 'At least one filter required: card, wf, type, or id' } };
  }
  const { where, params } = buildWhereAndParams(query);

  const refs = deps.db.prepare(`
    SELECT r.entity_type, r.entity_id, r.relationship,
           m.content, m.timestamp, m.role, m.source, m.channel
    FROM refs r
    JOIN messages m ON r.message_id = m.id
    WHERE ${where.join(' AND ')}
    ORDER BY m.timestamp DESC
    LIMIT 50
  `).all(...params) as RefRow[];

  const formatted = refs.map((r) => ({
    entity_type: r.entity_type,
    entity_id: r.entity_id,
    relationship: r.relationship,
    message: {
      content: r.content?.substring(0, 500),
      timestamp: r.timestamp,
      role: r.role,
      source: r.source,
      channel: r.channel,
    },
  }));

  return { status: 200, body: { refs: formatted } };
}
