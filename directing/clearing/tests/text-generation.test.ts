import { TextGenerationClient, readSSE, textConfiguration } from '../src/text-generation';
import { Transcript } from '../src/transcript';

const request = {model:'test-model',system:'system',input:'hello',maxTokens:50};
function stream(chunks: string[]) {
  return new Response(new ReadableStream({start(controller) { for (const chunk of chunks) controller.enqueue(new TextEncoder().encode(chunk)); controller.close(); }}));
}
function client(fetcher: jest.Mock, protocol: 'openai-compatible' | 'openai-responses' = 'openai-compatible', timeoutMs = 1000) {
  return new TextGenerationClient({protocol,baseURL:'http://localhost/v1',timeoutMs},fetcher as typeof fetch);
}

test('defaults to Anthropic and rejects unknown provider configuration', () => {
  expect(textConfiguration({}).protocol).toBe('anthropic');
  expect(() => textConfiguration({CLEARING_PROVIDER:'typo'})).toThrow('Unsupported');
  expect(() => textConfiguration({CLEARING_TIMEOUT_MS:'no'})).toThrow('positive');
});
test('chat wire contract sends no tools; absent usage remains unknown', async () => {
  const fetcher = jest.fn().mockResolvedValue(new Response(JSON.stringify({choices:[{message:{content:'answer'},finish_reason:'stop'}]})));
  const result = await client(fetcher).generate(request);
  expect(result).toEqual({content:'answer',inputTokens:null,outputTokens:null,finishReason:'stop'});
  const body = JSON.parse(fetcher.mock.calls[0][1].body);
  expect(body).toMatchObject({model:'test-model',max_tokens:50,messages:[{role:'system',content:'system'},{role:'user',content:'hello'}]});
  expect(body.tools).toBeUndefined();
  expect(String(fetcher.mock.calls[0][0])).toBe('http://localhost/v1/chat/completions');
});
test('Responses endpoint has its own wire shape and usage', async () => {
  const fetcher = jest.fn().mockResolvedValue(new Response(JSON.stringify({status:'completed',output:[{content:[{type:'output_text',text:'ok'}]}],usage:{input_tokens:10,output_tokens:3}})));
  expect(await client(fetcher,'openai-responses').generate(request)).toEqual({content:'ok',inputTokens:10,outputTokens:3,finishReason:'completed'});
  expect(JSON.parse(fetcher.mock.calls[0][1].body)).toMatchObject({input:'hello',instructions:'system',max_output_tokens:50,store:false});
  expect(String(fetcher.mock.calls[0][0])).toBe('http://localhost/v1/responses');
});
test('SSE streams split chunks, usage-only event, comments and DONE', async () => {
  const fetcher = jest.fn().mockResolvedValue(stream([':ping\r\n\r\ndata: {"choices":[{"delta":{"content":"he', 'llo"},"finish_reason":null}]}\r\n\r\n', 'data: {"choices":[{"delta":{},"finish_reason":"stop"}]}\n\ndata: {"usage":{"prompt_tokens":8,"completion_tokens":2},"choices":[]}\n\ndata: [DONE]\n\n']));
  const chunks: string[] = [];
  expect(await client(fetcher).generate({...request,onToken:token => chunks.push(token)})).toEqual({content:'hello',inputTokens:8,outputTokens:2,finishReason:'stop'});
  expect(chunks).toEqual(['hello']);
});
test('SSE parser preserves split UTF8 and multiline data fields', async () => {
  const bytes = new TextEncoder().encode('data: café\ndata: second\n\n');
  const body = new ReadableStream<Uint8Array>({start(c) { for (const byte of bytes) c.enqueue(Uint8Array.of(byte)); c.close(); }});
  const events = []; for await (const event of readSSE(body)) events.push(event);
  expect(events).toEqual(['café\nsecond']);
});
test('Responses stream normalizes text delta and completed usage', async () => {
  const fetcher = jest.fn().mockResolvedValue(stream(['data: {"type":"response.output_text.delta","delta":"ok"}\n\ndata: {"type":"response.completed","response":{"status":"completed","usage":{"input_tokens":4,"output_tokens":1}}}\n\n']));
  const result = await client(fetcher,'openai-responses').generate({...request,onToken:() => undefined});
  expect(result).toEqual({content:'ok',inputTokens:4,outputTokens:1,finishReason:'completed'});
});
test.each([
  ['data: {"choices":[]}\n\n','protocol'],
  ['data: invalid-json\n\n','protocol'],
  ['data: {"error":{"message":"SECRET"}}\n\n','provider'],
])('stream errors are sanitized (%s)', async (text,code) => {
  const operation = client(jest.fn().mockResolvedValue(stream([text]))).generate({...request,onToken:() => undefined});
  await expect(operation).rejects.toMatchObject({code});
  await expect(operation).rejects.not.toThrow('SECRET');
});
test('authentication errors never echo provider body or credentials', async () => {
  const result = client(jest.fn().mockResolvedValue(new Response('credential-secret',{status:401}))).generate(request);
  await expect(result).rejects.toMatchObject({code:'authentication'});
  await expect(result).rejects.not.toThrow('credential-secret');
});
test('credential config names environment variables; missing explicit reference fails closed', async () => {
  const fetcher = jest.fn();
  await expect(new TextGenerationClient({protocol:'openai-compatible',apiKeyEnv:'UNSET_CHORUS_TEST_CREDENTIAL'},fetcher).generate(request)).rejects.toMatchObject({code:'configuration'});
  expect(fetcher).not.toHaveBeenCalled();
});
test('abort and timeout stop the transport', async () => {
  const fetcher = jest.fn((_url,init) => new Promise((_resolve,reject) => init.signal.addEventListener('abort',() => reject(new DOMException('aborted','AbortError')))));
  const controller = new AbortController();
  const operation = client(fetcher).generate({...request,signal:controller.signal}); controller.abort();
  await expect(operation).rejects.toMatchObject({code:'cancelled'});
  await expect(client(fetcher,'openai-compatible',10).generate(request)).rejects.toMatchObject({code:'timeout'});
});
test('unknown token counts remain unknown in saved transcript totals and costs', () => {
  const transcript = new Transcript('claude-haiku-4-5-20251001');
  transcript.add('Wren','known',{input:3,output:2}); transcript.add('Kade','unknown',{input:null,output:null});
  expect(transcript.getTotalTokens()).toEqual({input:null,output:null});
  expect(transcript.getEstimatedCost()).toBeNull();
});
test.each(['openai-compatible','openai-responses'] as const)('continuation preserves native message roles for %s', async protocol => {
  const fetcher = jest.fn().mockResolvedValue(new Response(JSON.stringify(protocol === 'openai-compatible' ? {choices:[{message:{content:'continued'},finish_reason:'stop'}]} : {status:'completed',output_text:'continued'})));
  const history = [{role:'user' as const,content:'first question'},{role:'assistant' as const,content:'first answer'}];
  await client(fetcher,protocol).generate({...request,history});
  const body = JSON.parse(fetcher.mock.calls[0][1].body);
  expect(protocol === 'openai-compatible' ? body.messages.slice(1) : body.input).toEqual([...history,{role:'user',content:'hello'}]);
  expect(body.tools).toBeUndefined();
});
test('provider-native refusal is not mistaken for an empty successful completion', async () => {
  const chat = jest.fn().mockResolvedValue(new Response(JSON.stringify({choices:[{finish_reason:'stop',message:{content:null,refusal:'declined'}}]})));
  await expect(client(chat).generate(request)).rejects.toMatchObject({code:'provider'});
  const responses = jest.fn().mockResolvedValue(new Response(JSON.stringify({status:'completed',output:[{content:[{type:'refusal',refusal:'declined'}]}]})));
  await expect(client(responses,'openai-responses').generate(request)).rejects.toMatchObject({code:'provider'});
});
