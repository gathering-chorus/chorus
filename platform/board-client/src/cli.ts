#!/usr/bin/env node
/**
 * Cards CLI — thin dispatch layer over board-sdk.
 *
 * Usage:
 *   board [--self] list
 *   board [--self] add "title" [--status S] [--owner O] [--priority P] [--domain D]
 *   board [--self] move <id> <status>
 *   board [--self] done <id>
 *   board [--self] block <id> "reason"
 *   board [--self] unblock <id>
 *   board [--self] mine [role]
 *   board [--self] now [role]
 *   board [--self] view <id>
 *   board [--self] update <id> [--title T] [--description D] [--domain D] [--chunk C] [--seq S] [--owner O]
 *   board [--self] reassign <id> <role>
 *   board [--self] comment <id> "text"
 *   board [--self] tag <id> <chunk>
 *   board [--self] chunk [name]
 *   board [--self] sequence [name]
 *   board [--self] sequence-tag <ids...> <sequence>
 *   board [--self] buckets
 *   board [--self] set-limit <bucket> <number>
 *   board [--self] snapshot
 *   board [--self] audit-start <role>
 *   board [--self] audit-close <role>
 */

import * as fs from 'fs';
import * as path from 'path';
import { BoardClient } from './client';
import { GATHERING, SELF, LABELS, loadEnv, detectRole } from './config';
import { BoardConfig } from './types';
import { emitSpineEvent } from './events';
import {
  addCard, moveCard, doneCard, demoCard, rejectCard,
  blockCard, unblockCard, updateCard, commentCard, tagCard, untagCard,
  reassignCard, setCard, swatCard, snapshotBoard, auditStart, auditClose,
  bulkSequenceTag,
} from './sdk';

function die(msg: string): never {
  console.error(`ERROR: ${msg}`);
  process.exit(1);
}

function parseGlobalFlags(args: string[]): {
  boardSelection: 'gathering' | 'self';
  productFilter?: 'gathering' | 'chorus';
  rest: string[];
} {
  let boardSelection: 'gathering' | 'self' = 'gathering';
  let productFilter: 'gathering' | 'chorus' | undefined;
  const rest: string[] = [];

  for (let i = 0; i < args.length; i++) {
    const arg = args[i];
    if (arg === '--self' || arg === '-s') {
      boardSelection = 'self';
    } else if (arg === '--product' || arg === '-p') {
      const val = args[++i]?.toLowerCase();
      if (val !== 'gathering' && val !== 'chorus') {
        die('--product must be "gathering" or "chorus"');
      }
      productFilter = val as 'gathering' | 'chorus';
    } else {
      rest.push(arg);
    }
  }

  return { boardSelection, productFilter, rest };
}

function parseAddArgs(args: string[]): {
  title: string; status: string; owner: string; priority: string;
  domain: string; description: string; product: string; chunk: string; sequence: string;
  quick: boolean;
} {
  let title = '', status = 'later', owner = '', priority = '';
  let domain = '', description = '', product = '', chunk = '', sequence = '';
  let quick = false;

  let i = 0;
  while (i < args.length) {
    switch (args[i]) {
      case '--status': status = args[++i]; break;
      case '--owner': owner = args[++i]; break;
      case '--priority': priority = args[++i]; break;
      case '--domain': domain = args[++i]; break;
      case '--description': case '--desc': description = args[++i]; break;
      case '--product': case '-p': product = args[++i]; break;
      case '--chunk': chunk = args[++i]; break;
      case '--sequence': case '--seq': sequence = args[++i]; break;
      case '--quick': case '-q': quick = true; break;
      default:
        if (!title) title = args[i];
        else die(`Unexpected argument: ${args[i]}`);
    }
    i++;
  }

  if (!title) die('Usage: cards add "title" [--status S] [--owner O] [--priority P] [--domain D] [--product P] [--chunk C] [--sequence S] [--desc D] [--quick]');
  return { title, status, owner, priority, domain, description, product, chunk, sequence, quick };
}

