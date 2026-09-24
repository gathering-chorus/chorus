/* eslint-disable security/detect-non-literal-fs-filename, security/detect-object-injection -- Paths are rooted in operator configuration; session filenames and cursor keys pass SAFE_ID validation. */
/** Canonical session projection. Selection is an explicit primary lease, never mtime. */
import fs from 'fs';
import os from 'os';
import path from 'path';
import { MessageRouter } from './router';
import { EmitSpine, renderedEvent } from './reply-delivery';

type Text = { text: string; ts: string };
interface Lease { version: number; session_id: string; role: string; primary: boolean; state: string; heartbeat: string }
interface AgentEvent { version: number; session_id: string; event_id: string; sequence: number; type: string; timestamp?: string; data?: {kind?: string;text?: string} }
type Cursor = { role?: string; offset: number; sequence: number; pending?: Text; stream?: Text };
type Binding = Cursor & { session: string; file: string; watcher?: fs.FSWatcher };
export type CanonicalOptions = { stateDir?: string; cursorPath?: string; replayExisting?: boolean; maxLeaseAgeMs?: number };
const SAFE_ID = /^[a-zA-Z0-9_-]{1,200}$/;
const MAX_RECORD = 8 * 1024 * 1024;

export class CanonicalSessions {
  private root: string;
  private cursorFile: string;
  private cursors: Partial<Record<string, Cursor>> = Object.create(null);
  private bindings = new Map<string, Binding>();
  private enrolled = new Set<string>();
  private gaps = new Map<string, string>();
  private cursorError = false;
  private registryWatcher?: fs.FSWatcher;
  private replay: boolean;
  private maxAge: number;
  private stopped = false;
  constructor(private router: MessageRouter, private rendered: EmitSpine, options: CanonicalOptions = {}) {
    this.root = options.stateDir || process.env.CHORUS_AGENT_STATE_DIR || path.join(os.homedir(), '.chorus');
    this.cursorFile = options.cursorPath || process.env.CLEARING_AGENT_CURSOR_FILE || path.join(this.root, 'agent-event-cursors/clearing.json');
    this.replay = options.replayExisting ?? process.env.CLEARING_AGENT_REPLAY_HISTORY === '1';
    this.maxAge = options.maxLeaseAgeMs ?? Number(process.env.CLEARING_SESSION_LEASE_MS || 0);
    this.loadCursors();
  }
  private loadCursors() {
    try {
      const value = JSON.parse(fs.readFileSync(this.cursorFile, 'utf8'));
      if (value.version !== 1 || !value.sessions || typeof value.sessions !== 'object') throw new Error();
      for (const [id, cursor] of Object.entries(value.sessions)) {
        const entry = cursor as Cursor;
        if (!SAFE_ID.test(id) || !Number.isSafeInteger(entry.offset) || entry.offset < 0 || !Number.isSafeInteger(entry.sequence) || entry.sequence < 0) throw new Error();
        this.cursors[id] = entry;
        if (typeof entry.role === 'string') this.enrolled.add(entry.role);
      }
    } catch (error) { if ((error as NodeJS.ErrnoException).code !== 'ENOENT') this.cursorError = true; }
  }
  private gap(role: string, reason: string) {
    if (this.gaps.get(role) === reason) return;
    this.gaps.set(role, reason);
    this.router.ingest({from:role,type:'gap',text:`Session conversation unavailable: ${reason}`,ts:new Date().toISOString()});
  }
  private save(role: string, state: Binding) {
    this.cursors[state.session] = {role,offset:state.offset,sequence:state.sequence,pending:state.pending,stream:state.stream};
    const temp = `${this.cursorFile}.${process.pid}.tmp`;
    try {
      fs.mkdirSync(path.dirname(this.cursorFile), {recursive:true,mode:0o700});
      fs.writeFileSync(temp, JSON.stringify({version:1,sessions:this.cursors}), {mode:0o600});
      fs.renameSync(temp, this.cursorFile);
    } catch { this.gap(role, 'canonical cursor could not be saved; restart replay may duplicate messages'); }
  }
  /** true means enrolled: caller must not fall back to a legacy provider transcript. */
  poll(role: string): boolean {
    if (this.stopped) return this.enrolled.has(role);
    if (this.cursorError) { this.unbind(role); this.gap(role,'canonical cursor is invalid; repair it before resuming projection'); return true; }
    const directory = path.join(this.root, 'sessions/v2');
    let records: Lease[];
    try {
      const files = fs.readdirSync(directory).filter(file => file.endsWith('.json'));
      records = files.map(file => {
        const value = JSON.parse(fs.readFileSync(path.join(directory,file),'utf8'));
        if (value.version !== 1 || !SAFE_ID.test(value.session_id) || file !== `${value.session_id}.json` || typeof value.role !== 'string') throw new Error();
        return value;
      });
      if (!this.registryWatcher) {
        try { this.registryWatcher = fs.watch(directory, () => { for (const known of this.enrolled) this.poll(known); }); this.registryWatcher.unref(); } catch { /* poll still observes leases */ }
      }
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code === 'ENOENT' && !this.enrolled.has(role)) return false;
      this.unbind(role); this.gap(role, 'canonical registry is missing or invalid'); return true;
    }
    const lease = this.primary(role, records);
    if (lease === false) return false;
    if (!lease) return true;
    this.follow(role,lease);
    return true;
  }
  private primary(role: string, records: Lease[]): Lease | false | undefined {
    const roleRecords=records.filter(record=>record.role===role);
    if (!roleRecords.length && !this.enrolled.has(role)) return false;
    this.enrolled.add(role);
    const primary=roleRecords.filter(record=>record.primary && !['stopped','failed'].includes(record.state));
    if (primary.length!==1) { this.unbind(role); this.gap(role,'role has no unique live primary session'); return; }
    const lease=primary[0];
    if (!['idle','running','awaiting_approval'].includes(lease.state)) { this.unbind(role); this.gap(role,'primary session is disconnected; explicit resume is required'); return; }
    if (this.maxAge>0 && (!Number.isFinite(Date.parse(lease.heartbeat)) || Date.now()-Date.parse(lease.heartbeat)>this.maxAge)) {
      this.unbind(role); this.gap(role,'primary session lease has expired'); return;
    }
    return lease;
  }
  private follow(role: string, lease: Lease) {
    const file=path.join(this.root,'agent-events',`${lease.session_id}.jsonl`);
    try {
      const stats=fs.statSync(file); if (!stats.isFile() || stats.size===0) throw new Error();
      let state=this.bindings.get(role);
      if (state?.session!==lease.session_id) {
        this.unbind(role);
        const cursor=this.cursors[lease.session_id] || this.bootstrap(file,lease.session_id);
        state={...cursor,session:lease.session_id,file};
        this.watchFile(role,state);
        this.bindings.set(role,state); this.save(role,state);
      }
      if (this.read(role,state!)) this.gaps.delete(role);
    } catch { this.unbind(role); this.gap(role,'primary session event journal is missing or invalid'); }
  }
  private watchFile(role: string, state: Binding) {
    try { state.watcher=fs.watch(state.file,()=>this.poll(role)); state.watcher.unref(); } catch { /* poll fallback */ }
  }
  private bootstrap(file: string, session: string): Cursor {
    if (this.replay) return {offset:0,sequence:0};
    const size = fs.statSync(file).size;
    if (!size) return {offset:0,sequence:0};
    const fd = fs.openSync(file,'r'); const length = Math.min(size,MAX_RECORD+1); const buf = Buffer.alloc(length);
    try { fs.readSync(fd,buf,0,length,size-length); } finally { fs.closeSync(fd); }
    const last = buf.lastIndexOf(10);
    if (last < 0) { if (size > MAX_RECORD) throw new Error(); return {offset:0,sequence:0}; }
    const previous = buf.lastIndexOf(10,last-1);
    const event = JSON.parse(buf.subarray(previous+1,last).toString('utf8'));
    if (event.version !== 1 || event.session_id !== session || !SAFE_ID.test(event.event_id) || !Number.isSafeInteger(event.sequence) || event.sequence < 1) throw new Error();
    return {offset:size-length+last+1,sequence:event.sequence};
  }
  private unbind(role: string) { this.bindings.get(role)?.watcher?.close(); this.bindings.delete(role); }
  count(): number { return this.bindings.size; }
  stop() { this.stopped=true; this.registryWatcher?.close(); for (const role of this.bindings.keys()) this.unbind(role); }
  private read(role: string, state: Binding): boolean {
    const size = fs.statSync(state.file).size;
    if (size < state.offset) { this.gap(role,'canonical journal was truncated; cursor recovery is required'); return false; }
    if (size === state.offset) return true;
    const length = Math.min(size-state.offset,MAX_RECORD+1); const bytes = Buffer.alloc(length); const fd = fs.openSync(state.file,'r');
    let count: number;
    try { count = fs.readSync(fd,bytes,0,length,state.offset); } finally { fs.closeSync(fd); }
    const buffer = bytes.subarray(0,count); const last = buffer.lastIndexOf(10);
    if (last < 0) { if (count > MAX_RECORD) this.gap(role,'canonical event exceeds size limit'); return count <= MAX_RECORD; }
    // Split bytes first, decode only complete records: a torn UTF-8 sequence is retained.
    if (!this.readRecords(role,state,buffer,last)) return false;
    this.save(role,state);
    if (state.offset < size && count === MAX_RECORD+1) setImmediate(() => { if (this.bindings.get(role)===state) this.poll(role); });
    return true;
  }
  private readRecords(role: string, state: Binding, buffer: Buffer, last: number): boolean {
    let begin=0;
    while (begin<=last) {
      const end=buffer.indexOf(10,begin); const line=buffer.subarray(begin,end); begin=end+1;
      if (!line.length) { state.offset+=1; continue; }
      const event=this.decodeEvent(role,state,line);
      if (!event) return false;
      if (event.sequence>state.sequence) { this.project(role,state,event); state.sequence=event.sequence; }
      state.offset+=line.length+1;
    }
    return true;
  }
  private decodeEvent(role: string, state: Binding, line: Buffer): AgentEvent | undefined {
    let event: AgentEvent;
    try { event=JSON.parse(line.toString('utf8')); } catch { this.gap(role,'canonical event is invalid JSON'); return; }
    if (event.version!==1 || event.session_id!==state.session || !SAFE_ID.test(event.event_id) || !Number.isSafeInteger(event.sequence) || event.sequence<1) { this.gap(role,'canonical event identity is invalid'); return; }
    if (event.sequence>state.sequence+1) { this.gap(role,'canonical event sequence has a gap'); return; }
    return event;
  }
  private interim(role: string, value?: Text) { if (value?.text.trim()) this.router.ingest({from:role,type:'pm-thinking',...value}); }
  private candidate(role: string, state: Binding, value: Text) {
    if (state.pending && state.pending.text !== value.text) this.interim(role,state.pending);
    state.pending=value;
  }
  private project(role: string, state: Binding, event: AgentEvent) {
    const data=event.data || {}; const ts=event.timestamp || new Date().toISOString();
    switch (event.type) {
      case 'input.accepted':
        if (data.kind==='human_input' && typeof data.text==='string') {
          this.flushInterim(role,state);
          this.router.ingest({from:'jeff',type:'jeff-input',text:data.text,ts});
        } break;
      case 'message.delta': case 'message.replaced': case 'message.completed':
        this.projectText(role,state,event.type,data.text,ts); break;
      case 'tool.started': this.flushInterim(role,state); break;
      case 'turn.completed': this.finish(role,state,data.text,ts); break;
      case 'turn.failed': case 'session.disconnected':
        this.flushInterim(role,state); this.gap(role,'agent turn interrupted; no final reply is confirmed'); break;
      default: break;
    }
  }
  private flushInterim(role: string, state: Binding) {
    this.interim(role,state.pending); this.interim(role,state.stream); state.pending=undefined; state.stream=undefined;
  }
  private projectText(role: string, state: Binding, type: string, text: unknown, ts: string) {
    if (typeof text!=='string') return;
    if (type==='message.delta') state.stream={text:(state.stream?.text || '')+text,ts};
    else if (type==='message.replaced') state.stream={text,ts};
    else if (text.trim()) { state.stream=undefined; this.candidate(role,state,{text,ts}); }
  }
  private finish(role: string, state: Binding, text: unknown, ts: string) {
    if (state.stream?.text) this.candidate(role,state,state.stream);
    if (typeof text==='string' && text.trim()) this.candidate(role,state,{text,ts});
    if (state.pending) {
      this.router.ingest({from:role,type:'role-response',...state.pending}); this.rendered(renderedEvent(role,state.pending.text));
    } else this.gap(role,'turn completed without assistant text evidence after the saved cursor');
    state.pending=undefined; state.stream=undefined;
  }
}
