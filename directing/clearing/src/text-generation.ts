/** Text-only model transport. This does not launch an agent or grant tools. */
import Anthropic from '@anthropic-ai/sdk';

export type TextProtocol = 'anthropic' | 'openai-compatible' | 'openai-responses';
export interface TextConfiguration {
  protocol: TextProtocol;
  baseURL?: string;
  apiKeyEnv?: string;
  timeoutMs?: number;
}
export interface TextRequest {
  model: string;
  system: string;
  input: string;
  maxTokens: number;
  history?: Array<{ role: 'user' | 'assistant'; content: string }>;
  signal?: AbortSignal;
  onToken?: (token: string) => void;
}
export interface TextResult {
  content: string;
  inputTokens: number | null;
  outputTokens: number | null;
  finishReason?: string | null;
}
export class GenerationError extends Error {
  constructor(public readonly code: 'configuration' | 'authentication' | 'rate_limit' | 'timeout' | 'cancelled' | 'network' | 'provider' | 'protocol', message: string) {
    super(message);
    this.name = 'GenerationError';
  }
}
export function textConfiguration(env = process.env): TextConfiguration {
  const protocol = env.CLEARING_PROVIDER || 'anthropic';
  if (!['anthropic', 'openai-compatible', 'openai-responses'].includes(protocol)) {
    throw new GenerationError('configuration', 'Unsupported CLEARING_PROVIDER');
  }
  const timeoutMs = Number(env.CLEARING_TIMEOUT_MS || 60000);
  if (!Number.isFinite(timeoutMs) || timeoutMs <= 0) throw new GenerationError('configuration', 'CLEARING_TIMEOUT_MS must be positive');
  return { protocol: protocol as TextProtocol, baseURL: env.CLEARING_BASE_URL,
    apiKeyEnv: env.CLEARING_API_KEY_ENV, timeoutMs };
}
export function tokenCount(value: unknown): number | null {
  return typeof value === 'number' && Number.isFinite(value) && value >= 0 ? value : null;
}

/** Incremental SSE parser: handles split UTF-8, CRLF, comments and multiline data. */
export async function* readSSE(body: ReadableStream<Uint8Array>): AsyncGenerator<string> {
  const reader = body.getReader();
  const decoder = new TextDecoder();
  let pending = '';
  let data: string[] = [];
  let dataLength = 0;
  function consume(line: string): string | undefined {
    if (line.endsWith('\r')) line = line.slice(0, -1);
    if (line === '') { const event = data.length ? data.join('\n') : undefined; data = []; dataLength = 0; return event; }
    if (line.startsWith('data:')) {
      dataLength += line.length;
      if (dataLength > 8 * 1024 * 1024) throw new GenerationError('protocol','Provider event exceeds size limit');
      data.push(line.slice(5).replace(/^ /, ''));
    }
    return undefined;
  }
  function completeLines(): string[] {
    const events: string[]=[]; let newline: number;
    while ((newline=pending.indexOf('\n'))>=0) {
      const event=consume(pending.slice(0,newline)); pending=pending.slice(newline+1);
      if (event!==undefined) events.push(event);
    }
    return events;
  }
  try {
    for (;;) {
      const part = await reader.read();
      pending += part.done ? decoder.decode() : decoder.decode(part.value, { stream: true });
      if (pending.length > 8 * 1024 * 1024) throw new GenerationError('protocol', 'Provider event exceeds size limit');
      for (const event of completeLines()) yield event;
      if (part.done) break;
    }
    const finalEvent=consume(pending); if (finalEvent!==undefined) yield finalEvent;
    if (data.length) yield data.join('\n');
  } finally { await reader.cancel().catch(() => undefined); reader.releaseLock(); }
}

