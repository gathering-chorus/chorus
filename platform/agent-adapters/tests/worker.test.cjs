// @domain: roles
const {test}=require('node:test');
const assert=require('node:assert/strict');
const {spawn}=require('node:child_process');
const {createInterface}=require('node:readline');
const path=require('node:path');
const {setTimeout:delay}=require('node:timers/promises');
function worker(t,runtime='gemini',env={}) {
 const child=spawn(process.execPath,[path.join(__dirname,'../dist/worker.js'),'--runtime',runtime],{stdio:'pipe',env:{...process.env,...env}});
 const records=[]; let diagnostics='';
 child.stderr.on('data',chunk=>diagnostics+=chunk);
 createInterface({input:child.stdout}).on('line',line=>records.push(JSON.parse(line)));
 t.after(()=>child.kill());
 const send=value=>child.stdin.write(typeof value==='string'?value+'\n':JSON.stringify(value)+'\n');
 async function response(id){for(let i=0;i<200;i++){const result=records.find(value=>value.id===id);if(result)return result;await delay(10);}throw Error('timeout '+diagnostics);}
 return {send,response,records,child};
}
// #4424 — CLI mode only: the api/text-job mode needs Clearing's text
// generation, which waits on Jeff's call. It refuses loudly, never half-runs.
test('JSONL worker rejects malformed/versioned requests; api mode and run are refused',async t=>{
 const peer=worker(t,'gemini');peer.send('bad-json'); assert.equal((await peer.response(null)).error.code,'invalid_request');
 peer.send({version:2,id:'wrong',method:'probe'});assert.equal((await peer.response('wrong')).error.code,'invalid_request');
 peer.send({version:1,id:'job',method:'run',params:{input:'hello',no_tools:true}});assert.equal((await peer.response('job')).error.code,'unsupported_method');
 const api=spawn(process.execPath,[path.join(__dirname,'../dist/worker.js'),'--runtime','api'],{stdio:'pipe'});
 let err='';api.stderr.on('data',c=>err+=c);
 const code=await new Promise(r=>api.on('exit',r));
 assert.equal(code,2); assert.match(err,/api mode waits on the Clearing/);
});
test('JSONL worker remains responsive during pending Gemini prompt and approval',async t=>{
 const peer=worker(t,'gemini');
 peer.send({version:1,id:'start',method:'start',params:{cwd:process.cwd(),config:{command:process.execPath,args:[path.join(__dirname,'gemini-fixture.cjs')],timeout_ms:2000}}});
 const session=(await peer.response('start')).result.native_session_id;
 peer.send({version:1,id:'send',method:'send',params:{native_session_id:session,input:'permission'}});
 assert.equal((await peer.response('send')).result.status,'accepted');
 peer.send({version:1,id:'status',method:'status',params:{native_session_id:session}});assert.equal((await peer.response('status')).result.state,'running');
 peer.send({version:1,id:'stop',method:'stop',params:{native_session_id:session}});assert.equal((await peer.response('stop')).result.requested,true);
});

test('Gemini Chorus MCP gets authoritative session refs on start/resume without static spoofing',async t=>{
 const authority={CHORUS_SESSION_ID:'canonical-session',CHORUS_ROLE:'wren',DEPLOY_ROLE:'wren',CHORUS_SESSION_TOKEN_FILE:'/tmp/session-token-ref',CHORUS_MCP_IDENTITY_MODE:'strict',CHORUS_AGENT_SOCKET:'/tmp/agent.sock',CHORUS_AGENT_STATE_DIR:'/tmp/agent-state',CHORUS_API_URL:'http://127.0.0.1:3340'};
 const peer=worker(t,'gemini',{...authority,CHORUS_IDENTITY_TOKEN:'AMBIENT_BEARER_MUST_NOT_CROSS'});
 const other={name:'unrelated',command:'other-mcp',args:[],env:[{name:'CHORUS_ROLE',value:'independent-config'}]};
 const configured=[{name:'chorus-api',command:process.execPath,args:['/example/main-stdio.js'],env:[...Object.keys(authority).map(name=>({name,value:'STATIC_SPOOF'})),{name:'CHORUS_IDENTITY_TOKEN',value:'STATIC_BEARER_MUST_NOT_CROSS'},{name:'UNRELATED_OPTION',value:'preserved'}]},other];
 const params={cwd:process.cwd(),config:{command:process.execPath,args:[path.join(__dirname,'gemini-fixture.cjs')],timeout_ms:2000,mcp_servers:configured}};
 peer.send({version:1,id:'mcp-start',method:'start',params});
 const started=await peer.response('mcp-start');assert.ok(started.result);
 peer.send({version:1,id:'mcp-resume',method:'resume',params:{...params,native_session_id:started.result.native_session_id}});
 assert.ok((await peer.response('mcp-resume')).result);
 const configs=peer.records.filter(record=>record.method==='event'&&record.params.data.sessionUpdate==='fixture_mcp_config').map(record=>record.params.data);
 assert.deepEqual(configs.map(value=>value.method),['session/new','session/load']);
 for(const value of configs) {
  const chorus=value.mcpServers[0];
  assert.deepEqual(Object.fromEntries(chorus.env.map(entry=>[entry.name,entry.value])),{UNRELATED_OPTION:'preserved',...authority});
  assert.equal(chorus.env.length,Object.keys(authority).length+1);
  assert.deepEqual(value.mcpServers[1],other);
  assert.equal(JSON.stringify(chorus).includes('BEARER_MUST_NOT_CROSS'),false);
  assert.equal(JSON.stringify(chorus).includes('STATIC_SPOOF'),false);
 }
 assert.equal(configured[0].env[0].value,'STATIC_SPOOF');
 peer.send({version:1,id:'mcp-stop',method:'stop'});await peer.response('mcp-stop');
});

test('Gemini refuses Chorus MCP static identity when supervisor identity is unavailable',async t=>{
 const peer=worker(t,'gemini',{CHORUS_SESSION_ID:'',CHORUS_ROLE:'',DEPLOY_ROLE:'',CHORUS_SESSION_TOKEN_FILE:''});
 peer.send({version:1,id:'missing-identity',method:'start',params:{cwd:process.cwd(),config:{command:process.execPath,args:[path.join(__dirname,'gemini-fixture.cjs')],mcp_servers:[{name:'chorus-api',command:'node',args:[],env:[{name:'CHORUS_SESSION_TOKEN_FILE',value:'/tmp/spoof'}]}]}}});
 assert.equal((await peer.response('missing-identity')).error.code,'invalid_request');
});
