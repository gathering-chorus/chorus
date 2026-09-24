import { spawn, ChildProcessWithoutNullStreams } from 'node:child_process';
import { isAbsolute } from 'node:path';
import { createInterface } from 'node:readline';
import { AdapterError, deadline, Emit, environment, Json, required, Runtime } from './types';

const CHORUS_MCP_RESERVED = new Set([
  'CHORUS_SESSION_ID','CHORUS_ROLE','DEPLOY_ROLE','CHORUS_SESSION_TOKEN_FILE','CHORUS_MCP_IDENTITY_MODE',
  'CHORUS_AGENT_SOCKET','CHORUS_AGENT_STATE_DIR','CHORUS_API_URL','CHORUS_IDENTITY_TOKEN',
]);

function chorusMcpEnvironment(): Array<{name: string;value: string}> {
  const tokenFile=required(process.env.CHORUS_SESSION_TOKEN_FILE,'supervisor CHORUS_SESSION_TOKEN_FILE');
  if (!isAbsolute(tokenFile)) throw new AdapterError('configuration','Supervisor session token file must be absolute');
  const values=[
    {name:'CHORUS_SESSION_ID',value:required(process.env.CHORUS_SESSION_ID,'supervisor CHORUS_SESSION_ID')},
    {name:'CHORUS_ROLE',value:required(process.env.CHORUS_ROLE,'supervisor CHORUS_ROLE')},
    {name:'DEPLOY_ROLE',value:required(process.env.DEPLOY_ROLE,'supervisor DEPLOY_ROLE')},
    {name:'CHORUS_SESSION_TOKEN_FILE',value:tokenFile},
    {name:'CHORUS_MCP_IDENTITY_MODE',value:'strict'},
  ];
  for (const [name,value] of [
    ['CHORUS_AGENT_SOCKET',process.env.CHORUS_AGENT_SOCKET],
    ['CHORUS_AGENT_STATE_DIR',process.env.CHORUS_AGENT_STATE_DIR],
    ['CHORUS_API_URL',process.env.CHORUS_API_URL],
  ]) if (name && value) values.push({name,value});
  return values;
}

/** ACP MCP env is explicit; only the designated Chorus server gets session identity. */
function sessionMcpServers(servers: Json[]): Json[] {
  if (!Array.isArray(servers)) throw new AdapterError('configuration','mcp_servers must be an array');
  return servers.map(server => {
    if (server.name!=='chorus-api') return server;
    if (typeof server.command!=='string') throw new AdapterError('configuration','chorus-api MCP must use the local stdio transport');
    const configured=server.env ?? [];
    if (!Array.isArray(configured) || configured.some(value => typeof value?.name!=='string' || typeof value?.value!=='string')) throw new AdapterError('configuration','chorus-api MCP env must contain name/value strings');
    // Discard static identity and bearer-token entries; only file references cross ACP.
    const env=configured.filter(value => !CHORUS_MCP_RESERVED.has(value.name));
    return {...server,env:[...env,...chorusMcpEnvironment()]};
  });
}

