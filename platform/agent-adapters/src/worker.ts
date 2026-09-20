import { createInterface } from 'node:readline';
import { resolve } from 'node:path';
import { GeminiRuntime } from './gemini';
import { OpenCodeRuntime } from './opencode';
import { AdapterError, Json, required, Runtime } from './types';

const write = (value: Json) => process.stdout.write(JSON.stringify(value) + '\n');
const emit = (type: string, data: Json = {}, native_session_id?: string, turn_id?: string) => write({version:1,method:'event',params:{type,native_session_id,turn_id,data}});
const name = process.argv[process.argv.indexOf('--runtime') + 1];
let runtime: Runtime | undefined;
if (name === 'gemini') runtime = new GeminiRuntime(emit);
else if (name === 'opencode') runtime = new OpenCodeRuntime(emit);
else if (name !== 'api') { process.stderr.write('Expected --runtime opencode|gemini|api\n'); process.exit(2); }

function textModule() {
  try { return require(resolve(__dirname,'../../../directing/clearing/dist/text-generation.js')); }
  catch { throw new AdapterError('unavailable','Build directing/clearing before using text API jobs'); }
}
const jobs = new Map<string,AbortController>();
async function run(params: Json, requestId: string) {
  if (params.no_tools !== true) throw new AdapterError('invalid_request','Text jobs require no_tools:true');
  const provider = params.provider || {};
  const protocol = provider.protocol === 'openai-chat' ? 'openai-compatible' : required(provider.protocol,'provider.protocol');
  if (!['anthropic','openai-compatible','openai-responses'].includes(protocol)) throw new AdapterError('configuration','Unsupported text provider protocol');
  const {TextGenerationClient} = textModule();
  const controller = new AbortController(); jobs.set(requestId,controller);
  try {
    const client = new TextGenerationClient({protocol,baseURL:provider.base_url,apiKeyEnv:provider.api_key_env,timeoutMs:params.timeout_ms || 60000});
    const result = await client.generate({model:required(params.model,'model'),input:required(params.input,'input'),system:params.instructions || '',maxTokens:params.max_output_tokens || params.max_tokens || 4096,signal:controller.signal});
    return {text:result.content,usage:{input_tokens:result.inputTokens,output_tokens:result.outputTokens},finish_reason:result.finishReason ?? null};
  } finally { jobs.delete(requestId); }
}
async function execute(method: string, params: Json, id: string): Promise<Json> {
    let result: Json;
    if (method === 'run') result = await run(params,id);
    else if (method === 'cancel' && params.job_id) { jobs.get(String(params.job_id))?.abort(); result = {requested:true}; }
    else if (name === 'api' && method === 'probe') { textModule(); result = {runtime:'api',runtime_version:'text-client/1',capabilities:{no_tools:true,structured_output:false,resume:false,autonomous_wake:false,mid_turn_steering:false,cancellation:true,before_tool:false,history_recovery:false,context_boundaries:[],gaps:['JSON schema is validated by the supervisor after generation','Text generation has no autonomous tools or persistent agent sessions']}}; }
    else {
      if (!runtime || !['probe','start','resume','send','cancel','stop','status','approve'].includes(method)) throw new AdapterError('unsupported_method','Unsupported worker method');
      result = await runtime[method as keyof Runtime](params);
    }
    return result;
}
async function dispatch(value: Json) {
  const id = value.id;
  try {
    if (value.version !== 1 || !['string','number'].includes(typeof id) || typeof value.method !== 'string') throw new AdapterError('invalid_request','Expected version 1 JSONL request');
    const result=await execute(value.method,value.params || {},String(id));
    write({version:1,id,result});
  } catch (error) {
    const safe = error as {code?:string;message?:string};
    write({version:1,id:id ?? null,error:{code:safe.code || 'internal',message:safe.code ? safe.message : 'Agent worker request failed'}});
  }
}
const lines = createInterface({input:process.stdin});
lines.on('line', line => {
  if (line.length > 8 * 1024 * 1024) { write({version:1,id:null,error:{code:'invalid_request',message:'Request exceeds size limit'}}); return; }
  try { const value = JSON.parse(line); if (!value || typeof value !== 'object') throw new Error(); void dispatch(value); }
  catch { write({version:1,id:null,error:{code:'invalid_request',message:'Invalid JSON request'}}); }
});
lines.on('close', () => { for (const controller of jobs.values()) controller.abort(); if (name === 'gemini') void runtime?.stop({}); });