function parseUpdateArgs(args: string[]): { index: number; title?: string; description?: string; domain?: string; chunk?: string; sequence?: string; owner?: string } {
  if (!args[0]) die('Usage: cards update <id> [--title T] [--desc D] [--domain D] [--chunk C] [--owner O]');
  const index = parseInt(args[0], 10);
  let title: string | undefined;
  let description: string | undefined;
  let domain: string | undefined;
  let chunk: string | undefined;
  let sequence: string | undefined;
  let owner: string | undefined;

  let i = 1;
  while (i < args.length) {
    switch (args[i]) {
      case '--title': title = args[++i]; break;
      case '--description': case '--desc': description = args[++i]; break;
      case '--domain': domain = args[++i]; break;
      case '--chunk': chunk = args[++i]; break;
      case '--sequence': case '--seq': sequence = args[++i]; break;
      case '--owner': owner = args[++i]; break;
      default: die(`Unexpected argument: ${args[i]}`);
    }
    i++;
  }

  if (!title && description === undefined && !domain && !chunk && !sequence && !owner) die('Provide --title, --desc, --domain, --chunk, --seq, and/or --owner');
  return { index, title, description, domain, chunk, sequence, owner };
}

// ── Display-only commands (no business logic, just formatting) ──

async function cmdList(client: BoardClient, label: string, productFilter?: string) {
  const grouped = await client.listGrouped();
  const order = ['Now', 'WIP', 'SWAT', 'Harvesting', 'Blocked', 'Next', 'Later', 'Jeff Tickets', 'Tech Debt', "Won't Do", 'Done'];

  for (const status of order) {
    let tasks = grouped.get(status);
    if (!tasks || tasks.length === 0) continue;

    if (productFilter) {
      tasks = tasks.filter(t => {
        const productLabel = `product:${productFilter}`;
        if (productFilter === 'chorus') return t.domains.includes(productLabel);
        return !t.domains.includes('product:chorus') || t.domains.includes(productLabel);
      });
      if (tasks.length === 0) continue;
    }

    console.log(`\n${status} (${tasks.length}):`);
    for (const t of tasks) {
      const productTag = t.domains.includes('product:chorus') ? 'chorus' : '';
      const nonProductDomains = t.domains.filter(d => !d.startsWith('product:'));
      const tags = [t.owner, t.priority, productTag, ...nonProductDomains].filter(Boolean);
      const tagStr = tags.length > 0 ? ` [${tags.join('|')}]` : '';
      console.log(`  ${String(t.index).padStart(4)}  ${t.title}${tagStr}`);
    }
  }
}

async function cmdMine(client: BoardClient, args: string[], label: string) {
  const role = args[0] || detectRole();
  const tasks = await client.mine(role);
  if (tasks.length === 0) {
    console.log(`No ${label} items assigned to ${role}`);
    return;
  }
  console.log(`${role.charAt(0).toUpperCase() + role.slice(1)} ${label} items (${tasks.length}):`);
  for (const t of tasks) {
    console.log(`  [${t.status}] ${String(t.index).padStart(4)}  ${t.title}${t.priority ? ` [${t.priority}]` : ''}`);
  }
}

