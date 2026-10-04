// @domain: roles
import { createInterface } from 'node:readline';
import { GeminiRuntime } from './gemini';
import { OpenCodeRuntime } from './opencode';
import { AdapterError, Json, required, Runtime } from './types';

const write = (value: Json) => process.stdout.write(JSON.stringify(value) + '\n');
const emit = (type: string, data: Json = {}, native_session_id?: string, turn_id?: string) => write({version:1,method:'event',params:{type,native_session_id,turn_id,data}});
const name = process.argv[process.argv.indexOf('--runtime') + 1];
let runtime: Runtime | undefined;
if (name === 'gemini') runtime = new GeminiRuntime(emit);
else if (name === 'opencode') runtime = new OpenCodeRuntime(emit);
else if (name === 'api') { process.stderr.write('api mode waits on the Clearing provider decision (#4424); use --runtime opencode|gemini\n'); process.exit(2); }
else { process.stderr.write('Expected --runtime opencode|gemini\n'); process.exit(2); }

async function execute(method: string, params: Json, id: string): Promise<Json> {
    let result: Json;
    {
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
lines.on('close', () => { if (name === 'gemini') void runtime?.stop({}); });
