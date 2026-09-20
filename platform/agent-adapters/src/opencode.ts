/* eslint-disable security/detect-object-injection -- API credential names must pass the environment identifier guard before lookup. */
import { isAbsolute } from 'node:path';
import { randomUUID } from 'node:crypto';
import { setTimeout as delay } from 'node:timers/promises';
import { AdapterError, deadline, Emit, Json, required, Runtime } from './types';

interface SessionState { state: string; turn?: string; messageID?: string; noTools: boolean; seen: Map<string,string>; approvals: Set<string>; }
/** HTTP contract pinned to OpenCode V2 docs 2026-09-20; deliberately rejects V1.
 * Uses projected messages, not volatile SSE, so telemetry is snapshot fidelity.
 */
export class OpenCodeRuntime implements Runtime {
  private endpoint = '';
  private config: Json = {};
  private sessions = new Map<string, SessionState>();
  constructor(private emit: Emit, private fetcher: typeof fetch = fetch) {}
  private configure(params: Json) {
    const endpoint = required(params.endpoint || this.endpoint,'endpoint');
    const url = new URL(endpoint);
    if (!['http:','https:'].includes(url.protocol) || url.username || url.password) throw new AdapterError('configuration','Invalid OpenCode endpoint');
    this.endpoint = endpoint.replace(/\/$/,''); this.config = params.config || this.config;
  }
  private async api(route: string, method = 'GET', body?: Json, timeoutMs = deadline(this.config)): Promise<Json> {
    const headers: Record<string,string> = {'content-type':'application/json'};
    if (this.config.api_key_env) {
      const name = required(this.config.api_key_env,'api_key_env');
      if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(name) || !process.env[name]) throw new AdapterError('configuration','Missing OpenCode credential environment reference');
      headers.authorization = `Bearer ${process.env[name]}`;
    }
    let response: Response;
    try { response = await this.fetcher(`${this.endpoint}${route}`, {method,headers,body:body ? JSON.stringify(body) : undefined,signal:AbortSignal.timeout(timeoutMs)}); }
    catch (error) { throw new AdapterError((error as Error).name === 'TimeoutError' ? 'timeout' : 'network','OpenCode request failed'); }
    if (!response.ok) { await response.body?.cancel(); throw new AdapterError(response.status === 401 || response.status === 403 ? 'authentication' : 'runtime',`OpenCode returned HTTP ${response.status}`); }
    if (response.status === 204) return {};
    try { return await response.json(); } catch { throw new AdapterError('protocol','Invalid OpenCode JSON'); }
  }
  async probe(params: Json) {
    this.configure(params);
    const info = await this.api('/api/info');
    if (typeof info.version !== 'string' || !/^2\./.test(info.version)) throw new AdapterError('unsupported_version','Expected OpenCode V2 server info');
    if (this.config.expected_version && info.version !== this.config.expected_version) throw new AdapterError('unsupported_version','OpenCode version differs from pinned configuration');
    return {runtime:'opencode',runtime_version:info.version,protocol:'opencode-v2-2026-09-20',capabilities:{resume:true,autonomous_wake:true,mid_turn_steering:false,cancellation:true,structured_output:false,before_tool:false,history_recovery:false,no_tools:true,context_boundaries:[],gaps:['Tool telemetry is projected snapshots, not a complete durable event stream','Pre-tool interception requires a separately verified plugin','Session permissions do not provide filesystem isolation','Version compatibility is experimental; pin expected_version']},native:info};
  }
  private state(session: string): SessionState {
    const state = this.sessions.get(session);
    if (!state) throw new AdapterError('not_found','Unknown OpenCode session');
    return state;
  }
  private model(value: unknown) {
    if (!value) return undefined;
    const model = required(value,'model'); const slash = model.indexOf('/');
    if (slash <= 0 || slash === model.length - 1) throw new AdapterError('configuration','OpenCode model must be provider/model');
    return {providerID:model.slice(0,slash),id:model.slice(slash+1)};
  }
  private async open(params: Json, resume: boolean) {
    this.configure(params);
    const cwd = required(params.cwd,'cwd');
    if (!isAbsolute(cwd)) throw new AdapterError('configuration','cwd must be absolute');
    const noTools = !!(params.no_tools || this.config.no_tools);
    const value=await this.openSession(params,cwd,noTools,resume);
    const session=required(value.data?.id,'OpenCode session id');
    if (value.data?.location?.directory!==cwd) throw new AdapterError('workspace_mismatch','OpenCode session workspace differs from requested cwd');
    if (noTools) this.confirmNoTools(value);
    this.sessions.set(session,{state:'idle',noTools,seen:new Map(),approvals:new Set()});
    await this.observe(session,true);
    this.emit('session.started',{resumed:resume},session);
    if (params.input) await this.send({...params,native_session_id:session});
    return {native_session_id:session,state:this.state(session).state};
  }
  private async openSession(params: Json, cwd: string, noTools: boolean, resume: boolean): Promise<Json> {
    if (resume) {
      const session=required(params.native_session_id,'native_session_id');
      const active=await this.api('/api/session/active');
      if (!active.data || typeof active.data!=='object' || Array.isArray(active.data)) throw new AdapterError('protocol','Invalid OpenCode active-session inventory');
      if (Object.hasOwn(active.data,session)) throw new AdapterError('busy','OpenCode session is still running; reconcile before resuming');
      return this.api(`/api/session/${encodeURIComponent(session)}`);
    }
    const permissions=noTools ? [{action:'*',resource:'*',effect:'deny'}] : [{action:'*',resource:'*',effect:'ask'},...(this.config.permissions || [])];
    return this.api('/api/session','POST',{location:{directory:cwd},...(params.model ? {model:this.model(params.model)} : {}),...(this.config.agent ? {agent:this.config.agent} : {}),permissions});
  }
  private confirmNoTools(value: Json) {
    const last=value.data.permissions?.at(-1);
    if (!last || last.action!=='*' || last.resource!=='*' || last.effect!=='deny') throw new AdapterError('unsupported_capability','OpenCode did not confirm deny-all session permissions');
  }
  start(params: Json) { return this.open(params,false); }
  resume(params: Json) { return this.open(params,true); }
  private async observe(session: string, baseline = false): Promise<Json[]> {
    const response = await this.api(`/api/session/${encodeURIComponent(session)}/context`);
    if (!Array.isArray(response.data)) throw new AdapterError('protocol','Invalid OpenCode context');
    const messages: Json[] = response.data;
    for (const message of messages) {
      if (message.type !== 'assistant') continue;
      for (const [index,part] of (message.content || []).entries()) this.observePart(session,message,part,index,baseline);
    }
    return messages;
  }
  private observePart(session: string, message: Json, part: Json, index: number, baseline: boolean) {
    const state=this.state(session); const key=`${message.id}:${part.id || index}`;
    const value=part.type==='text' ? part.text : JSON.stringify(part);
    const previous=state.seen.get(key); state.seen.set(key,value);
    if (baseline || value===previous) return;
    if (part.type==='text') {
      if (previous && !value.startsWith(previous)) this.emit('message.replaced',{message_id:message.id,text:value},session,state.turn);
      else this.emit('message.delta',{message_id:message.id,text:value.slice(previous?.length || 0)},session,state.turn);
    } else if (part.type==='tool') {
      let type=previous ? 'tool.updated' : 'tool.started';
      if (part.time?.completed) type='tool.completed';
      this.emit(type,part,session,state.turn);
    }
  }
  async send(params: Json) {
    const session = required(params.native_session_id,'native_session_id'); const state = this.state(session);
    if (state.state !== 'idle') throw new AdapterError('busy','OpenCode session is not idle');
    // Set state before await so concurrent requests cannot admit two turns.
    state.state = 'admitting'; state.turn = params.turn_id || randomUUID();
    try {
      const id = `msg_${state.turn!.replace(/[^a-zA-Z0-9_]/g,'_')}`;
      const admitted = await this.api(`/api/session/${encodeURIComponent(session)}/prompt`,'POST',{id,text:required(params.input,'input'),delivery:'queue',resume:true});
      state.messageID = required(admitted.data?.id,'admitted message id');
      state.state = 'running'; this.emit('turn.started',{native_message_id:admitted.data?.id},session,state.turn);
      void this.monitor(session);
      return {accepted:true,status:'accepted',native_session_id:session,turn_id:state.turn,native_message_id:admitted.data?.id};
    } catch (error) { state.state = 'idle'; throw error; }
  }
  private async monitor(session: string) {
    const state = this.state(session);
    const startedAt = Date.now();
    try {
      while (state.state === 'running') {
        if (Date.now() - startedAt > deadline(this.config)) throw new AdapterError('timeout','OpenCode turn timed out');
        const messages = await this.observe(session);
        if (this.completeTurn(session,messages)) return;
        await this.observePermissions(session);
        await delay(250);
      }
    } catch (error) {
      if (state.state === 'stopped') return;
      state.state = 'failed';
      try { await this.cancel({native_session_id:session}); } catch { /* remote state uncertain, supervisor reconciles */ }
      this.emit('turn.failed',{code:(error as AdapterError).code || 'runtime',message:'OpenCode observation failed; reconcile remote session before retry'},session,state.turn);
    }
  }
  private completeTurn(session: string, messages: Json[]): boolean {
    const state=this.state(session);
    // Only an idle marker after the exact admitted input proves completion.
    const admittedIndex=messages.findIndex(message=>message.id===state.messageID);
    const current=admittedIndex<0 ? [] : messages.slice(admittedIndex+1);
    const idle=current.find(message=>message.type==='idle');
    if (!idle) return false;
    const last=current.filter(message=>message.type==='assistant').at(-1);
    state.state='idle';
    this.emit(idle.outcome==='failed' ? 'turn.failed' : 'turn.completed',{finish_reason:idle.outcome || last?.finish || null,usage:last?.tokens || null},session,state.turn);
    return true;
  }
  private async observePermissions(session: string) {
    const state=this.state(session); const pending=await this.api(`/api/session/${encodeURIComponent(session)}/permission`);
    for (const permission of pending.data || []) {
      if (state.approvals.has(permission.id)) continue;
      state.approvals.add(permission.id); this.emit('approval.required',{...permission,request_id:permission.id},session,state.turn);
    }
  }
  async approve(params: Json) {
    const session = required(params.native_session_id,'native_session_id'); this.state(session);
    if (!['once','always','reject'].includes(params.decision)) throw new AdapterError('invalid_request','Invalid permission decision');
    await this.api(`/api/session/${encodeURIComponent(session)}/permission/${encodeURIComponent(required(params.request_id,'request_id'))}/reply`,'POST',{decision:params.decision});
    return {accepted:true};
  }
  async cancel(params: Json) {
    const session = required(params.native_session_id,'native_session_id');
    return this.api(`/api/session/${encodeURIComponent(session)}/interrupt?resume=false`,'POST',{});
  }
  async stop(params: Json) {
    const state = this.state(params.native_session_id);
    state.state = 'stopped'; const result = await this.cancel(params); this.emit('session.stopped',{},params.native_session_id); return result;
  }
  async status(params: Json) {
    const session = required(params.native_session_id,'native_session_id');
    const value = await this.api(`/api/session/${encodeURIComponent(session)}`);
    return {native_session_id:session,state:this.sessions.get(session)?.state || 'unknown',native:value.data};
  }
}
