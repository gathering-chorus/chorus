// Re-export from chorus-sdk — board-client is now a consumer, not the source
import { emit, type SpineEvent } from 'chorus-sdk';

export type { SpineEvent };

export function emitSpineEvent(event: string, role: string, extra: Record<string, string> = {}): void {
  emit(event, role, extra, { appName: 'board-client', component: 'cli' });
}

export function emitChorusEvent(event: string, role: string, extra: Record<string, string> = {}): void {
  emit(event, role, extra, { appName: 'chorus-events', component: 'lifecycle' });
}