interface UsageWire { input_tokens?: unknown; output_tokens?: unknown; prompt_tokens?: unknown; completion_tokens?: unknown }
interface ContentWire { type?: string; text?: string; refusal?: string }
interface Json {
  error?: unknown; output_text?: string; status?: string; type?: string; delta?: string;
  output?: Array<{content?: ContentWire[]}>;
  usage?: UsageWire; response?: Json;
  choices?: Array<{message?: {content?: unknown;refusal?: string};delta?: {content?: string;refusal?: string};finish_reason?: string | null}>;
}
function parseJson(text: string): Json {
  try { const value: unknown = JSON.parse(text); if (value && typeof value === 'object') return value as Json; } catch { /* sanitized error */ }
  throw new GenerationError('protocol', 'Provider returned invalid JSON');
}
function responseText(value: Json): string {
  if (typeof value.output_text === 'string') return value.output_text;
  if (value.output !== undefined && !Array.isArray(value.output)) throw new GenerationError('protocol','Expected response output array');
  return (value.output || []).flatMap(item => Array.isArray(item.content) ? item.content : [])
    .filter(item => item.type === 'output_text').map(item => item.text || '').join('');
}
function responseRefused(value: Json): boolean {
  if (value.choices?.[0]?.message?.refusal || value.choices?.[0]?.delta?.refusal) return true;
  if (['response.refusal.delta','response.refusal.done'].includes(value.type || '')) return true;
  return Array.isArray(value.output) && value.output.some(item => Array.isArray(item.content) && item.content.some(part => part.type === 'refusal'));
}
function assertResponse(value: Json, responses: boolean) {
  if (value.error || ['error','response.failed','response.incomplete'].includes(value.type || '')) throw new GenerationError('provider','Model response failed or incomplete');
  if (responses && ['failed','incomplete'].includes(value.status || value.response?.status || '')) throw new GenerationError('provider','Model response failed or incomplete');
  if (responseRefused(value)) throw new GenerationError('provider','Model response was refused');
}
function updateUsage(result: TextResult, value: UsageWire | undefined, responses: boolean) {
  if (!value) return;
  result.inputTokens=tokenCount(responses ? value.input_tokens : value.prompt_tokens);
  result.outputTokens=tokenCount(responses ? value.output_tokens : value.completion_tokens);
}
function providerFailure(error: unknown, signal: AbortSignal, timedOut: boolean): GenerationError {
  if (signal.aborted) return new GenerationError(timedOut ? 'timeout' : 'cancelled', timedOut ? 'Text generation timed out' : 'Text generation cancelled');
  if (error instanceof GenerationError) return error;
  const status=(error as {status?:number}).status;
  if (status===401 || status===403) return new GenerationError('authentication','Text generation failed');
  if (status===429) return new GenerationError('rate_limit','Text generation failed');
  return new GenerationError(status ? 'provider' : 'network','Text generation failed');
}
function requestBody(request: TextRequest, responses: boolean) {
  const stream=!!request.onToken;
  const conversation=[...(request.history || []),{role:'user',content:request.input}];
  return responses
    ? {model:request.model,instructions:request.system,input:request.history?.length ? conversation : request.input,max_output_tokens:request.maxTokens,stream,store:false}
    : {model:request.model,messages:[{role:'system',content:request.system},...conversation],max_tokens:request.maxTokens,stream,...(stream ? {stream_options:{include_usage:true}} : {})};
}
function completeResponse(value: Json, responses: boolean): TextResult {
  assertResponse(value,responses);
  const content=responses ? responseText(value) : value.choices?.[0]?.message?.content || '';
  if (typeof content!=='string') throw new GenerationError('protocol','Expected a text response');
  const result: TextResult={content,inputTokens:null,outputTokens:null,finishReason:responses ? value.status ?? null : value.choices?.[0]?.finish_reason ?? null};
  updateUsage(result,value.usage,responses); return result;
}
function streamEvent(event: Json, result: TextResult, responses: boolean, onToken: (text: string)=>void): boolean {
  assertResponse(event,responses);
  const choice=event.choices?.[0];
  const delta=responses ? (event.type==='response.output_text.delta' ? event.delta : '') : choice?.delta?.content;
  if (typeof delta==='string' && delta) { result.content+=delta; onToken(delta); }
  if (responses && event.type==='response.completed') {
    updateUsage(result,event.response?.usage,true); result.finishReason='completed'; return true;
  }
  if (!responses) { updateUsage(result,event.usage,false); if (choice?.finish_reason) result.finishReason=choice.finish_reason; }
  return false;
}

