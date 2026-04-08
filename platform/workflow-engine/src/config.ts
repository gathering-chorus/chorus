import { resolve, dirname } from 'path';
import { WorkflowEngineConfig, Role } from './types';

const MESSAGES_DIR = resolve(__dirname, '../..');
const CASCADEPROJECTS = resolve(MESSAGES_DIR, '..');

export const VALID_ROLES: readonly Role[] = ['silas', 'kade', 'wren', 'jeff'] as const;

export const DEFAULT_CONFIG: WorkflowEngineConfig = {
  activeDir: resolve(MESSAGES_DIR, 'workflows/active'),
  archiveDir: resolve(MESSAGES_DIR, 'workflows/archive'),
  briefDirs: {
    silas: resolve(CASCADEPROJECTS, 'roles/silas/briefs'),
    kade: resolve(CASCADEPROJECTS, 'roles/kade/briefs'),
    wren: resolve(CASCADEPROJECTS, 'roles/wren/briefs'),
    jeff: resolve(CASCADEPROJECTS, 'roles/wren/briefs'),
  },
  handoffLogPath: resolve(MESSAGES_DIR, 'logs/handoffs.log'),
};

export function isValidRole(role: string): role is Role {
  return (VALID_ROLES as readonly string[]).includes(role);
}

export function nowISO(): string {
  return new Date().toISOString().replace(/\.\d{3}Z$/, 'Z');
}