async function cmdView(client: BoardClient, args: string[]) {
  if (!args[0]) die('Usage: cards view <id>');
  const index = parseInt(args[0], 10);
  const task = await client.view(index);
  console.log(`#${task.index} ${task.title}`);
  console.log(`  Status:   ${task.status}`);
  console.log(`  Owner:    ${task.owner || 'unassigned'}`);
  console.log(`  Priority: ${task.priority || 'none'}`);
  if (task.description) console.log(`  Desc:\n${task.description.split('\n').map(l => `    ${l}`).join('\n')}`);
  if (task.domains.length) console.log(`  Domains:  ${task.domains.join(', ')}`);
  console.log(`  Created:  ${task.created}`);
  console.log(`  Updated:  ${task.updated}`);

  // Domain Radius — show domain context alongside blast radius (#1688)
  const domainTags = task.domains.filter((d: string) => d.startsWith('domain:')).map((d: string) => d.replace('domain:', ''));
  if (domainTags.length > 0) {
    const fs = await import('fs');
    const path = await import('path');
    const contextDir = path.join(__dirname, '..', '..', 'domain-context');
    const found: string[] = [];
    const missing: string[] = [];
    for (const domain of domainTags) {
      const filePath = path.join(contextDir, `domain-context-${domain}.md`);
      if (fs.existsSync(filePath)) {
        found.push(domain);
        const content = fs.readFileSync(filePath, 'utf-8');
        const lines = content.split('\n');
        // Extract constraints section
        let inConstraints = false;
        const constraints: string[] = [];
        for (const line of lines) {
          if (/^##\s+Constraints/i.test(line)) { inConstraints = true; continue; }
          if (inConstraints && /^##\s/.test(line)) break;
          if (inConstraints && line.trim().startsWith('-')) constraints.push(line.trim());
        }
        // Get last modified
        const stat = fs.statSync(filePath);
        const age = Math.round((Date.now() - stat.mtimeMs) / 3600000);
        const ageStr = age < 1 ? '<1h ago' : age < 24 ? `${age}h ago` : `${Math.round(age / 24)}d ago`;
        console.log(`\n**Domain Radius** (${domain}):`);
        console.log(`  ${filePath.replace(/.*domain-context\//, 'domain-context/')}`);
        console.log(`  Updated: ${ageStr}`);
        if (constraints.length > 0) {
          console.log(`  Constraints (${constraints.length}):`);
          constraints.slice(0, 5).forEach(c => console.log(`    ${c}`));
          if (constraints.length > 5) console.log(`    ... +${constraints.length - 5} more`);
        }
      } else {
        missing.push(domain);
      }
    }
    if (missing.length > 0) {
      console.log(`\n**Domain Radius** — missing context: ${missing.map(d => `domain-context-${d}.md`).join(', ')}`);
    }
  }

  const comments = await client.comments(index);
  if (comments.length > 0) {
    console.log(`  Comments (${comments.length}):`);
    for (const c of comments) {
      console.log(`    [${c.author}] ${c.text}`);
    }
  }
}

async function cmdNow(client: BoardClient, args: string[], label: string) {
  const role = args[0] || detectRole();
  const tasks = await client.now(role);
  if (tasks.length === 0) {
    console.log(`No ${label} items in Now for ${role}`);
    return;
  }
  console.log(`${role.charAt(0).toUpperCase() + role.slice(1)} — Now (${tasks.length}):`);
  for (const t of tasks) {
    console.log(`  #${String(t.index).padStart(4)}  ${t.title}${t.priority ? ` [${t.priority}]` : ''}`);
  }
}

async function cmdBuckets(client: BoardClient) {
  const buckets = await client.fetchBucketsWithLimits();
  for (const b of buckets) {
    const limitStr = b.limit > 0 ? `limit: ${b.limit}` : 'no limit';
    console.log(`  ${b.title.padEnd(14)} ${String(b.taskCount).padStart(3)} tasks  (${limitStr})`);
  }
}

async function cmdSetLimit(client: BoardClient, args: string[], boardConfig: BoardConfig) {
  if (!args[0] || args[1] === undefined) die('Usage: cards set-limit <bucket> <number>');
  const bucketName = args[0].toLowerCase();
  const limit = parseInt(args[1], 10);
  if (isNaN(limit) || limit < 0) die('Limit must be a non-negative number (0 = no limit)');

  const bucketId = boardConfig.buckets[bucketName];
  if (!bucketId) {
    die(`Unknown bucket "${args[0]}". Valid: ${Object.keys(boardConfig.buckets).join(', ')}`);
  }

  await client.setBucketLimit(bucketId, limit);
  const displayName = boardConfig.bucketNames[bucketId] || args[0];
  console.log(`Set ${displayName} limit to ${limit === 0 ? 'none' : limit}`);
  emitSpineEvent('board.limit.set', detectRole(), {
    bucket: displayName, limit: String(limit), board: client.boardName,
  });
}

const CHUNKS_DIR = path.join(__dirname, '../../../product-manager/chunks');

async function cmdChunk(client: BoardClient, args: string[]) {
  const validChunks = ['spine', 'ops', 'memory', 'music', 'senses', 'strategy', 'app', 'sexuality', 'convergence'];
  const chunk = args[0]?.toLowerCase();

  if (!chunk || !validChunks.includes(chunk)) {
    const all = await client.list();
    const activeBuckets = ['Now', 'WIP', 'SWAT', 'Harvesting', 'Next', 'Later', 'Blocked'];
    console.log('Chunks:');
    for (const c of validChunks) {
      const cards = all.filter(t => activeBuckets.includes(t.status) && t.domains.includes(`chunk:${c}`));
      const contextFile = path.join(CHUNKS_DIR, `${c}.md`);
      const hasContext = fs.existsSync(contextFile) ? '+' : ' ';
      console.log(`  ${hasContext} ${c.padEnd(10)} ${String(cards.length).padStart(2)} cards`);
    }
    const untagged = all.filter(t => activeBuckets.includes(t.status) && !t.domains.some(d => d.startsWith('chunk:')));
    if (untagged.length > 0) {
      console.log(`    ${'untagged'.padEnd(10)} ${String(untagged.length).padStart(2)} cards`);
    }
    return;
  }

  const contextFile = path.join(CHUNKS_DIR, `${chunk}.md`);
  if (fs.existsSync(contextFile)) {
    const content = fs.readFileSync(contextFile, 'utf-8');
    const lines = content.split('\n');
    let headerCount = 0;
    const summary: string[] = [];
    for (const line of lines) {
      if (line.startsWith('# ')) { summary.push(line); continue; }
      if (line.startsWith('## ')) {
        headerCount++;
        if (headerCount > 2) break;
      }
      summary.push(line);
    }
    console.log(summary.join('\n'));
    console.log(`\n  (full context: product-manager/chunks/${chunk}.md)\n`);
  } else {
    console.log(`\n  No context doc yet for chunk:${chunk}. Create: product-manager/chunks/${chunk}.md\n`);
  }

  const all = await client.list();
  const activeBuckets = ['Now', 'WIP', 'Next', 'Later', 'Blocked'];
  const cards = all.filter(t => activeBuckets.includes(t.status) && t.domains.includes(`chunk:${chunk}`));

  if (cards.length === 0) {
    console.log(`No active cards tagged chunk:${chunk}`);
    return;
  }

  const byStatus = new Map<string, typeof cards>();
  for (const c of cards) {
    const list = byStatus.get(c.status) || [];
    list.push(c);
    byStatus.set(c.status, list);
  }

  const order = ['Now', 'WIP', 'Blocked', 'Next', 'Later'];
  for (const status of order) {
    const group = byStatus.get(status);
    if (!group) continue;
    console.log(`${status} (${group.length}):`);
    for (const t of group) {
      const tags = [t.owner, t.priority].filter(Boolean);
      const tagStr = tags.length > 0 ? ` [${tags.join('|')}]` : '';
      console.log(`  ${String(t.index).padStart(4)}  ${t.title}${tagStr}`);
    }
  }
}

async function cmdDomain(client: BoardClient, args: string[]) {
  const validDomains = Object.keys(LABELS.domain);
  const sub = args[0]?.toLowerCase();

  if (sub === 'add') {
    const name = args[1]?.toLowerCase();
    if (!name) die('Usage: cards domain add <name>');
    if (LABELS.domain[name]) {
      console.log(`Domain "${name}" already exists (label ID ${LABELS.domain[name]})`);
      return;
    }
    // Create label via Vikunja API
    const label = await client.createLabel(`domain:${name}`);
    console.log(`Created domain "${name}" (label ID ${label.id})`);
    console.log(`⚠ Add to config.ts LABELS.domain: ${name}: ${label.id}`);
    return;
  }

  if (sub === 'remove') {
    const name = args[1]?.toLowerCase();
    if (!name) die('Usage: cards domain remove <name>');
    // Check config.ts first, then Vikunja labels
    let labelId = LABELS.domain[name];
    if (!labelId) {
      // Not in config — check Vikunja directly (covers domains added via `domain add`)
      const labels = await client.listLabels();
      const match = labels.find(l => l.title === `domain:${name}`);
      if (!match) die(`Domain "${name}" not found in config.ts or Vikunja labels.`);
      labelId = match.id;
    }
    console.log(`Domain "${name}" has label ID ${labelId}`);
    // Check if any cards use this domain before deleting
    const all = await client.list();
    const using = all.filter(t => t.domains.includes(`domain:${name}`));
    if (using.length > 0) {
      console.log(`⚠ ${using.length} card(s) use domain:${name} — remove from cards first`);
      for (const c of using.slice(0, 5)) {
        console.log(`  #${c.apiId} ${c.title.substring(0, 60)}`);
      }
      return;
    }
    try {
      await client.deleteLabel(labelId);
      console.log(`Deleted domain "${name}" (label ID ${labelId}) from Vikunja`);
    } catch (e: any) {
      if (e.message?.includes('401')) {
        console.log(`⚠ Cannot delete label ${labelId} — requires creator's token. Delete manually in Vikunja UI.`);
      } else {
        throw e;
      }
    }
    if (LABELS.domain[name]) {
      console.log(`⚠ Also remove from config.ts LABELS.domain and recompile`);
    }
    return;
  }

  // Default: list domains with card counts
  const all = await client.list();
  const activeBuckets = ['Now', 'WIP', 'SWAT', 'Harvesting', 'Next', 'Later', 'Blocked'];
  console.log('Domains:');
  for (const d of validDomains.sort()) {
    const cards = all.filter(t => activeBuckets.includes(t.status) && t.domains.includes(`domain:${d}`));
    if (cards.length > 0 || sub === 'all') {
      console.log(`  ${d.padEnd(16)} ${String(cards.length).padStart(3)} cards`);
    }
  }
  const untagged = all.filter(t => activeBuckets.includes(t.status) && !t.domains.some(d => d.startsWith('domain:')));
  if (untagged.length > 0) {
    console.log(`  ${'(no domain)'.padEnd(16)} ${String(untagged.length).padStart(3)} cards`);
  }
}

async function cmdSequence(client: BoardClient, args: string[]) {
  const validSequences = Object.keys(LABELS.sequence);
  const seq = args[0]?.toLowerCase();

  if (!seq || !validSequences.includes(seq)) {
    const all = await client.list();
    const activeBuckets = ['Now', 'WIP', 'SWAT', 'Harvesting', 'Next', 'Later', 'Blocked'];
    console.log('Sequences:');
    for (const s of validSequences) {
      const cards = all.filter(t => activeBuckets.includes(t.status) && t.domains.includes(`sequence:${s}`));
      console.log(`  ${s.padEnd(14)} ${String(cards.length).padStart(3)} cards`);
    }
    const untagged = all.filter(t => activeBuckets.includes(t.status) && !t.domains.some(d => d.startsWith('sequence:')));
    if (untagged.length > 0) {
      console.log(`  ${'untagged'.padEnd(14)} ${String(untagged.length).padStart(3)} cards`);
    }
    return;
  }

  const all = await client.list();
  const activeBuckets = ['Now', 'WIP', 'Next', 'Later', 'Blocked'];
  const cards = all.filter(t => activeBuckets.includes(t.status) && t.domains.includes(`sequence:${seq}`));

  if (cards.length === 0) {
    console.log(`No active cards tagged sequence:${seq}`);
    return;
  }

  const byStatus = new Map<string, typeof cards>();
  for (const c of cards) {
    const list = byStatus.get(c.status) || [];
    list.push(c);
    byStatus.set(c.status, list);
  }

  console.log(`\nsequence:${seq} (${cards.length} active):`);
  const order = ['Now', 'WIP', 'Blocked', 'Next', 'Later'];
  for (const status of order) {
    const group = byStatus.get(status);
    if (!group) continue;
    console.log(`${status} (${group.length}):`);
    for (const t of group) {
      const tags = [t.owner, t.priority].filter(Boolean);
      const tagStr = tags.length > 0 ? ` [${tags.join('|')}]` : '';
      console.log(`  ${String(t.index).padStart(4)}  ${t.title}${tagStr}`);
    }
  }

  // Also show Done cards for this sequence
  const doneCards = (await client.list()).filter(t => t.status === 'Done' && t.domains.includes(`sequence:${seq}`));
  if (doneCards.length > 0) {
    console.log(`Done (${doneCards.length}):`);
    for (const t of doneCards.slice(0, 10)) {
      console.log(`  ${String(t.index).padStart(4)}  ${t.title}`);
    }
    if (doneCards.length > 10) {
      console.log(`  ... and ${doneCards.length - 10} more`);
    }
  }
}

function cmdFields(board: BoardConfig) {
  const buckets = Object.keys(board.buckets).join(', ');
  console.log(`Board:    ${board.name}`);
  console.log(`Statuses: ${buckets}`);
  console.log(`Owners:   Jeff, Wren, Silas, Kade`);
  console.log(`Priority: P1, P2, P3`);
  if (board.name === 'gathering') {
    console.log(`Domains:  ${Object.keys(LABELS.domain).join(', ')}`);
    console.log(`Streams:  ${Object.keys(LABELS.stream).join(', ')}`);
    console.log(`Chunks:   ${Object.keys(LABELS.chunk).join(', ')}`);
    console.log(`Sequences: ${Object.keys(LABELS.sequence).join(', ')}`);
  }
}

function printUsage() {
  console.log(`Cards CLI — unified kanban board

Usage: cards [--self] <command> [args]

Commands:
  list                           Show all tasks by status
  add "title" [options]          Create a task
  move <id> <status>             Change task status (incl. "won't do" or "wd")
  done <id>                      Mark as Done (emits card.accepted)
  demo <id>                      Log demo started (DEC-048 Proving gate)
  reject <id> "reason"           Log rejection with reason
  block <id> "reason"            Block with reason
  unblock <id>                   Unblock → Next
  mine [role]                    Show role's tasks
  now [role]                     Show role's cards in Now
  view <id>                      Full task details
  update <id> [--title T] [--desc D] [--domain D] [--chunk C] [--seq S] [--owner O]  Update task fields + metadata
  reassign <id> <role>            Change card owner (wren/silas/kade/jeff)
  comment <id> "text"            Add a comment
  tag <id> <chunk>               Tag card with chunk label
  chunk [name]                   Show chunk context + cards (no arg = summary of all chunks)
  sequence [name]                Show sequence cards (no arg = summary of all sequences)
  sequence-tag <ids> <seq>       Bulk-tag cards with a sequence (comma-separated IDs)
  swat "description"             Create [swat]-tagged card in SWAT lane (outside WIP limit)
  buckets                        Show buckets with WIP limits
  set-limit <bucket> <number>    Set WIP limit (0 = none)
  snapshot                       Save board state
  audit-start <role>             Session start board check
  audit-close <role>             Session close board diff
  fields                         Show available statuses/labels

Options:
  --self, -s                     Target Self board
  --product P, -p P              Filter by product (gathering|chorus)
  --status S                     Initial status (Now/Next/Later/Done/Blocked/Harvesting/SWAT/Won't Do)
  --owner O                      Owner (Wren/Silas/Kade/Jeff)
  --priority P                   Priority (P1/P2/P3)
  --domain D                     Domain label (gathering/infrastructure/...)
  --chunk C                      Chunk label (spine/ops/memory/music/senses/strategy/app)
  --sequence S, --seq S          Sequence label (${Object.keys(LABELS.sequence).join('/')})
  --desc D                       Description text (required — must include AC)
  --quick, -q                    Skip AC requirement (unplanned issues/quick fixes only)`);
}

// ── Main dispatch ──

async function main() {
  const args = process.argv.slice(2);
  if (args.length === 0) { printUsage(); return; }

  const { boardSelection, productFilter, rest } = parseGlobalFlags(args);
  const { url, token } = loadEnv();
  const boardConfig = boardSelection === 'self' ? SELF : GATHERING;
  const client = new BoardClient(url, token, boardConfig);
  const boardLabel = boardSelection === 'self' ? 'Self' : 'Gathering';

  const cmd = rest[0]?.toLowerCase();
  const cmdArgs = rest.slice(1);

  switch (cmd) {
    case 'list': await cmdList(client, boardLabel, productFilter); break;

    case 'add': {
      const opts = parseAddArgs(cmdArgs);
      if (productFilter && !opts.product) opts.product = productFilter;
      await addCard(client, opts.title, opts);
      break;
    }

    case 'move': {
      if (!cmdArgs[0] || !cmdArgs[1]) die('Usage: cards move <id> <status>');
      await moveCard(client, parseInt(cmdArgs[0], 10), cmdArgs[1]);
      break;
    }

    case 'done': {
      if (!cmdArgs[0]) die('Usage: cards done <id>');
      await doneCard(client, parseInt(cmdArgs[0], 10));
      break;
    }

    case 'demo': {
      if (!cmdArgs[0]) die('Usage: cards demo <id>');
      await demoCard(client, parseInt(cmdArgs[0], 10));
      break;
    }

    case 'reject': {
      if (!cmdArgs[0]) die('Usage: cards reject <id> "reason"');
      await rejectCard(client, parseInt(cmdArgs[0], 10), cmdArgs.slice(1).join(' ') || 'no reason given');
      break;
    }

    case 'block': {
      if (!cmdArgs[0]) die('Usage: cards block <id> "reason"');
      await blockCard(client, parseInt(cmdArgs[0], 10), cmdArgs.slice(1).join(' '));
      break;
    }

    case 'unblock': {
      if (!cmdArgs[0]) die('Usage: cards unblock <id>');
      await unblockCard(client, parseInt(cmdArgs[0], 10));
      break;
    }

    case 'mine': await cmdMine(client, cmdArgs, boardLabel); break;
    case 'view': await cmdView(client, cmdArgs); break;
    case 'now': await cmdNow(client, cmdArgs, boardLabel); break;

    case 'set': {
      if (cmdArgs.length < 2) die('Usage: cards set <id> key=value [key=value ...]');
      const setId = parseInt(cmdArgs[0], 10);
      if (isNaN(setId)) die(`Invalid card ID: ${cmdArgs[0]}`);
      const pairs: Record<string, string> = {};
      for (let i = 1; i < cmdArgs.length; i++) {
        const arg = cmdArgs[i];
        const eq = arg.indexOf('=');
        if (eq === -1) die(`Bare value "${arg}" — specify category: domain=${arg}, chunk=${arg}, or sequence=${arg}`);
        const key = arg.substring(0, eq).toLowerCase();
        const val = arg.substring(eq + 1);
        if (!val) die(`Empty value for key "${key}"`);
        pairs[key] = val;
      }
      await setCard(client, setId, pairs);
      break;
    }

    case 'update':
      die('Removed: use "board set <id> key=value" instead. Example: board set 1633 domain=photos chunk=memory');
      break;

    case 'comment': {
      if (!cmdArgs[0] || !cmdArgs[1]) die('Usage: cards comment <id> "text"');
      await commentCard(client, parseInt(cmdArgs[0], 10), cmdArgs.slice(1).join(' '));
      break;
    }

    case 'reassign': {
      if (!cmdArgs[0] || !cmdArgs[1]) die('Usage: cards reassign <id> <role>');
      await reassignCard(client, parseInt(cmdArgs[0], 10), cmdArgs[1]);
      break;
    }

    case 'tag':
      die('Removed: use "board set <id> domain=X" or "board set <id> chunk=X" instead');
      break;

    case 'untag':
      die('Removed: use "board set <id> domain=none" to remove tags');
      break;

    case 'deps': {
      if (!cmdArgs[0]) die('Usage: cards deps <id>');
      const depsId = parseInt(cmdArgs[0], 10);
      const rels = await client.getRelations(depsId);
      const card = await client.view(depsId);
      console.log(`#${depsId} ${card.title}`);
      if (rels.blockedBy.length) console.log(`  After:  ${rels.blockedBy.map(id => `#${id}`).join(', ')}`);
      else console.log(`  After:  (none)`);
      if (rels.blocks.length) console.log(`  Gates:  ${rels.blocks.map(id => `#${id}`).join(', ')}`);
      else console.log(`  Gates:  (none)`);
      break;
    }

    case 'blocked': {
      const all = await client.fetchAllTasks();
      let found = false;
      for (const task of all) {
        const related = (task as any).related_tasks?.blocked || [];
        if (related.length > 0) {
          const doneCount = related.filter((r: any) => r.done).length;
          if (doneCount < related.length) {
            found = true;
            const map = await client.buildTaskMap();
            const revMap = new Map<number, number>();
            for (const [di, ai] of map) revMap.set(ai, di);
            const displayId = revMap.get(task.id) || task.id;
            const blockers = related.map((r: any) => `#${revMap.get(r.id) || r.id}${r.done ? '✓' : ''}`).join(', ');
            console.log(`#${displayId} ${task.title?.substring(0, 60)} — blocked by: ${blockers}`);
          }
        }
      }
      if (!found) console.log('No blocked cards');
      break;
    }

    case 'ready': {
      const allTasks = await client.fetchAllTasks();
      let found = false;
      for (const task of allTasks) {
        const related = (task as any).related_tasks?.blocked || [];
        if (related.length > 0) {
          const allDone = related.every((r: any) => r.done);
          if (allDone && !task.done) {
            found = true;
            const map = await client.buildTaskMap();
            const revMap = new Map<number, number>();
            for (const [di, ai] of map) revMap.set(ai, di);
            const displayId = revMap.get(task.id) || task.id;
            console.log(`#${displayId} ${task.title?.substring(0, 60)} — all deps done, ready to pull`);
          }
        }
      }
      if (!found) console.log('No cards with completed dependencies waiting');
      break;
    }

    case 'chain': {
      if (!cmdArgs[0]) die('Usage: cards chain <id>');
      const chainRoot = parseInt(cmdArgs[0], 10);
      // Walk the full dependency chain (transitive)
      const visited = new Set<number>();
      const chain: Array<{ id: number; title: string; status: string; depth: number; blockedBy: number[] }> = [];

      async function walkChain(id: number, depth: number) {
        if (visited.has(id)) return;
        visited.add(id);
        try {
          const card = await client.view(id);
          const rels = await client.getRelations(id);
          chain.push({ id, title: card.title, status: card.status, depth, blockedBy: rels.blockedBy });
          // Walk downstream (what this card gates)
          for (const gated of rels.blocks) {
            await walkChain(gated, depth + 1);
          }
        } catch { /* card not found — skip */ }
      }

      // First walk upstream to find the chain root
      let root = chainRoot;
      const upVisited = new Set<number>();
      while (true) {
        if (upVisited.has(root)) break;
        upVisited.add(root);
        try {
          const rels = await client.getRelations(root);
          if (rels.blockedBy.length > 0) { root = rels.blockedBy[0]; }
          else break;
        } catch { break; }
      }

      // Walk downstream from root
      await walkChain(root, 0);

      // Sort by depth then id
      chain.sort((a, b) => a.depth - b.depth || a.id - b.id);

      if (chain.length === 0) {
        console.log(`#${chainRoot} has no dependency chain`);
      } else {
        console.log(`Chain from #${root}:`);
        for (const c of chain) {
          const icon = c.status === 'Done' ? '✅' : c.status === 'WIP' ? '🔨' : c.status === "Won't Do" ? '⏭️' : '⬜';
          const indent = '  '.repeat(c.depth);
          const deps = c.blockedBy.length ? ` (after ${c.blockedBy.map(d => `#${d}`).join(', ')})` : '';
          console.log(`${indent}${icon} #${c.id} ${c.title.substring(0, 60)}${deps}`);
          if (c.status === "Won't Do") {
            console.log(`${indent}  ⚠ Won't Do — chain gap. Deliberate or drift?`);
          }
        }
        const done = chain.filter(c => c.status === 'Done').length;
        const total = chain.length;
        const wontDo = chain.filter(c => c.status === "Won't Do").length;
        console.log(`\n${done}/${total} complete${wontDo ? ` (${wontDo} Won't Do — gaps)` : ''}`);
      }
      break;
    }

    case 'chunk': await cmdChunk(client, cmdArgs); break;
    case 'domain': await cmdDomain(client, cmdArgs); break;

    case 'sequence': await cmdSequence(client, cmdArgs); break;

    case 'sequence-tag': {
      if (cmdArgs.length < 2) die('Usage: cards sequence-tag <id>[,<id>,...] <sequence>');
      const seqName = cmdArgs[cmdArgs.length - 1];
      const ids = cmdArgs.slice(0, -1).join(',').split(',').map(s => parseInt(s.trim(), 10)).filter(n => !isNaN(n));
      if (ids.length === 0) die('No valid card IDs provided');
      await bulkSequenceTag(client, ids, seqName);
      break;
    }

    case 'swat': {
      if (!cmdArgs[0]) die('Usage: cards swat "description of urgent issue"');
      await swatCard(client, cmdArgs[0]);
      break;
    }

    case 'snapshot': await snapshotBoard(client); break;
    case 'audit-start': await auditStart(client, cmdArgs[0] || detectRole()); break;
    case 'audit-close': await auditClose(client, cmdArgs[0] || detectRole()); break;
    case 'buckets': await cmdBuckets(client); break;
    case 'set-limit': await cmdSetLimit(client, cmdArgs, boardConfig); break;
    case 'fields': cmdFields(boardConfig); break;
    case 'help': case '--help': case '-h': printUsage(); break;
    default: die(`Unknown command: ${cmd}`);
  }
}

main().catch(err => {
  console.error(`ERROR: ${err.message || err}`);
  process.exit(1);
});
