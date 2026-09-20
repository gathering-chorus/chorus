// A deterministic ACP peer, not a substitute for live-runtime certification.
const readline = require('node:readline');
const write = value => process.stdout.write(JSON.stringify({jsonrpc:'2.0',...value})+'\n');
let promptId;
readline.createInterface({input:process.stdin}).on('line',line => {
 const value=JSON.parse(line);
 if(value.method==='initialize') write({id:value.id,result:{protocolVersion:1,agentCapabilities:{loadSession:true}}});
 else if(value.method==='session/new' || value.method==='session/load') {
  if(value.params.mcpServers.length) write({method:'session/update',params:{sessionId:'gemini-test-session',update:{sessionUpdate:'fixture_mcp_config',method:value.method,mcpServers:value.params.mcpServers}}});
  write({id:value.id,result:value.method==='session/new'?{sessionId:'gemini-test-session'}:{}});
 }
 else if(value.method==='session/prompt') {
  promptId=value.id;
  if(value.params.prompt[0].text==='permission') write({id:900,method:'session/request_permission',params:{sessionId:'gemini-test-session',toolCall:{toolCallId:'t1',title:'write'},options:[{optionId:'allow-one',kind:'allow_once',name:'Allow'}]}});
  else { write({method:'session/update',params:{sessionId:'gemini-test-session',update:{sessionUpdate:'agent_message_chunk',content:{type:'text',text:'hello'}}}});write({id:value.id,result:{stopReason:'end_turn'}}); }
 } else if(value.method==='session/cancel') write({id:promptId,result:{stopReason:'cancelled'}});
 else if(value.id===900 && value.result) write({id:promptId,result:{stopReason:value.result.outcome.outcome==='selected'?'end_turn':'cancelled'}});
});
