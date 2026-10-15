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
  type: 'jeff-input' | 'role-response' | 'system' | 'gap';
  visible: boolean;
  pubkey?: string;
  room?: string;
  tick?: boolean;
}

export interface RoleTile {
  role: string;
  name: string;
  state: 'idle' | 'building' | 'blocked' | 'observing' | 'waiting';
  card: number | null;
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
