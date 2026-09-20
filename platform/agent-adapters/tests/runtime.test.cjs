const {test} = require('node:test');
const assert = require('node:assert/strict');
const path = require('node:path');
const {setTimeout:delay} = require('node:timers/promises');
const {GeminiRuntime} = require('../dist/gemini.js');
const {OpenCodeRuntime} = require('../dist/opencode.js');
async function until(fn) { for(let n=0;n<100;n++){ if(fn())return; await delay(10); } throw Error('event timeout'); }
const launch={cwd:process.cwd(),config:{command:process.execPath,args:[path.join(__dirname,'gemini-fixture.cjs')],timeout_ms:2000}};

test('Gemini ACP negotiates, sends, reports output, resumes, and cancels approval',async t=>{
 const events=[]; const runtime=new GeminiRuntime((type,data,session)=>events.push({type,data,session}));
 t.after(()=>runtime.stop({}));
 const probe=await runtime.probe(launch); assert.equal(probe.capabilities.resume,true); assert.equal(probe.capabilities.no_tools,false);
 const started=await runtime.start(launch);assert.equal(started.native_session_id,'gemini-test-session');
 const session={native_session_id:started.native_session_id};
 const sent=await runtime.send({...session,input:'hi',turn_id:'one'}); assert.equal(sent.status,'accepted');
 await until(()=>events.some(e=>e.type==='turn.completed'));assert.equal(events.find(e=>e.type==='message.delta').data.text,'hello');
 await runtime.resume({...launch,...session});
 await runtime.send({...session,input:'permission'});
 await until(()=>events.some(e=>e.type==='approval.required'));
 assert.equal((await runtime.status(session)).state,'running');
 await assert.rejects(()=>runtime.approve({request_id:'900',option_id:'invented'}),{code:'invalid_request'});
 await runtime.cancel(session);
 await until(()=>events.filter(e=>e.type==='turn.completed').length===2);
 assert.equal(events.filter(e=>e.type==='turn.completed').at(-1).data.finish_reason,'cancelled');
});
test('Gemini rejects no-tools before launching a process',async()=>{
 const runtime=new GeminiRuntime(()=>{});
 await assert.rejects(()=>runtime.start({...launch,no_tools:true}),{code:'unsupported_capability'});
});
test('OpenCode v2 wire contracts, deny-all verification, polling output and idle outcome',async()=>{
 const requests=[], events=[];let started=false, permissions, admittedID;
 const fetcher=async(url,init)=>{
  const route=new URL(url).pathname; const body=init.body?JSON.parse(init.body):undefined;requests.push({route,method:init.method,body});
  let data;
  if(route==='/api/info') data={version:'2.0.0'};
  else if(route==='/api/session'&&init.method==='POST'){permissions=body.permissions;data={data:{id:'ses_test',location:{directory:process.cwd()},permissions}};}
  else if(route==='/api/session/ses_test/context') data={data:started?[{id:admittedID,type:'user'}, {id:'msg_answer',type:'assistant',content:[{type:'text',text:'answer'}],finish:'stop'},{id:'msg_idle',type:'idle',outcome:'succeeded'}]:[]};
  else if(route==='/api/session/ses_test/prompt'){started=true;admittedID=body.id;data={data:{id:body.id}};}
  else if(route==='/api/session/ses_test/permission')data={data:[]};
  else throw Error(route);
  return new Response(JSON.stringify(data));
 };
 const runtime=new OpenCodeRuntime((type,data)=>events.push({type,data}),fetcher);
 const params={endpoint:'http://localhost:4096',cwd:process.cwd(),no_tools:true,model:'custom/model-a'};
 const probe = await runtime.probe(params); assert.equal(probe.runtime_version,'2.0.0'); assert.equal(probe.capabilities.history_recovery,false);
 const session=await runtime.start(params);
 assert.deepEqual(permissions,[{action:'*',resource:'*',effect:'deny'}]);
 assert.deepEqual(requests.find(r=>r.route==='/api/session').body.model,{providerID:'custom',id:'model-a'});
 assert.equal((await runtime.send({...session,input:'hello',turn_id:'turn-1'})).status,'accepted');
 await until(()=>events.some(e=>e.type==='turn.completed'));
 assert.equal(events.find(e=>e.type==='message.delta').data.text,'answer');
 assert.equal(events.find(e=>e.type==='turn.completed').data.finish_reason,'succeeded');
 assert.equal(requests.find(r=>r.route.endsWith('/prompt')).body.id,'msg_turn_1');
});
test('OpenCode refuses no-tools when server does not confirm policy',async()=>{
 const runtime=new OpenCodeRuntime(()=>{},async()=>new Response(JSON.stringify({data:{id:'ses_test',location:{directory:process.cwd()},permissions:[]}})));
 await assert.rejects(()=>runtime.start({endpoint:'http://localhost:4096',cwd:process.cwd(),no_tools:true}),{code:'unsupported_capability'});
});

test('OpenCode never mistakes a prior idle marker for completion of a new input',async()=>{
 const events=[]; let inputID, polls=0;
 const runtime=new OpenCodeRuntime((type,data)=>events.push({type,data}),async(url,init)=>{
  const route=new URL(url).pathname;let data;
  if(route==='/api/session')data={data:{id:'s',location:{directory:process.cwd()},permissions:[]}};
  else if(route.endsWith('/prompt')){inputID=JSON.parse(init.body).id;data={data:{id:inputID}};}
  else if(route.endsWith('/context')) {
   const old=[{id:'old',type:'user'},{id:'old-idle',type:'idle',outcome:'succeeded'}];
   data={data:inputID && ++polls>1 ? [...old,{id:inputID,type:'user'},{id:'new-idle',type:'idle',outcome:'succeeded'}] : old};
  } else if(route.endsWith('/permission')) data={data:[]}; else throw Error(route);
  return new Response(JSON.stringify(data));
 });
 const session=await runtime.start({endpoint:'http://localhost',cwd:process.cwd()});
 await runtime.send({...session,input:'new',turn_id:'new-turn'});
 await delay(50); assert.equal(events.some(e=>e.type==='turn.completed'),false);
 await until(()=>events.some(e=>e.type==='turn.completed')); assert.ok(polls>=2);
});
