import fs from 'fs';
import path from 'path';

export interface ChatMessage {
  id: string;
  sender: string;
  content: string;
  timestamp: number;
  tokens?: { input: number; output: number };
}

export interface Decision {
  marker: string;
  speaker: string;
  timestamp: number;
  messageId: string;
  cardLink: string | null;
}

export interface SessionReturn {
  session: SessionSummary;
  decisions: Decision[];
  archiveLink: string;
  messages: ChatMessage[];
}

export interface SessionSummary {
  started: string;
  ended: string;
  participants: string[];
  model: string;
  totalTokens: { input: number; output: number };
  estimatedCost: number;
  messageCount: number;
  decisionCount: number;
}

// Approximate costs per million tokens
const MODEL_COSTS: Partial<Record<string, { input: number; output: number }>> = {
  'claude-haiku-4-5-20251001': { input: 0.80, output: 4.00 },
  'claude-sonnet-4-20250514': { input: 3.00, output: 15.00 },
  'claude-opus-4-20250514': { input: 15.00, output: 75.00 },
};

// Pattern: DECISION: or DECISION - at start of message (case-insensitive)
const DECISION_PATTERN = /^DECISION[\s:–—-]+(.+)/im;

export class Transcript {
  private messages: ChatMessage[] = [];
  private startTime: string;
  private model: string;
  private nextId = 1;
  private lastSavePath: string | null = null;

  constructor(model: string) {
    this.startTime = new Date().toISOString();
    this.model = model;
  }

  add(sender: string, content: string, tokens?: { input: number; output: number }): ChatMessage {
    const msg: ChatMessage = {
      id: String(this.nextId++),
      sender,
      content,
      timestamp: Date.now(),
      tokens,
    };
    this.messages.push(msg);
    return msg;
  }

  getMessages(): ChatMessage[] {
    return [...this.messages];
  }

  reset(): void {
    this.messages = [];
    this.nextId = 1;
    this.startTime = new Date().toISOString();
    this.lastSavePath = null;
  }

  getTotalTokens(): { input: number; output: number } {
    return this.messages.reduce(
      (acc, m) => ({
        input: acc.input + (m.tokens?.input || 0),
        output: acc.output + (m.tokens?.output || 0),
      }),
      { input: 0, output: 0 }
    );
  }

  getEstimatedCost(): number {
    const tokens = this.getTotalTokens();
    const costs = MODEL_COSTS[this.model] ?? { input: 0.80, output: 4.00 };
    return (tokens.input * costs.input + tokens.output * costs.output) / 1_000_000;
  }

  /**
   * Extract DECISION markers from the transcript.
   * Looks for messages starting with "DECISION:" or "DECISION -"
   */
  extractDecisions(): Decision[] {
    const decisions: Decision[] = [];

    for (const msg of this.messages) {
      const match = msg.content.match(DECISION_PATTERN);
      if (match) {
        decisions.push({
          marker: match[0].trim(),
          speaker: msg.sender,
          timestamp: msg.timestamp,
          messageId: msg.id,
          cardLink: null, // populated if message contains card reference
        });
      }
    }

    return decisions;
  }

  getSummary(): SessionSummary {
    const senders = new Set(this.messages.map((m) => m.sender));
    return {
      started: this.startTime,
      ended: new Date().toISOString(),
      participants: [...senders],
      model: this.model,
      totalTokens: this.getTotalTokens(),
      estimatedCost: this.getEstimatedCost(),
      messageCount: this.messages.length,
      decisionCount: this.extractDecisions().length,
    };
  }

  /**
   * Build the structured return object for the invoking process.
   * Contains: session summary, parsed decisions, archive link, full messages.
   */
  buildReturnObject(archivePath: string): SessionReturn {
    return {
      session: this.getSummary(),
      decisions: this.extractDecisions(),
      archiveLink: archivePath,
      messages: this.messages,
    };
  }

  /**
   * Save transcript to disk. Returns the file path.
   * Called on session end and periodically via auto-save.
   */
  save(): string {
    const transcriptsDir = path.join(__dirname, '..', 'transcripts');
    if (!fs.existsSync(transcriptsDir)) {
      fs.mkdirSync(transcriptsDir, { recursive: true });
    }

    // Reuse the same filename for auto-saves within a session
    if (!this.lastSavePath) {
      const timestamp = new Date().toISOString().replace(/[:.]/g, '-').slice(0, 19);
      this.lastSavePath = path.join(transcriptsDir, `${timestamp}.json`);
    }

    const returnObj = this.buildReturnObject(this.lastSavePath);
    fs.writeFileSync(this.lastSavePath, JSON.stringify(returnObj, null, 2));
    return this.lastSavePath;
  }
}
