import { log } from './logger';

export type ConversationEventType =
  | 'conversation_start'
  | 'conversation_turn'
  | 'conversation_end';

interface BaseEvent {
  event: ConversationEventType;
  conversationId: string;
  channel: string;
  timestamp: string;
}

interface ConversationStartEvent extends BaseEvent {
  event: 'conversation_start';
  mode: 'group';
  triggerText: string;
  participatingRoles: string[];
  turnOrder: string[];
}

interface ConversationTurnEvent extends BaseEvent {
  event: 'conversation_turn';
  role: string;
  turnNumber: number;
  inputTokens: number;
  outputTokens: number;
}

interface ConversationEndEvent extends BaseEvent {
  event: 'conversation_end';
  totalTurns: number;
  totalInputTokens: number;
  totalOutputTokens: number;
  durationMs: number;
}

type ConversationEvent = ConversationStartEvent | ConversationTurnEvent | ConversationEndEvent;

export class ConversationEventLogger {
  private conversationCounter = 0;

  generateConversationId(): string {
    this.conversationCounter++;
    const dateStr = new Date().toISOString().replace(/[-:T]/g, '').slice(0, 14);
    return `conv-${dateStr}-${this.conversationCounter}`;
  }

  logStart(
    conversationId: string,
    channel: string,
    triggerText: string,
    participatingRoles: string[],
    turnOrder: string[]
  ): void {
    const event: ConversationStartEvent = {
      event: 'conversation_start',
      conversationId,
      channel,
      timestamp: new Date().toISOString(),
      mode: 'group',
      triggerText: triggerText.slice(0, 200),
      participatingRoles,
      turnOrder,
    };
    this.emit(event);
  }

  logTurn(
    conversationId: string,
    channel: string,
    role: string,
    turnNumber: number,
    inputTokens: number,
    outputTokens: number
  ): void {
    const event: ConversationTurnEvent = {
      event: 'conversation_turn',
      conversationId,
      channel,
      timestamp: new Date().toISOString(),
      role,
      turnNumber,
      inputTokens,
      outputTokens,
    };
    this.emit(event);
  }

  logEnd(
    conversationId: string,
    channel: string,
    totalTurns: number,
    totalInputTokens: number,
    totalOutputTokens: number,
    durationMs: number
  ): void {
    const event: ConversationEndEvent = {
      event: 'conversation_end',
      conversationId,
      channel,
      timestamp: new Date().toISOString(),
      totalTurns,
      totalInputTokens,
      totalOutputTokens,
      durationMs,
    };
    this.emit(event);
  }

  private emit(event: ConversationEvent): void {
    // Structured JSON to stdout — Promtail picks this up
    process.stdout.write(JSON.stringify(event) + '\n');
    log('info', `${event.event}: ${event.conversationId}`, event as unknown as Record<string, unknown>);
  }
}
