/**
 * #2895 — card proposal storage for agent-initiated `cards add`.
 *
 * Bouncer at the door: agents propose a card via POST /api/cards/proposals
 * and the CLI blocks until Jeff approves, denies, or 10 minutes elapse.
 *
 * In-memory store. Proposals live ~10 minutes (TTL), then auto-timeout.
 * That's deliberate: this is ephemeral approval state, not persistent.
 * If chorus-api restarts, in-flight proposals time out — agent retries,
 * Jeff approves the new proposal. No durable state to corrupt.
 *
 * Spine events are emitted via the local chorus-log binary so they land in
 * ~/.chorus/chorus.log alongside every other event.
 */
import { execFileSync } from 'child_process';
import * as crypto from 'crypto';

export type ProposalStatus = 'pending' | 'approved' | 'denied' | 'timeout';

export interface CardProposal {
  id: string;
  role: string;          // proposing role (wren / silas / kade)
  title: string;
  owner: string;
  priority: string;
  domain: string;
  type: string;
  origin: string;
  sequence: string;
  description: string;
  submittedAt: number;   // ms epoch
  status: ProposalStatus;
  deniedReason?: string;
  decidedAt?: number;
  cardId?: number;       // #2895 phase 2: filled on approve when chorus-api files the card
  cardFileError?: string;
}

const TIMEOUT_MS = 10 * 60 * 1000; // 10 minutes

class ProposalStore {
  private byId = new Map<string, CardProposal>();
  private sweeper?: NodeJS.Timeout;

  constructor() {
    // Sweep timed-out proposals every 30s.
    this.sweeper = setInterval(() => this.sweepTimeouts(), 30_000);
    this.sweeper.unref?.();
  }

  submit(input: Omit<CardProposal, 'id' | 'submittedAt' | 'status'>): CardProposal {
    const id = crypto.randomUUID();
    const proposal: CardProposal = {
      ...input,
      id,
      submittedAt: Date.now(),
      status: 'pending',
    };
    this.byId.set(id, proposal);
    emitSpine('card.proposal.submitted', input.role, {
      proposal_id: id,
      title: input.title,
      owner: input.owner,
      type: input.type,
      priority: input.priority,
    });
    return proposal;
  }

  get(id: string): CardProposal | undefined {
    return this.byId.get(id);
  }

  /**
   * #2895 phase 2: approve flips status AND calls chorus-api to file the actual
   * card. POST /api/cards/add with X-Role: jeff so the spawned cards CLI sets
   * DEPLOY_ROLE=jeff and bypasses the proposal hook (the hook fires only for
   * agent roles). cardId stored on proposal so GET /api/cards/proposals/:id
   * returns it.
   */
  async approve(id: string): Promise<CardProposal | undefined> {
    const p = this.byId.get(id);
    if (!p || p.status !== 'pending') return p;
    p.status = 'approved';
    p.decidedAt = Date.now();
    const result = await fileApprovedCard(p);
    if (result.cardId) p.cardId = result.cardId;
    if (result.error) p.cardFileError = result.error;
    emitSpine('card.proposal.approved', 'jeff', {
      proposal_id: id, title: p.title, proposer: p.role,
      ...(p.cardId ? { card_id: p.cardId } : {}),
      ...(p.cardFileError ? { card_file_error: p.cardFileError } : {}),
    });
    return p;
  }

  deny(id: string, reason?: string): CardProposal | undefined {
    const p = this.byId.get(id);
    if (!p || p.status !== 'pending') return p;
    p.status = 'denied';
    p.deniedReason = reason || '';
    p.decidedAt = Date.now();
    emitSpine('card.proposal.denied', 'jeff', { proposal_id: id, title: p.title, proposer: p.role, reason: reason || '' });
    return p;
  }

  pending(): CardProposal[] {
    return Array.from(this.byId.values())
      .filter(p => p.status === 'pending')
      .sort((a, b) => a.submittedAt - b.submittedAt);
  }

  private sweepTimeouts(): void {
    const now = Date.now();
    for (const p of this.byId.values()) {
      if (p.status === 'pending' && now - p.submittedAt > TIMEOUT_MS) {
        p.status = 'timeout';
        p.decidedAt = now;
        emitSpine('card.proposal.timeout', p.role, { proposal_id: p.id, title: p.title });
      }
      // Drop anything older than 1h regardless of status — bounded memory.
      if (p.decidedAt && now - p.decidedAt > 60 * 60 * 1000) {
        this.byId.delete(p.id);
      }
    }
  }
}

/**
 * #2895 phase 2: file the approved proposal via chorus-api POST /api/cards/add.
 * X-Role: jeff means the spawned cards CLI sees DEPLOY_ROLE=jeff and bypasses
 * the proposal hook (no recursion).
 */
async function fileApprovedCard(p: CardProposal): Promise<{ cardId?: number; error?: string }> {
  // Tests must not hit the live chorus-api — would create real cards on the board.
  if (process.env.NODE_ENV === 'test') return { cardId: 99999 };
  const chorusApi = process.env.CHORUS_API_URL || 'http://localhost:3340';
  try {
    const res = await fetch(`${chorusApi}/api/cards/add`, {
      method: 'POST',
      headers: { 'content-type': 'application/json', 'X-Role': 'jeff' },
      body: JSON.stringify({
        title: p.title, owner: p.owner, priority: p.priority,
        domain: p.domain, type: p.type, origin: p.origin,
        desc: p.description, sequence: p.sequence,
      }),
    });
    const body = (await res.json()) as { ok?: boolean; stdout?: string; stderr?: string };
    if (!res.ok || !body.ok) {
      return { error: `chorus-api refused: ${body.stderr || res.statusText}` };
    }
    const match = (body.stdout || '').match(/Added #(\d+):/);
    return match ? { cardId: parseInt(match[1], 10) } : { error: `could not parse card_id from stdout: ${body.stdout}` };
  } catch (err) {
    return { error: err instanceof Error ? err.message : String(err) };
  }
}

function emitSpine(event: string, role: string, fields: Record<string, string | number>): void {
  try {
    const args = [event, role];
    for (const [k, v] of Object.entries(fields)) {
      args.push(`${k}=${String(v)}`);
    }
    execFileSync('/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chorus-log', args, {
      stdio: 'ignore',
      timeout: 2_000,
    });
  } catch { /* spine write is best-effort; do not block the proposal flow on log emit */ }
}

export const proposalStore = new ProposalStore();
