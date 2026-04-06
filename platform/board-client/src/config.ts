import * as fs from 'fs';
import * as path from 'path';
import { BoardConfig } from './types';

// Label IDs (shared across both boards)
export const LABELS = {
  owner: { jeff: 1, wren: 2, silas: 3, kade: 4 } as Record<string, number>,
  priority: { P1: 5, P2: 6, P3: 7 } as Record<string, number>,
  domain: {
    // Legacy (keep for existing cards)
    health: 8, save: 9, make: 10,
    'house-garden': 11, gathering: 12, infrastructure: 72,
    // Registry-aligned domains
    seeds: 30, glimmers: 31, ideas: 32, projects: 33,
    cooking: 34, reading: 35, watching: 36, todo: 37, intentions: 38,
    music: 39, photos: 40, books: 41, stories: 42, notes: 43,
    blog: 44, social: 45, documents: 46, property: 47,
    sexuality: 48, people: 49, values: 50, practices: 51,
    self: 52, search: 53,
    // Chorus domains
    skills: 66, roles: 67, cards: 68, decisions: 69, briefs: 70, sessions: 71,
    // Convergence domain — ICD ontology, RDF migration, API, test automation, namespace governance
    convergence: 73,
    // Product domains
    chorus: 75, borg: 76,
    // General domains
    app: 102, product: 103,
    // Note: infrastructure moved from legacy ID 13 to domain:infrastructure ID 72
  } as Record<string, number>,
  product: {
    gathering: 14, chorus: 15,
  } as Record<string, number>,
  chunk: {
    spine: 17, ops: 18, memory: 19, music: 20,
    senses: 21, strategy: 22, app: 23, sexuality: 24, convergence: 42,
  } as Record<string, number>,
  sequence: {
    v1: 25, hardening: 26, style: 27, sparql: 28, 'flow-tests': 29,
    visibility: 92, gates: 93, spine: 94, ops: 95, strategy: 96,
    icd: 97, werk: 98, infrastructure: 99, clearing: 100, content: 101,
    framework: 104, awareness: 105, coordination: 106, surfaces: 107, loom: 108,
    quality: 111,
  } as Record<string, number>,
  type: {
    new: 87, enhance: 88, fix: 89, chore: 90, swat: 91,
  } as Record<string, number>,
  origin: {
    reflective: 109, reactive: 110,
  } as Record<string, number>,
  stream: {
    sowing: 54, growing: 55, practicing: 56,
    harvesting: 57, reflecting: 58, connecting: 59,
  } as Record<string, number>,
};

export const GATHERING: BoardConfig = {
  name: 'gathering',
  projectId: 2,
  viewId: 8,
  buckets: {
    later: 4, next: 7, now: 5, wip: 29, blocked: 8, done: 6,
    'jeff-tickets': 12, 'tech-debt': 13, ops: 30, 'wont-do': 31,
    harvesting: 33, swat: 34, ideas: 35,
  },
  bucketNames: {
    4: 'Later', 7: 'Next', 5: 'Now', 29: 'WIP', 8: 'Blocked', 6: 'Done',
    12: 'Jeff Tickets', 13: 'Tech Debt', 30: 'Ops', 31: "Won't Do",
    33: 'Harvesting', 34: 'SWAT', 35: 'Ideas',
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
    path.join(__dirname, '../../../.env'),          // chorus/.env
    path.join(__dirname, '../../../../chorus/.env'), // from project root
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
