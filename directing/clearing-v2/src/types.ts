// @domain: messages

export interface NostrEvent {
  id: string;
  pubkey: string;
  created_at: number;
  kind: number;
  tags: string[][];
  content: string;
  sig: string;
}

export type RoleName = 'jeff' | 'wren' | 'silas' | 'kade' | 'abby-normal';

export interface ClearingMessage {
  id: string;
  from: string;
  text: string;
  ts: string;
  type: 'jeff-input' | 'role-response' | 'system' | 'gap' | 'thinking';
  visible: boolean;
  pubkey?: string;
  room?: string;
  tick?: boolean;
  signer?: string;
  /** #4592 — a nudge between roles or a system alert (v1 tags the note); the room's toggle hides these. */
  kind?: 'nudge' | 'alert';
}

export interface RoleTile {
  role: string;
  name: string;
  state: 'idle' | 'building' | 'blocked' | 'observing' | 'waiting';
  card: number | null;
  cards?: Array<{ id: number; title: string }>;
  stream?: Array<{ action: string; timestamp: string }>;
  lastActivity: string | null;
  lastEvent: string | null;
  lastSeenAt: string | null;
  detail: string | null;
}

export interface PulseNudgePayload {
  from: string;
  to: string;
  content: string;
}

export interface RelayStatus {
  connected: boolean;
  url: string;
  bufferedCount: number;
  lastSeenSec: number | null;
}
