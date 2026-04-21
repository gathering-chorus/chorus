import * as fs from 'fs';
import * as path from 'path';
import { BoardConfig } from './types';

// Label IDs (shared across both boards)
// Updated 2026-04-07 after Vikunja DB rebuild
export const LABELS = {
  owner: { jeff: 1, wren: 2, silas: 3, kade: 4 } as Record<string, number>,
  priority: { P1: 5, P2: 6, P3: 7 } as Record<string, number>,
  domain: {
    // Legacy (keep for existing cards)
    health: 8, save: 9, make: 10,
    'house-garden': 11, gathering: 12, infrastructure: 59,
    // Registry-aligned domains
    seeds: 30, glimmers: 31, ideas: 32, projects: 33,
    cooking: 34, reading: 35, watching: 36, todo: 37, intentions: 38,
    music: 39, photos: 40, books: 41, stories: 42, notes: 43,
    blog: 44, social: 45, documents: 46, property: 47,
    sexuality: 48, people: 49, values: 50, practices: 65,
    self: 51, search: 52,
    // Chorus domains
    skills: 53, roles: 54, cards: 55, decisions: 56, briefs: 57, sessions: 58,
    // Convergence domain
    convergence: 60,
    // Product domains
    chorus: 61, borg: 62, pipelines: 139,
    // General domains
    app: 63, product: 64,
  } as Record<string, number>,
  product: {
    gathering: 14, chorus: 15,
  } as Record<string, number>,
  chunk: {
    spine: 17, ops: 18, memory: 19, music: 20,
    senses: 21, strategy: 22, app: 23, sexuality: 24, convergence: 89,
  } as Record<string, number>,
  sequence: {
    v1: 25, hardening: 26, style: 27, sparql: 28, 'flow-tests': 29,
    visibility: 71, gates: 72, spine: 73, ops: 74, strategy: 75,
    icd: 76, werk: 77, infrastructure: 78, clearing: 79, content: 80,
    framework: 81, awareness: 82, coordination: 83, surfaces: 84, loom: 85,
    quality: 86, athena: 137, convergence: 138,
    harness: 141, borg: 142, cards: 143, context: 144, protocol: 145, seeds: 146,
    pulse: 147,
  } as Record<string, number>,
  type: {
    new: 66, enhance: 67, fix: 68, chore: 69, swat: 70,
  } as Record<string, number>,
  origin: {
    reflective: 87, reactive: 88,
  } as Record<string, number>,
  stream: {
    sowing: 90, growing: 91, practicing: 92,
    harvesting: 93, reflecting: 94, connecting: 95,
  } as Record<string, number>,
};

export const GATHERING: BoardConfig = {
  name: 'gathering',
  projectId: 2,
  viewId: 8,
  buckets: {
    later: 4, next: 7, now: 8, wip: 5, blocked: 9, done: 6,
    'jeff-tickets': 10, 'tech-debt': 11, ops: 12, 'wont-do': 13,
    harvesting: 14, swat: 15, ideas: 16,
  },
  bucketNames: {
    4: 'Later', 7: 'Next', 8: 'Now', 5: 'WIP', 9: 'Blocked', 6: 'Done',
    10: 'Jeff Tickets', 11: 'Tech Debt', 12: 'Ops', 13: "Won't Do",
    14: 'Harvesting', 15: 'SWAT', 16: 'Ideas',
  },
};


export const SELF: BoardConfig = {
  name: 'self',
  projectId: 5,
  viewId: 21,
  buckets: {
    later: 24, now: 25, done: 26, next: 27, blocked: 28,
  },
  bucketNames: {
    24: 'Later', 25: 'Now', 26: 'Done', 27: 'Next', 28: 'Blocked',
  },
};

/** Resolve a status name to a bucket ID */
export function resolveBucket(board: BoardConfig, status: string): number {
  const raw = status.toLowerCase();
  // Alias map — exact match first, then substring aliases
  const aliases: Record<string, string> = {
    'in progress': 'now', 'inprogress': 'now', 'ip': 'now',
    'doing': 'now', 'ready': 'next', 'todo': 'later',
    'jeff tickets': 'jeff-tickets', 'tech debt': 'tech-debt',
    'jt': 'jeff-tickets', 'td': 'tech-debt', 'operations': 'ops',
    "won't do": 'wont-do', 'wontdo': 'wont-do', 'wd': 'wont-do',
    'not doing': 'wont-do', 'killed': 'wont-do', 'dup': 'wont-do',
    'harvest': 'harvesting', 'idea': 'ideas', 'parked': 'ideas',
  };
  const key = aliases[raw] ?? raw;
  const id = board.buckets[key];
  if (!id) {
    throw new Error(`Unknown status "${status}". Valid: ${Object.keys(board.buckets).join(', ')}`);
  }
  return id;
}

/** Load Vikunja config from env or .env file */
export function loadEnv(): { url: string; token: string } {
  if (process.env.VIKUNJA_URL && process.env.VIKUNJA_TOKEN) {
    return { url: process.env.VIKUNJA_URL, token: process.env.VIKUNJA_TOKEN };
  }

  const envPaths = [
    path.join(__dirname, '../../../../.env'),          // chorus/.env
    path.join(__dirname, '../../../../../chorus/.env'), // from project root
  ];

  for (const envPath of envPaths) {
    if (fs.existsSync(envPath)) {
      const content = fs.readFileSync(envPath, 'utf-8');
      const vars: Record<string, string> = {};
      for (const line of content.split('\n')) {
        const match = line.match(/^(\w+)=(.+)$/);
        if (match) vars[match[1]] = match[2].trim();
      }

      const url = vars['VIKUNJA_URL'] || 'http://localhost:3456';

      // Try role-specific token, then generic
      const role = detectRole().toUpperCase();
      const token = vars[`VIKUNJA_TOKEN_${role}`]
        || vars['VIKUNJA_TOKEN_KADE']
        || vars['VIKUNJA_TOKEN']
        || '';

      if (!token) throw new Error('No Vikunja token found in env or .env file');
      return { url, token };
    }
  }

  throw new Error('No .env file found and VIKUNJA_TOKEN not set');
}

/** Detect role from cwd */
export function detectRole(): string {
  const cwd = process.cwd();
  if (cwd.includes('engineer')) return 'kade';
  if (cwd.includes('product-manager')) return 'wren';
  if (cwd.includes('architect')) return 'silas';
  return 'wren';
}
