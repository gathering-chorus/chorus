import * as fs from 'fs';
import * as path from 'path';
import { RoleConfig } from './config';
import { PodBriefClient } from './pod-brief-client';
import { log } from './logger';

/**
 * High-level facade for brief I/O.
 *
 * Design: filesystem-always + pod-best-effort.
 * - writeBrief: writes to filesystem (must succeed) AND pod (best-effort).
 * - readBrief: reads from filesystem (primary source).
 * - listBriefs: lists from filesystem (primary source).
 *
 * This is the ONLY module other code should import for brief operations.
 */

let podClient: PodBriefClient | null = null;

/**
 * Initialize the pod client. Call once at bridge startup.
 * If env vars are missing, pod writes are silently skipped.
 */
export function initPodBriefClient(): void {
  const baseUrl = process.env.APP_BASE_URL;
  const secret = process.env.BRIDGE_AGENT_SECRET;

  if (!baseUrl || !secret) {
    log('info', 'Pod brief client not configured — filesystem-only mode', {
      event: 'pod_client_skip',
      hasBaseUrl: !!baseUrl,
      hasSecret: !!secret,
    });
    return;
  }

  podClient = new PodBriefClient(baseUrl, 'bridge', secret);
  log('info', 'Pod brief client initialized', {
    event: 'pod_client_init',
    baseUrl,
  });
}

/** Map role names to their pod inbox names (for the pod client) */
const ROLE_NAME_MAP: Record<string, string> = {
  silas: 'silas',
  wren: 'wren',
  kade: 'kade',
};

/**
 * Write a brief to the role's inbox.
 * Filesystem write is primary (must succeed). Pod write is best-effort.
 */
export async function writeBrief(
  role: RoleConfig,
  filename: string,
  content: string
): Promise<void> {
  // Filesystem write — primary, must succeed
  if (!fs.existsSync(role.briefsPath)) {
    fs.mkdirSync(role.briefsPath, { recursive: true });
  }
  const filepath = path.join(role.briefsPath, filename);
  fs.writeFileSync(filepath, content, 'utf-8');

  // Pod write — best-effort
  const podRoleName = ROLE_NAME_MAP[role.name];
  if (podClient && podRoleName) {
    const success = await podClient.writeBriefToPod(podRoleName, filename, content);
    if (!success) {
      log('warn', `Pod write failed for ${role.name}/${filename}, filesystem copy exists`, {
        event: 'pod_fallback',
        role: role.name,
        filename,
      });
    }
  }
}

/**
 * Read a brief from the role's inbox. Reads from filesystem (primary source).
 */
export function readBrief(role: RoleConfig, filename: string): string | null {
  const filepath = path.join(role.briefsPath, filename);
  try {
    return fs.readFileSync(filepath, 'utf-8');
  } catch {
    return null;
  }
}

/**
 * List briefs in the role's inbox. Lists from filesystem (primary source).
 */
export function listBriefs(role: RoleConfig): string[] {
  try {
    if (!fs.existsSync(role.briefsPath)) return [];
    return fs.readdirSync(role.briefsPath).filter(f => f.endsWith('.md'));
  } catch {
    return [];
  }
}

// --- State file operations ---

/** Map role names to their filesystem state directories */
const ROLE_STATE_DIRS: Record<string, string> = {
  silas: path.resolve(__dirname, '../../../architect'),
  wren: path.resolve(__dirname, '../../../product-manager'),
  kade: path.resolve(__dirname, '../../../engineer'),
};

/**
 * Write a state file for a role.
 * Filesystem write is primary (must succeed). Pod write is best-effort.
 */
export async function writeState(
  role: string,
  filename: string,
  content: string
): Promise<void> {
  const stateDir = ROLE_STATE_DIRS[role];
  if (!stateDir) {
    log('warn', `writeState: unknown role '${role}'`, { event: 'state_write_error' });
    return;
  }

  // Filesystem write — primary, must succeed
  const filepath = path.join(stateDir, filename);
  const dir = path.dirname(filepath);
  if (!fs.existsSync(dir)) {
    fs.mkdirSync(dir, { recursive: true });
  }
  fs.writeFileSync(filepath, content, 'utf-8');

  // Pod write — best-effort
  if (podClient) {
    const success = await podClient.writeStateToPod(role, filename, content);
    if (!success) {
      log('warn', `Pod state write failed for ${role}/${filename}, filesystem copy exists`, {
        event: 'pod_state_fallback',
        role,
        filename,
      });
    }
  }
}

/**
 * Read a state file for a role. Reads from filesystem (primary source).
 */
export function readState(role: string, filename: string): string | null {
  const stateDir = ROLE_STATE_DIRS[role];
  if (!stateDir) {
    log('warn', `readState: unknown role '${role}'`, { event: 'state_read_error' });
    return null;
  }

  const filepath = path.join(stateDir, filename);
  try {
    return fs.readFileSync(filepath, 'utf-8');
  } catch {
    return null;
  }
}
