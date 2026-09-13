"use strict";
const test=require("node:test");
const assert=require("node:assert/strict");
const fs=require("node:fs"),path=require("node:path"),vm=require("node:vm");
const plain=value=>JSON.parse(JSON.stringify(value));
function bridge(respond) {
  let listener; const calls=[];
  const parent={postMessage:message=>{
    if (!message.id) return;
    calls.push(message);
    queueMicrotask(async()=>{
      let result={};
      if (message.method!=="ui/initialize") {
        const answer=await respond(message.params.name,message.params.arguments);
        result=answer&&answer.isError?answer:{structuredContent:answer};
      }
      listener({source:parent,data:{jsonrpc:"2.0",id:message.id,result}});
    });
  }};
  const context=vm.createContext({setTimeout,clearTimeout,console,document:{documentElement:{dataset:{}}},parent,addEventListener:(type,fn)=>{listener=fn;}});
  context.window=context;
  for (const file of ["loom-core.js","mcp-app-bridge.js"]) vm.runInContext(fs.readFileSync(path.join(__dirname,file),"utf8"),context);
  return {api:context.KMP_APP_API,calls};
}
test("MCP App human reports preserve focused refs, overlays and additional abouts",async()=>{
  const snapshot={about:"a",view_revision:4,focus:{refs:["a:proof"]},projection:{semantic_zoom:"atlas",abouts:["b"],overlays:["noise_ratio"]}};
  let report;
  const {api}=bridge((name,args)=>{
    if (name==="kmp_view_apply_intent") report=args;
    return {state:snapshot};
  });
  await api("/api/view",{});
  await api("/api/view/report",{about:"a",clock:"observed",from:"2026-09-01T00:00:00Z",to:"2026-09-02T00:00:00Z",layer_abouts:'["b","c"]',zoom:"auto"});
  assert.deepEqual(plain(report.focus.refs),["a:proof"]);
  assert.deepEqual(plain(report.projection.abouts),["b","c"]);
  assert.deepEqual(plain(report.projection.overlays),["noise_ratio"]);
  assert.equal(report.projection.semantic_zoom,undefined,"automatic detail explicitly clears the requested rung");
  assert.equal(report.actor,"human");
});
test("MCP App handoff reaches the same revision-checked operation as HTTP",async()=>{
  const {api,calls}=bridge(()=>({state:{view_revision:8,last_change:{actor:"human"}}}));
  const state=await api("/api/view/take-control",{id:"shared",expected_revision:7});
  assert.equal(state.view_revision,8);
  assert.deepEqual(plain(calls.at(-1).params),{name:"kmp_view_take_control",arguments:{view_id:"shared",expected_revision:7}});
});
test("MCP App coalesces parallel catalogue and initial view reads",async()=>{
  const snapshot={state:{about:"a",view_revision:4,projection:{},focus:{}}};
  const {api,calls}=bridge(()=>snapshot);
  const [abouts,state]=await Promise.all([api("/api/abouts",{}),api("/api/view",{id:"default"})]);
  assert.deepEqual(plain(abouts),{abouts:["a"]});
  assert.equal(state.about,"a");
  assert.equal(calls.filter(call=>call.params?.name==="kmp_view_get_state").length,1);
});
test("MCP App clock focus reads recorded coordinates and trace frames every stored hop",async()=>{
  const coordinate={dimension:"topic",scope_id:"review",observed_at:"2026-09-01T00:00:00Z"};
  const {api}=bridge(name=>name==="kmp_inspect"?{object:{ref:"a:proof",kind:"decision",text:"Proof"},links:{incoming:[{rel:"contains_entry",coordinate}],outgoing:[]}}:{trace:[{from:"a:start",to:"a:middle",rel:"follows"},{from:"a:middle",to:"a:end",rel:"follows"}]});
  const inspected=await api("/api/node",{about:"a",id:"a:proof"});
  assert.deepEqual(plain(inspected.raw_coordinates),[coordinate]);
  const trace=await api("/api/trace",{about:"a",from:"a:start",to:"a:end"});
  assert.deepEqual(plain(trace.nodes.map(node=>node.id)).sort(),["a:end","a:middle","a:start"]);
});

test("MCP App preserves independent relation clocks in the shared viewer shape",async()=>{
  const clocks={observed_at:"2026-09-02T13:00:00.500Z",ingested_at:"2026-09-03T09:00:00Z"};
  const {api}=bridge(()=>({trace:[{from:"a:check",to:"a:execution",rel:"supports",clocks}]}));
  const trace=await api("/api/trace",{about:"a",from:"a:check",to:"a:execution"});
  assert.equal(trace.edges[0].observed_at,clocks.observed_at);
  assert.equal(trace.edges[0].ingested_at,clocks.ingested_at);
  assert.equal(trace.edges[0].occurred_at,undefined);
});

test("MCP App resolves a focus batch in one native call and preserves partial status",async()=>{
  const coordinate={dimension:"task",scope_id:"task:batch",observed_at:"2026-09-13T10:00:00Z"};
  const result={nodes:[{id:"a",kind:"decision"}],coordinates:{a:[coordinate]},missing:["gone"],omitted:["b"],incomplete_coordinates:[],stop_reason:"edge_budget",scanned_edges:3};
  const {api,calls}=bridge((name,args)=>{
    assert.equal(name,"kmp_view_read_nodes");
    assert.deepEqual(plain(args),{about:"project:x",refs:["a","b","gone"],max_edges:3});
    return result;
  });
  assert.deepEqual(plain(await api("/api/nodes",{about:"project:x",ids:"a,b,gone",max_edges:3})),result);
  assert.equal(calls.filter(c=>c.method==="tools/call").length,1);
});

test("MCP App keeps the kernel's conflict code on a refused focus batch so the loom can retry it",async()=>{
  const {api}=bridge(()=>({isError:true,content:[{type:"text",text:"node batch snapshot changed; discard previous batches and restart the focus"}],structuredContent:{error:{code:"conflict",message:"node batch snapshot changed"}}}));
  await assert.rejects(()=>api("/api/nodes",{about:"project:x",ids:"a"}),error=>error.code==="conflict"&&/snapshot changed/.test(error.message));
});
