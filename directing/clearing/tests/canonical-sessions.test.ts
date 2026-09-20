import fs from 'fs';
import os from 'os';
import path from 'path';
import { CanonicalSessions } from '../src/canonical-sessions';

let root: string;
let router: {ingest: jest.Mock};
let rendered: jest.Mock;
let tailers: CanonicalSessions[];
const role = 'wren'; const session = 'test-session';
function record(id=session, primary=true, state='idle') {
 fs.mkdirSync(path.join(root,'sessions/v2'),{recursive:true});
 fs.writeFileSync(path.join(root,`sessions/v2/${id}.json`),JSON.stringify({version:1,session_id:id,role,primary,state,heartbeat:new Date().toISOString()}));
}
function event(sequence: number, type: string, data: Record<string,unknown>={}, id=session) {
 return {version:1,session_id:id,event_id:`event-${sequence}`,sequence,type,data,timestamp:'2026-09-20T10:00:00Z'};
}
function append(value: ReturnType<typeof event>) {
 fs.mkdirSync(path.join(root,'agent-events'),{recursive:true});
 fs.appendFileSync(path.join(root,`agent-events/${value.session_id}.jsonl`),JSON.stringify(value)+'\n');
}
function tailer(replayExisting=false) {
 const t=new CanonicalSessions(router as any,rendered,{stateDir:root,replayExisting});tailers.push(t);return t;
}
beforeEach(()=>{root=fs.mkdtempSync(path.join(os.tmpdir(),'clearing-canonical-'));router={ingest:jest.fn()};rendered=jest.fn();tailers=[];});
afterEach(()=>{for(const t of tailers)t.stop();fs.rmSync(root,{recursive:true,force:true});});
test('explicit primary lease wins over newer child transcript, first binding starts at EOF',()=>{
 record();append(event(1,'message.completed',{text:'historical'}));
 record('newer-child',false);append(event(1,'message.completed',{text:'wrong child'},'newer-child'));
 const t=tailer();expect(t.poll(role)).toBe(true);expect(router.ingest).not.toHaveBeenCalled();
 append(event(2,'input.accepted',{kind:'human_input',text:'hello'}));
 append(event(3,'message.completed',{text:'status note'}));append(event(4,'message.completed',{text:'final reply'}));
 t.poll(role);expect(router.ingest.mock.calls.map(call=>call[0].text)).toEqual(['hello','status note']);
 append(event(5,'turn.completed',{usage:{input_tokens:null,output_tokens:null}}));t.poll(role);
 expect(router.ingest).toHaveBeenLastCalledWith(expect.objectContaining({text:'final reply',type:'role-response'}));
 expect(rendered).toHaveBeenCalledTimes(1);
});
test('persisted cursor replays downtime events and preserves pending final across restart',()=>{
 record();append(event(1,'session.started'));const first=tailer();first.poll(role);
 append(event(2,'message.completed',{text:'saved candidate'}));first.poll(role);first.stop();
 append(event(3,'turn.completed'));const second=tailer();second.poll(role);
 expect(router.ingest).toHaveBeenLastCalledWith(expect.objectContaining({text:'saved candidate',type:'role-response'}));
 second.poll(role);expect(rendered).toHaveBeenCalledTimes(1);
 const third=tailer();third.poll(role);expect(rendered).toHaveBeenCalledTimes(1);
});
test('partial UTF8 and trailing JSON record preserve exact byte offsets and event dedup',()=>{
 record();append(event(1,'session.started'));const t=tailer();t.poll(role);
 const first=Buffer.from(JSON.stringify(event(2,'input.accepted',{kind:'human_input',text:'café'}))+'\n');
 const next=Buffer.from(JSON.stringify(event(3,'message.completed',{text:'naïve 🪶'}))+'\n');
 const cut=next.indexOf(Buffer.from('🪶'))+2;
 fs.appendFileSync(path.join(root,`agent-events/${session}.jsonl`),Buffer.concat([first,next.subarray(0,cut)]));t.poll(role);
 expect(router.ingest.mock.calls[0][0].text).toBe('café');
 fs.appendFileSync(path.join(root,`agent-events/${session}.jsonl`),next.subarray(cut));
 append(event(3,'message.completed',{text:'naïve 🪶'}));append(event(4,'turn.completed'));t.poll(role);
 expect(router.ingest.mock.calls.map(call=>call[0].text)).toEqual(['café','naïve 🪶']);expect(rendered).toHaveBeenCalledTimes(1);
});
test('enrolled disconnected, missing journal, and ambiguous leases fail visibly without legacy fallback',()=>{
 record();const t=tailer();expect(t.poll(role)).toBe(true);expect(router.ingest.mock.calls[0][0].type).toBe('gap');
 append(event(1,'session.started'));record(session,true,'disconnected');expect(t.poll(role)).toBe(true);
 expect(router.ingest.mock.calls.at(-1)[0].text).toContain('disconnected');
 record();record('other-primary');expect(t.poll(role)).toBe(true);expect(router.ingest.mock.calls.at(-1)[0].text).toContain('unique');
 expect(t.poll('kade')).toBe(false);
});
test('failed turns never promote a final reply and sequence gaps are visible',()=>{
 record();append(event(1,'session.started'));const t=tailer();t.poll(role);
 append(event(2,'message.completed',{text:'still working'}));append(event(3,'turn.failed'));t.poll(role);
 expect(router.ingest.mock.calls.map(call=>call[0].type)).toEqual(['pm-thinking','gap']);expect(rendered).not.toHaveBeenCalled();
 append(event(5,'message.completed',{text:'missing sequence'}));t.poll(role);
 expect(router.ingest.mock.calls.at(-1)[0].text).toContain('sequence');
});
test('delta narration before a tool remains commentary and final chunks become one response',()=>{
 record();append(event(1,'session.started'));const t=tailer();t.poll(role);
 append(event(2,'message.delta',{text:'checking'}));append(event(3,'tool.started'));
 append(event(4,'message.delta',{text:'all '}));append(event(5,'message.delta',{text:'done'}));append(event(6,'turn.completed'));t.poll(role);
 expect(router.ingest.mock.calls.map(call=>[call[0].text,call[0].type])).toEqual([['checking','pm-thinking'],['all done','role-response']]);
});
test('saved enrollment prevents legacy fallback even if registry disappears before restart',()=>{
 record();append(event(1,'session.started'));const first=tailer();first.poll(role);first.stop();
 fs.rmSync(path.join(root,'sessions'),{recursive:true,force:true});
 const second=tailer();expect(second.poll(role)).toBe(true);
 expect(router.ingest.mock.calls.at(-1)[0].type).toBe('gap');
});