export class TextGenerationClient {
  private readonly anthropic?: Anthropic;
  constructor(private readonly config: TextConfiguration = textConfiguration(), private readonly fetcher: typeof fetch = fetch) {
    if (!['anthropic','openai-compatible','openai-responses'].includes(config.protocol)) throw new GenerationError('configuration','Unsupported provider protocol');
    if (config.timeoutMs !== undefined && (!Number.isFinite(config.timeoutMs) || config.timeoutMs <= 0)) throw new GenerationError('configuration','Timeout must be positive');
    if (config.apiKeyEnv && !/^[A-Za-z_][A-Za-z0-9_]*$/.test(config.apiKeyEnv)) throw new GenerationError('configuration', 'Invalid credential environment variable name');
    if (config.protocol === 'anthropic') this.anthropic = new Anthropic({
      ...(config.baseURL ? { baseURL: config.baseURL } : {}),
      ...(config.apiKeyEnv ? { apiKey: this.key() } : {}),
      maxRetries: 0,
    });
  }
  private key(): string | undefined {
    const name = this.config.apiKeyEnv || (this.config.protocol === 'anthropic' ? 'ANTHROPIC_API_KEY' : 'OPENAI_API_KEY');
    // eslint-disable-next-line security/detect-object-injection -- Name validated by constructor or a fixed provider default.
    const key = process.env[name];
    if (this.config.apiKeyEnv && !key) throw new GenerationError('configuration', `Missing credential environment variable: ${name}`);
    return key;
  }
  async generate(request: TextRequest): Promise<TextResult> {
    const controller = new AbortController();
    const deadline = {expired:false};
    const timer = setTimeout(() => { deadline.expired = true; controller.abort(); }, this.config.timeoutMs ?? 60000);
    const abort = () => controller.abort();
    request.signal?.addEventListener('abort', abort, { once: true });
    if (request.signal?.aborted) controller.abort();
    try {
      controller.signal.throwIfAborted();
      return this.anthropic ? await this.anthropicGenerate(request, controller.signal) : await this.openaiGenerate(request, controller.signal);
    } catch (error) {
      throw providerFailure(error,controller.signal,deadline.expired);
    } finally {
      clearTimeout(timer); request.signal?.removeEventListener('abort', abort);
    }
  }
  private async anthropicGenerate(request: TextRequest, signal: AbortSignal): Promise<TextResult> {
    const body = { model: request.model, max_tokens: request.maxTokens, system: request.system,
      messages: [...(request.history || []), { role: 'user' as const, content: request.input }] };
    if (request.onToken) {
      const stream = this.anthropic!.messages.stream(body, { signal });
      let content = '';
      stream.on('text', text => { content += text; request.onToken!(text); });
      const final = await stream.finalMessage();
      return { content, inputTokens: tokenCount(final.usage.input_tokens), outputTokens: tokenCount(final.usage.output_tokens), finishReason: final.stop_reason };
    }
    const final = await this.anthropic!.messages.create(body, { signal });
    return { content: final.content.filter((b): b is Anthropic.TextBlock => b.type === 'text').map(b => b.text).join(''),
      inputTokens: tokenCount(final.usage.input_tokens), outputTokens: tokenCount(final.usage.output_tokens), finishReason: final.stop_reason };
  }
  private async openaiGenerate(request: TextRequest, signal: AbortSignal): Promise<TextResult> {
    const responses = this.config.protocol === 'openai-responses';
    const base = this.config.baseURL || 'https://api.openai.com/v1';
    const url = new URL(base.replace(/\/$/, '') + (responses ? '/responses' : '/chat/completions'));
    if (!['https:', 'http:'].includes(url.protocol) || url.username || url.password) throw new GenerationError('configuration', 'Invalid model endpoint');
    const body = requestBody(request,responses);
    const key = this.key();
    const response = await this.fetcher(url, { method: 'POST', headers: { 'content-type': 'application/json', ...(key ? { authorization: `Bearer ${key}` } : {}) }, body: JSON.stringify(body), signal });
    if (!response.ok) {
      await response.body?.cancel();
      throw new GenerationError(response.status === 401 || response.status === 403 ? 'authentication' : response.status === 429 ? 'rate_limit' : 'provider', `Model endpoint returned HTTP ${response.status}`);
    }
    if (!request.onToken) return completeResponse(parseJson(await response.text()),responses);
    return this.streamResponse(response,responses,request.onToken);
  }
  private async streamResponse(response: Response, responses: boolean, onToken: (text: string)=>void): Promise<TextResult> {
    if (!response.body) throw new GenerationError('protocol','Provider stream is missing');
    const result: TextResult={content:'',inputTokens:null,outputTokens:null};
    let completed=false;
    for await (const raw of readSSE(response.body)) {
      if (raw==='[DONE]') { completed=true; break; }
      if (streamEvent(parseJson(raw),result,responses,onToken)) completed=true;
    }
    if (!completed) throw new GenerationError('protocol','Model stream ended before completion');
    return result;
  }
}
