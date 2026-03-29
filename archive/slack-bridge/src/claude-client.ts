import Anthropic from '@anthropic-ai/sdk';
import { log } from './logger';
import { metrics } from './metrics';

export interface ClaudeResponse {
  text: string;
  inputTokens: number;
  outputTokens: number;
}

export class ClaudeClient {
  private client: Anthropic;
  private model = 'claude-sonnet-4-5-20250929';
  private maxTokens = 1024;

  constructor(apiKey: string) {
    this.client = new Anthropic({ apiKey });
  }

  async respond(systemPrompt: string, userMessage: string, roleName: string): Promise<string>;
  async respond(systemPrompt: string, userMessage: string, roleName: string, returnUsage: true, maxTokens?: number): Promise<ClaudeResponse>;
  async respond(
    systemPrompt: string,
    userMessage: string,
    roleName: string,
    returnUsage?: true,
    maxTokens?: number
  ): Promise<string | ClaudeResponse> {
    const timer = metrics.apiLatency.startTimer({ role: roleName });

    try {
      metrics.apiCalls.inc({ role: roleName });

      const response = await this.client.messages.create({
        model: this.model,
        max_tokens: maxTokens || this.maxTokens,
        temperature: 0.7,
        system: systemPrompt,
        messages: [{ role: 'user', content: userMessage }],
      });

      const text = response.content
        .filter((block): block is Anthropic.TextBlock => block.type === 'text')
        .map(block => block.text)
        .join('\n');

      const inputTokens = response.usage.input_tokens;
      const outputTokens = response.usage.output_tokens;

      log('info', `Claude response for ${roleName}`, { inputTokens, outputTokens });

      if (returnUsage) {
        return { text, inputTokens, outputTokens };
      }
      return text;
    } catch (err) {
      metrics.errors.inc({ type: 'claude_api' });
      log('error', `Claude API error for ${roleName}: ${err}`);
      throw err;
    } finally {
      timer();
    }
  }
}
