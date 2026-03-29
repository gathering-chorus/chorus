import { WorkflowEngineConfig, Role } from './types';
export declare const VALID_ROLES: readonly Role[];
export declare const DEFAULT_CONFIG: WorkflowEngineConfig;
export declare function isValidRole(role: string): role is Role;
export declare function nowISO(): string;