/** ACP v1 (2026-09-20). No implied filesystem sandbox or universal hooks. */
export class GeminiRuntime implements Runtime {
  private child?: ChildProcessWithoutNullStreams;
  private nextId = 1;
  private pending = new Map<number, {resolve: (value: Json) => void; reject: (error: Error) => void; timer: NodeJS.Timeout}>();
  private permissions = new Map<string, { id: string | number; params: Json }>();
  private sessions = new Map<string, {state: string; turn?: string}>();
  private initialized?: Json;
  private timeout = 60000;
  private launchModel?: string;
  constructor(private emit: Emit) {}
  private write(value: Json) {
    if (!this.child || this.child.killed) throw new AdapterError('disconnected', 'Gemini process is unavailable');
    this.child.stdin.write(JSON.stringify(value) + '\n');
  }
  private request(method: string, params: Json): Promise<Json> {
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => { this.pending.delete(id); reject(new AdapterError('timeout', `Gemini ${method} timed out`)); }, this.timeout);
      this.pending.set(id, {resolve, reject, timer});
      try { this.write({jsonrpc:'2.0',id,method,params}); } catch (error) { clearTimeout(timer); this.pending.delete(id); reject(error); }
    });
  }
  private fail(error: Error) {
    for (const pending of this.pending.values()) { clearTimeout(pending.timer); pending.reject(error); }
    this.pending.clear(); this.permissions.clear(); this.initialized = undefined;
    for (const [session, state] of this.sessions) { state.state = 'stopped'; this.emit('session.stopped', {reason:error.message}, session); }
  }
  private receive(value: Json) {
    if (value.method === 'session/update') { this.receiveUpdate(value); return; }
    if (value.method === 'session/request_permission' && value.id !== undefined) {
      const params = value.params || {};
      const key = String(value.id);
      this.permissions.set(key, {id:value.id,params});
      this.emit('approval.required', {...params, request_id:key}, params.sessionId, this.sessions.get(params.sessionId)?.turn);
      return;
    }
    if (value.method && value.id !== undefined) {
      // Do not advertise proxy services we cannot mediate; never silently execute them.
      this.write({jsonrpc:'2.0',id:value.id,error:{code:-32601,message:'Client method not supported'}}); return;
    }
    const pending = this.pending.get(value.id);
    if (!pending) return;
    clearTimeout(pending.timer); this.pending.delete(value.id);
    if (value.error) pending.reject(new AdapterError('runtime', 'Gemini rejected the RPC request'));
    else pending.resolve(value.result || {});
  }
  private receiveUpdate(value: Json) {
    const session=value.params?.sessionId;
    const update=value.params?.update || {};
    const turn=this.sessions.get(session)?.turn;
    if (update.sessionUpdate==='agent_message_chunk' && update.content?.type==='text') {
      this.emit('message.delta',{text:update.content.text},session,turn); return;
    }
    let type='runtime.event';
    if (update.sessionUpdate==='tool_call') type='tool.started';
    else if (update.sessionUpdate==='tool_call_update') type=['completed','failed'].includes(update.status) ? 'tool.completed' : 'tool.updated';
    this.emit(type,update,session,turn);
  }
  private async connect(params: Json) {
    const config = params.config || {};
    if (config.no_tools || params.no_tools) throw new AdapterError('unsupported_capability', 'Gemini ACP cannot guarantee no-tools jobs; use the text API worker');
    if (this.initialized) {
      if (params.model && params.model !== this.launchModel) throw new AdapterError('configuration','Gemini model differs from the launched worker; launch a new worker with this model');
      return;
    }
    this.launch(params,config);
    try {
      const result = await this.request('initialize', {protocolVersion:1,clientCapabilities:{},clientInfo:{name:'chorus',version:'0.1.0'}});
      if (result.protocolVersion !== 1) throw new AdapterError('unsupported_version','Gemini did not negotiate ACP v1');
      this.initialized = result;
    } catch (error) { this.child?.kill(); throw error; }
  }
  private launch(params: Json, config: Json) {
    const cwd = required(params.cwd, 'cwd');
    if (!isAbsolute(cwd)) throw new AdapterError('configuration', 'cwd must be absolute');
    this.timeout = deadline(config);
    this.launchModel = params.model;
    const command = config.command || 'gemini';
    const args = config.args || ['--acp'];
    if (!Array.isArray(args) || args.some((arg: unknown) => typeof arg !== 'string')) throw new AdapterError('configuration', 'args must be strings');
    this.child = spawn(command, [...args, ...(params.model ? ['--model', required(params.model,'model')] : [])], {cwd, env:environment(config.env_refs), stdio:'pipe', shell:false});
    this.child.stderr.on('data', () => { /* Child diagnostics can contain credentials; do not relay raw stderr. */ });
    const lines = createInterface({input:this.child.stdout});
    lines.on('line', line => {
      try { if (line.length > 8 * 1024 * 1024) throw new Error(); this.receive(JSON.parse(line)); }
      catch { this.fail(new AdapterError('protocol','Invalid Gemini JSON-RPC output')); this.child?.kill(); }
    });
    this.child.once('error', () => this.fail(new AdapterError('unavailable','Unable to launch Gemini')));
    this.child.once('exit', () => { lines.close(); this.fail(new AdapterError('disconnected','Gemini process exited')); this.child = undefined; });
  }
  async probe(params: Json) {
    await this.connect(params);
    return {runtime:'gemini',runtime_version:this.initialized?.agentInfo?.version || 'unknown',protocol:'acp-v1',capabilities:{resume:!!this.initialized?.agentCapabilities?.loadSession,autonomous_wake:true,mid_turn_steering:false,cancellation:true,structured_output:false,before_tool:false,history_recovery:false,no_tools:false,context_boundaries:[],gaps:['ACP does not guarantee pre-tool interception or filesystem isolation','No-tools jobs are unsupported','Resume requires the agent loadSession capability; history replay may be incomplete']},native:this.initialized};
  }
  private async open(params: Json, resume: boolean) {
    if (!isAbsolute(required(params.cwd,'cwd'))) throw new AdapterError('configuration','cwd must be absolute');
    const mcpServers=sessionMcpServers(params.config?.mcp_servers || []);
    await this.connect(params);
    if (resume && !this.initialized?.agentCapabilities?.loadSession) throw new AdapterError('unsupported_capability','Gemini does not support session/load');
    const result = await this.request(resume ? 'session/load' : 'session/new', {cwd:required(params.cwd,'cwd'), mcpServers,
      ...(resume ? {sessionId:required(params.native_session_id,'native_session_id')} : {})});
    const session = resume ? params.native_session_id : required(result.sessionId,'sessionId');
    this.sessions.set(session,{state:'idle'});
    this.emit('session.started',{resumed:resume},session);
    if (params.input) await this.send({...params,native_session_id:session});
    return {native_session_id:session,state:this.sessions.get(session)?.state};
  }
  start(params: Json) { return this.open(params,false); }
  resume(params: Json) { return this.open(params,true); }
  // eslint-disable-next-line @typescript-eslint/require-await -- Promise interface keeps admission/validation failures asynchronous.
  async send(params: Json) {
    const session = required(params.native_session_id,'native_session_id');
    const state = this.sessions.get(session);
    if (!state) throw new AdapterError('not_found','Unknown Gemini session');
    if (state.state !== 'idle') throw new AdapterError('busy','Gemini session is not idle');
    const input = required(params.input,'input');
    state.state = 'running'; state.turn = params.turn_id;
    this.emit('turn.started',{},session,state.turn);
    void this.request('session/prompt',{sessionId:session,prompt:[{type:'text',text:input}]}).then(result => {
      state.state = 'idle'; this.emit('turn.completed',{finish_reason:result.stopReason ?? null,usage:null},session,state.turn);
    }).catch(async error => {
      state.state = 'failed';
      try { await this.cancel({native_session_id:session}); } catch { /* process may already be gone */ }
      this.emit('turn.failed',{code:error.code || 'runtime',message:error.message},session,state.turn);
    });
    return {accepted:true,status:'accepted',native_session_id:session,turn_id:state.turn};
  }
  // eslint-disable-next-line @typescript-eslint/require-await -- Promise interface keeps admission/validation failures asynchronous.
  async approve(params: Json) {
    const key = required(params.request_id,'request_id');
    const permission = this.permissions.get(key);
    if (!permission) throw new AdapterError('not_found','Unknown permission request');
    const option = (permission.params.options || []).find((value: Json) => value.optionId === params.option_id);
    if (params.option_id && !option) throw new AdapterError('invalid_request','Unknown permission option');
    this.write({jsonrpc:'2.0',id:permission.id,result:{outcome:option ? {outcome:'selected',optionId:option.optionId} : {outcome:'cancelled'}}});
    this.permissions.delete(key); return {accepted:true};
  }
  async cancel(params: Json) {
    const session = required(params.native_session_id,'native_session_id');
    for (const [request_id, permission] of this.permissions) if (permission.params.sessionId === session) await this.approve({request_id});
    this.write({jsonrpc:'2.0',method:'session/cancel',params:{sessionId:session}});
    return {requested:true};
  }
  async stop(_params: Json) {
    const child = this.child;
    if (child && child.exitCode === null) await new Promise<void>(resolve => {
      const timer = setTimeout(() => child.kill('SIGKILL'),2000);
      child.once('exit',() => { clearTimeout(timer); resolve(); });
      child.kill('SIGTERM');
    });
    return {requested:true};
  }
  // eslint-disable-next-line @typescript-eslint/require-await -- Promise interface keeps admission/validation failures asynchronous.
  async status(params: Json) {
    const session = required(params.native_session_id,'native_session_id');
    const state = this.sessions.get(session);
    if (!state) throw new AdapterError('not_found','Unknown Gemini session');
    return {native_session_id:session,...state};
  }
}
