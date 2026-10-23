import assert from 'node:assert/strict';
import {loadWasm,showLoadError,WasmLoadError} from '../loader.js';

// First-party platform mocks: no network, DOM library or timing-dependent sleeps.
class Signal {
 aborted=false;reason=undefined;listeners=new Set();
 addEventListener(type,listener){assert.equal(type,'abort');this.listeners.add(listener);}
 removeEventListener(type,listener){assert.equal(type,'abort');this.listeners.delete(listener);}
 abort(reason=Error('User cancelled')){
  if(this.aborted)return;
  this.aborted=true;this.reason=reason;
  for(const listener of [...this.listeners])listener();
 }
}

function fakeClock(){
 const originalSet=globalThis.setTimeout,originalClear=globalThis.clearTimeout;
 const pending=new Map();let next=0;
 globalThis.setTimeout=(callback,ms)=>{const id=++next;pending.set(id,{callback,ms});return id;};
 globalThis.clearTimeout=id=>pending.delete(id);
 return {pending,fire(){
  assert.equal(pending.size,1);const [id,{callback}]=[...pending][0];
  pending.delete(id);callback();
 },restore(){globalThis.setTimeout=originalSet;globalThis.clearTimeout=originalClear;}};
}

function deferred(){let resolve,reject;const promise=new Promise((yes,no)=>{resolve=yes;reject=no;});return {promise,resolve,reject};}
async function flush(){for(let i=0;i<12;i++)await Promise.resolve();}
let passed=0;
async function test(name,run){
 const clock=fakeClock();
 try{await run(clock);assert.equal(clock.pending.size,0,'loader must remove its timeout');console.log(`ok ${++passed} - ${name}`);}
 finally{clock.restore();}
}

function fixture({modern=true,streaming=true,mime='application/wasm'}={}){
 const imports={env:{}},module={},instance={exports:{}},bytes=new Uint8Array([1,2]).buffer;
 const result={module,instance},signal=new Signal(),behavior={};
 const calls={fetch:0,stream:0,compile:0,instantiate:0,body:0,clone:0,cancel:0};
 const body=()=>({cancel(){calls.cancel++;return behavior.cleanup?behavior.cleanup():Promise.resolve();}});
 const response={ok:true,status:200,headers:{get:()=>mime},body:body(),
  clone(){calls.clone++;return {body:body()};},
  async arrayBuffer(){calls.body++;return behavior.body?behavior.body():bytes;},
 };
 let requestSignal;
 const wasm={
  async instantiate(value,i){
   calls.instantiate++;assert.equal(value,modern?module:bytes);assert.equal(i,imports);
   return behavior.instantiate?behavior.instantiate():modern?instance:result;
  },
 };
 if(modern)wasm.compile=async value=>{calls.compile++;assert.equal(value,bytes);return behavior.compile?behavior.compile():module;};
 if(streaming)wasm[modern?'compileStreaming':'instantiateStreaming']=async (copy,i)=>{
  calls.stream++;assert.notEqual(copy,response);if(!modern)assert.equal(i,imports);
  return behavior.stream?behavior.stream():modern?module:result;
 };
 const host={wasm,signal,timeoutMs:25,async fetch(url,{signal}){
  calls.fetch++;assert.equal(url,'game.wasm');requestSignal=signal;
  return behavior.fetch?behavior.fetch(signal):response;
 }};
 return {imports,module,instance,result,signal,behavior,calls,response,host,
  load:()=>loadWasm('game.wasm',imports,host),
  clean(aborted=false){assert.equal(signal.listeners.size,0);assert.equal(requestSignal?.aborted??false,aborted);},
 };
}

function rejects(promise,expected){
 return assert.rejects(promise,error=>{
  assert.ok(error instanceof WasmLoadError);assert.equal(error.url,'game.wasm');
  for(const [key,value] of Object.entries(expected))assert.equal(error[key],value,key);
  assert.match(error.message,/retry|Reload|cancelled/i);return true;
 });
}

await test('streaming compiles once, starts once and cleans up',async()=>{
 const f=fixture();assert.deepEqual(await f.load(),f.result);
 assert.deepEqual(f.calls,{fetch:1,stream:1,compile:0,instantiate:1,body:0,clone:1,cancel:2});
 f.clean();f.signal.abort();f.clean();
});

await test('MIME fallback reuses the downloaded body and instantiates once',async()=>{
 const f=fixture({mime:'application/octet-stream'});
 f.behavior.stream=()=>{throw new TypeError('Unsupported MIME type');};
 assert.deepEqual(await f.load(),f.result);
 assert.deepEqual(f.calls,{fetch:1,stream:1,compile:1,instantiate:1,body:1,clone:1,cancel:2});f.clean();
});

await test('hosts without streaming compile and instantiate the original bytes',async()=>{
 const f=fixture({streaming:false});assert.deepEqual(await f.load(),f.result);
 assert.equal(f.calls.fetch,1);assert.equal(f.calls.compile,1);assert.equal(f.calls.instantiate,1);f.clean();
});

await test('legacy injected hosts retain instantiateStreaming and byte fallback',async()=>{
 for(const mime of ['application/octet-stream',null]){
  const f=fixture({modern:false,mime});f.behavior.stream=()=>{throw new TypeError('Unsupported MIME');};
  assert.equal(await f.load(),f.result);assert.equal(f.calls.fetch,1);assert.equal(f.calls.body,1);
  assert.equal(f.calls.instantiate,1);f.clean();
 }
 const streamed=fixture({modern:false});assert.equal(await streamed.load(),streamed.result);
 assert.equal(streamed.calls.body,0);assert.equal(streamed.calls.instantiate,0);streamed.clean();
 const buffered=fixture({modern:false,streaming:false});assert.equal(await buffered.load(),buffered.result);
 assert.equal(buffered.calls.instantiate,1);buffered.clean();
});

await test('HTTP failures identify status and release the response without compiling',async()=>{
 for(const status of [404,503]){
  const f=fixture();f.response.ok=false;f.response.status=status;
  await rejects(f.load(),{code:'HTTP_ERROR',stage:'download',status});
  assert.equal(f.calls.fetch,1);assert.equal(f.calls.stream,0);assert.equal(f.calls.body,0);
  assert.equal(f.calls.cancel,1);f.clean(true);
 }
});

await test('network and body failures retain the original cause',async()=>{
 for(const part of ['fetch','body']){
  const f=fixture({streaming:false}),cause=new TypeError('Connection interrupted');
  f.behavior[part]=()=>{throw cause;};
  await rejects(f.load(),{code:'DOWNLOAD_FAILED',stage:'download',cause});
  assert.equal(f.calls.compile,0);assert.equal(f.calls.instantiate,0);f.clean(true);
 }
});

await test('compile failure is never retried through buffered instantiation',async()=>{
 for(const modern of [true,false]){
  const f=fixture({modern}),cause=new WebAssembly.CompileError('Invalid module');
  f.behavior.stream=()=>{throw cause;};
  await rejects(f.load(),{code:'COMPILE_FAILED',stage:'compile',cause});
  assert.equal(f.calls.body,0);assert.equal(f.calls.instantiate,0);f.clean(true);
 }
 const f=fixture({streaming:false}),cause=new WebAssembly.CompileError('Invalid bytes');
 f.behavior.compile=()=>{throw cause;};
 await rejects(f.load(),{code:'COMPILE_FAILED',stage:'compile',cause});
 assert.equal(f.calls.instantiate,0);f.clean(true);
});

await test('link and runtime failures are staged and never instantiate twice',async()=>{
 for(const modern of [true,false])for(const [cause,code,stage] of [
  [new WebAssembly.LinkError('Missing import'),'LINK_FAILED','link'],
  [new WebAssembly.RuntimeError('Startup trap'),'START_FAILED','start'],
  [new TypeError('Imported startup function failed'),'INSTANTIATE_FAILED','instantiate'],
 ]){
  const f=fixture({modern});let starts=0;
  f.behavior[modern?'instantiate':'stream']=()=>{starts++;throw cause;};
  await rejects(f.load(),{code,stage,cause});assert.equal(starts,1);
  assert.equal(f.calls.fetch,1);assert.equal(f.calls.body,0);assert.equal(f.calls.instantiate,modern?1:0);f.clean(true);
 }
});

await test('an unknown legacy MIME cannot mistake an import TypeError for a fallback',async()=>{
 const f=fixture({modern:false});delete f.response.headers;
 const cause=new TypeError('Thrown by imported startup');f.behavior.stream=()=>{throw cause;};
 await rejects(f.load(),{code:'INSTANTIATE_FAILED',stage:'instantiate',cause});
 assert.equal(f.calls.body,0);assert.equal(f.calls.instantiate,0);f.clean(true);
});

await test('fetch timeout aborts the request and cleans up a late response',async clock=>{
 const f=fixture(),download=deferred();f.behavior.fetch=()=>download.promise;
 const failed=rejects(f.load(),{code:'TIMEOUT',stage:'download'});
 assert.equal([...clock.pending.values()][0].ms,25);clock.fire();await failed;f.clean(true);
 download.resolve(f.response);await flush();
 assert.equal(f.calls.cancel,1);assert.equal(f.calls.stream,0);assert.equal(f.calls.instantiate,0);
});

await test('body timeout prevents late bytes from compiling',async clock=>{
 const f=fixture({streaming:false}),body=deferred(),entered=deferred();
 f.behavior.body=()=>{entered.resolve();return body.promise;};
 const failed=rejects(f.load(),{code:'TIMEOUT',stage:'download'});
 await entered.promise;clock.fire();await failed;f.clean(true);
 body.resolve(new ArrayBuffer(2));await flush();assert.equal(f.calls.compile,0);assert.equal(f.calls.instantiate,0);
});

await test('compile timeout prevents late compilation from starting the game',async clock=>{
 const f=fixture(),compiled=deferred(),entered=deferred();
 f.behavior.stream=()=>{entered.resolve();return compiled.promise;};
 const failed=rejects(f.load(),{code:'TIMEOUT',stage:'compile'});
 await entered.promise;clock.fire();await failed;f.clean(true);
 compiled.resolve(f.module);await flush();assert.equal(f.calls.instantiate,0);assert.equal(f.calls.body,0);
});

await test('late streaming rejection after timeout cannot launch a fallback',async clock=>{
 const f=fixture(),compiled=deferred(),entered=deferred();
 f.behavior.stream=()=>{entered.resolve();return compiled.promise;};
 const failed=rejects(f.load(),{code:'TIMEOUT',stage:'compile'});
 await entered.promise;clock.fire();await failed;
 compiled.reject(new TypeError('Late MIME failure'));await flush();
 assert.equal(f.calls.body,0);assert.equal(f.calls.instantiate,0);f.clean(true);
});

await test('timeout during instantiation never repeats or returns a late instance',async clock=>{
 const f=fixture(),started=deferred(),entered=deferred();
 f.behavior.instantiate=()=>{entered.resolve();return started.promise;};
 const failed=rejects(f.load(),{code:'TIMEOUT',stage:'instantiate'});
 await entered.promise;clock.fire();await failed;
 started.resolve(f.instance);await flush();assert.equal(f.calls.instantiate,1);f.clean(true);
});

await test('pre-cancelled loads do not fetch or schedule work',async()=>{
 const f=fixture(),cause=Error('Cancel before loading');f.signal.abort(cause);
 await rejects(f.load(),{code:'CANCELLED',stage:'download',cause});
 assert.equal(f.calls.fetch,0);assert.equal(f.calls.instantiate,0);f.clean();
});

await test('user cancellation aborts pending requests even when fetch ignores abort',async()=>{
 const f=fixture(),download=deferred(),cause=Error('Cancel button');
 f.behavior.fetch=()=>download.promise;
 const failed=rejects(f.load(),{code:'CANCELLED',stage:'download',cause});
 f.signal.abort(cause);await failed;f.clean(true);
 download.reject(new TypeError('Late network rejection'));await flush();assert.equal(f.calls.stream,0);
});

await test('cancellation at body, compile and instantiate boundaries prevents further steps',async()=>{
 for(const [part,stage] of [['body','download'],['compile','compile'],['stream','compile'],['instantiate','instantiate']]){
  const f=fixture({streaming:part!=='body'&&part!=='compile'}),pending=deferred(),entered=deferred();
  f.behavior[part]=()=>{entered.resolve();return pending.promise;};
  const failed=rejects(f.load(),{code:'CANCELLED',stage});
  await entered.promise;f.signal.abort();await failed;f.clean(true);
  pending.resolve(part==='body'?new ArrayBuffer(2):part==='instantiate'?f.instance:f.module);await flush();
  assert.equal(f.calls.instantiate,part==='instantiate'?1:0);
 }
});

await test('late legacy streaming failure after cancellation cannot instantiate again',async()=>{
 const f=fixture({modern:false,mime:'application/octet-stream'}),stream=deferred(),entered=deferred();
 f.behavior.stream=()=>{entered.resolve();return stream.promise;};
 const failed=rejects(f.load(),{code:'CANCELLED',stage:'instantiate'});
 await entered.promise;f.signal.abort();await failed;
 stream.reject(new TypeError('MIME'));await flush();assert.equal(f.calls.body,0);assert.equal(f.calls.instantiate,0);f.clean(true);
});

await test('explicit retry with a fresh signal succeeds while a cancelled compile finishes',async()=>{
 const f=fixture(),compiled=deferred(),entered=deferred();
 f.behavior.stream=()=>{entered.resolve();return compiled.promise;};
 const failed=rejects(f.load(),{code:'CANCELLED',stage:'compile'});
 await entered.promise;f.signal.abort();await failed;
 const retrySignal=new Signal();f.host.signal=retrySignal;delete f.behavior.stream;
 assert.deepEqual(await f.load(),f.result);assert.equal(retrySignal.listeners.size,0);
 compiled.resolve(f.module);await flush();assert.equal(f.calls.fetch,2);assert.equal(f.calls.instantiate,1);
});

await test('body cleanup rejection or throw cannot mask success or a load error',async()=>{
 for(const cleanup of [()=>Promise.reject(Error('Body locked')),()=>{throw Error('Body consumed');}]){
  const f=fixture();f.behavior.cleanup=cleanup;assert.deepEqual(await f.load(),f.result);f.clean();
  f.response.ok=false;f.response.status=503;
  await rejects(f.load(),{code:'HTTP_ERROR',stage:'download',status:503});f.clean(true);
 }
});

// Tiny authored Wasm modules exercise the actual platform API too. Fetch is
// still mocked, and no game builds, external fixtures or packages are needed.
const emptyWasm=new Uint8Array([0,97,115,109,1,0,0,0]);
const startupWasm=new Uint8Array([
 ...emptyWasm,
 1,4,1,96,0,0, // () -> () function type
 2,12,1,3,101,110,118,4,98,111,111,116,0,0, // import env.boot
 8,1,0, // imported function is the Wasm start function
]);

await test('native Wasm preserves result shape for streaming and MIME fallback',async()=>{
 for(const mime of ['application/wasm','application/octet-stream']){
  let downloads=0;const signal=new Signal();
  const result=await loadWasm('game.wasm',{}, {signal,async fetch(){
   downloads++;return new Response(emptyWasm,{headers:{'Content-Type':mime}});
  }});
  assert.ok(result.module instanceof WebAssembly.Module);assert.ok(result.instance instanceof WebAssembly.Instance);
  assert.equal(downloads,1);assert.equal(signal.listeners.size,0);
 }
});

await test('native compile, link and imported startup failures never download or start twice',async()=>{
 for(const kind of ['compile','link','startup']){
  let downloads=0,starts=0;const cause=new TypeError('Startup import failed');
  const imports=kind==='startup'?{env:{boot(){starts++;throw cause;}}}:{env:{}};
  const expected=kind==='compile'?{code:'COMPILE_FAILED',stage:'compile'}:
   kind==='link'?{code:'LINK_FAILED',stage:'link'}:{code:'INSTANTIATE_FAILED',stage:'instantiate',cause};
  await rejects(loadWasm('game.wasm',imports,{async fetch(){
   downloads++;return new Response(kind==='compile'?new Uint8Array([0]):startupWasm,{headers:{'Content-Type':'application/wasm'}});
  }}),expected);
  assert.equal(downloads,1);assert.equal(starts,kind==='startup'?1:0);
 }
});

function chunkedResponse(bytes=emptyWasm,headers={},chunkSize=1){
 let offset=0;
 return new Response(new ReadableStream({pull(sink){
  if(offset===bytes.length){sink.close();return;}
  const end=Math.min(bytes.length,offset+chunkSize);
  sink.enqueue(bytes.subarray(offset,end));offset=end;
 }}),{headers:{'Content-Type':'application/wasm',...headers}});
}

await test('progress counts each byte once across native streaming, MIME fallback and legacy hosts',async()=>{
 for(const modern of [true,false])for(const streaming of [true,false]){
  for(const mime of ['application/wasm','application/octet-stream']){
   const events=[],calls={fetch:0,stream:0,compile:0,instantiate:0};let starts=0;
   const wasm={instantiate(...args){calls.instantiate++;return WebAssembly.instantiate(...args);}};
   if(modern)wasm.compile=bytes=>{calls.compile++;return WebAssembly.compile(bytes);};
   if(streaming){
    const method=modern?'compileStreaming':'instantiateStreaming';
    wasm[method]=(...args)=>{calls.stream++;return WebAssembly[method](...args);};
   }
   const result=await loadWasm('game.wasm',{env:{boot(){starts++;}}},{wasm,
    onProgress:event=>events.push(event),fetch(){
     calls.fetch++;return chunkedResponse(startupWasm,{'Content-Type':mime,'Content-Length':`${startupWasm.length}`});
    },
   });
   assert.ok(result.module instanceof WebAssembly.Module);assert.ok(result.instance instanceof WebAssembly.Instance);
   assert.equal(starts,1);assert.equal(calls.fetch,1);assert.equal(calls.stream,Number(streaming));
   assert.equal(calls.instantiate,Number(modern||!streaming||mime!=='application/wasm'));
   assert.equal(calls.compile,Number(modern&&(!streaming||mime!=='application/wasm')));
   assert.deepEqual(events[0],{loaded:0,total:startupWasm.length,done:false});
   assert.deepEqual(events.at(-1),{loaded:startupWasm.length,total:startupWasm.length,done:true});
   assert.equal(events.filter(event=>event.done).length,1);assert.ok(events.length>2);
   for(let i=1;i<events.length;i++){
    assert.ok(events[i].loaded>=events[i-1].loaded);assert.ok(Object.isFrozen(events[i]));
   }
  }
 }
});

await test('unknown, invalid, encoded and inconsistent lengths never invent a total',async()=>{
 const variants=[{},...['','0','-1','1.5','8junk','8, 8','1e1','Infinity','9007199254740992'].map(value=>({'Content-Length':value})),
  {'Content-Length':'8','Content-Encoding':'gzip'}, {'Content-Length':'8','Content-Encoding':'br'},
  {'Content-Length':'4'}, {'Content-Length':'16'},
 ];
 for(const headers of variants){
  const events=[];
  await loadWasm('game.wasm',{}, {onProgress:event=>events.push(event),fetch:()=>chunkedResponse(emptyWasm,headers)});
  assert.deepEqual(events.at(-1),{loaded:8,total:null,done:true});
  if(!['4','16'].includes(headers['Content-Length']))assert.ok(events.every(event=>event.total===null));
 }
 const events=[];
 await loadWasm('game.wasm',{}, {onProgress:event=>events.push(event),
  fetch:()=>chunkedResponse(emptyWasm,{'Content-Length':' 8 ','Content-Encoding':' Identity '}),
 });
 assert.deepEqual(events.at(-1),{loaded:8,total:8,done:true});
});

await test('known and unknown downloads bound notification counts and retain exact final bytes',async()=>{
 for(const known of [true,false]){
  const events=[],chunk=new Uint8Array(known?1:65536),chunks=known?1000:128,total=chunks*chunk.length;
  let reads=0,consumed=0,clones=0;
  const response=new Response(new ReadableStream({pull(sink){
   if(reads===chunks){sink.close();return;}
   reads++;sink.enqueue(chunk);
  }}),{headers:{'Content-Type':'application/wasm',...(known?{'Content-Length':`${total}`}:{})}});
  const originalClone=Response.prototype.clone;
  Response.prototype.clone=function(){clones++;return originalClone.call(this);};
  try{
   await loadWasm('game.wasm',{}, {onProgress:event=>events.push(event),fetch:()=>response,wasm:{
    compile(){assert.fail('must keep streaming');},
    async compileStreaming(copy){
     const reader=copy.body.getReader();
     for(;;){const {done,value}=await reader.read();if(done)break;consumed+=value.byteLength;}
     reader.releaseLock();return {};
    },
    instantiate(){return {};},
   }});
  }finally{Response.prototype.clone=originalClone;}
  assert.equal(consumed,total);assert.equal(reads,chunks);assert.equal(clones,1);
  assert.ok(events.length>2&&events.length<=64);assert.equal(events.filter(event=>event.done).length,1);
  assert.deepEqual(events.at(-1),{loaded:total,total:known?total:null,done:true});
 }
});

await test('progress observer throws, rejections and pending promises cannot change startup',async()=>{
 const observers=[()=>{throw Error('Broken UI');},async()=>{throw Error('Async UI failure');},
  ()=>new Promise(()=>{}),()=>({get then(){throw Error('Broken thenable');}}),
  event=>{event.loaded=-1;},
 ];
 for(const observer of observers){
  let calls=0,starts=0;
  const result=await loadWasm('game.wasm',{env:{boot(){starts++;}}},{
   onProgress(event){calls++;return observer(event);},fetch:()=>chunkedResponse(startupWasm),
  });
  assert.ok(result.instance instanceof WebAssembly.Instance);assert.equal(starts,1);assert.ok(calls>=2);
 }
});

await test('progress preserves compile, link and startup errors despite a failing observer',async()=>{
 for(const kind of ['compile','link','startup'])for(const mime of ['application/wasm','application/octet-stream']){
  let downloads=0,starts=0;const cause=new TypeError('Imported startup failure');
  const imports=kind==='startup'?{env:{boot(){starts++;throw cause;}}}:{env:{}};
  const expected=kind==='compile'?{code:'COMPILE_FAILED',stage:'compile'}:
   kind==='link'?{code:'LINK_FAILED',stage:'link'}:{code:'INSTANTIATE_FAILED',stage:'instantiate',cause};
  await rejects(loadWasm('game.wasm',imports,{onProgress(){throw Error('UI failure');},fetch(){
   downloads++;return chunkedResponse(kind==='compile'?new Uint8Array([0]):startupWasm,{'Content-Type':mime});
  }}),expected);
  assert.equal(downloads,1);assert.equal(starts,Number(kind==='startup'));
 }
});

await test('non-stream response adapters report coarse progress without changing the fallback',async()=>{
 const f=fixture({mime:'application/octet-stream'}),events=[];
 f.host.onProgress=event=>events.push(event);
 f.behavior.stream=()=>{throw new TypeError('MIME');};
 assert.deepEqual(await f.load(),f.result);
 assert.deepEqual(events,[{loaded:0,total:null,done:false},{loaded:2,total:null,done:true}]);
 assert.equal(f.calls.fetch,1);assert.equal(f.calls.clone,1);assert.equal(f.calls.body,1);f.clean();
});

await test('non-function observers leave the original loading path intact',async()=>{
 for(const observer of [undefined,null,false,1,'progress',{}]){
  const f=fixture();f.host.onProgress=observer;assert.deepEqual(await f.load(),f.result);
  assert.deepEqual(f.calls,{fetch:1,stream:1,compile:0,instantiate:1,body:0,clone:1,cancel:2});f.clean();
 }
});

await test('HTTP, network and pre-cancelled failures never notify progress',async()=>{
 for(const kind of ['http','network','cancel']){
  const f=fixture();
  // Observe counts outside the callback, whose errors are intentionally isolated.
  let events=0;f.host.onProgress=()=>{events++;};
  if(kind==='http'){f.response.ok=false;f.response.status=404;}
  if(kind==='network')f.behavior.fetch=()=>{throw new TypeError('Offline');};
  if(kind==='cancel')f.signal.abort();
  await rejects(f.load(),{code:kind==='http'?'HTTP_ERROR':kind==='network'?'DOWNLOAD_FAILED':'CANCELLED',stage:'download'});
  assert.equal(events,0);f.clean(kind!=='cancel');
 }
});

await test('cancelling from the first progress notification prevents compilation',async()=>{
 const f=fixture(),events=[];f.host.onProgress=event=>{events.push(event);f.signal.abort();};
 await rejects(f.load(),{code:'CANCELLED',stage:'download'});
 assert.equal(events.length,1);assert.equal(f.calls.stream,0);assert.equal(f.calls.body,0);assert.equal(f.calls.instantiate,0);f.clean(true);
});

await test('cancelling from byte or EOF progress never starts Wasm or emits late events',async()=>{
 for(const atEnd of [false,true]){
  const signal=new Signal(),events=[];let starts=0;
  await rejects(loadWasm('game.wasm',{env:{boot(){starts++;}}},{signal,
   onProgress(event){events.push(event);if(atEnd?event.done:event.loaded>0)signal.abort();},
   fetch:()=>chunkedResponse(startupWasm,{'Content-Length':`${startupWasm.length}`}),
  }),{code:'CANCELLED',stage:'compile'});
  const count=events.length;await flush();assert.equal(events.length,count);
  assert.equal(starts,0);assert.equal(signal.listeners.size,0);
 }
});

await test('pending progress streams are cancelled on timeout or abort without late notifications',async clock=>{
 for(const timedOut of [true,false]){
  const signal=new Signal(),events=[],entered=deferred(),pending=deferred(),cancelled=deferred();
  let starts=0,cancels=0;
  const response=new Response(new ReadableStream({
   pull(){entered.resolve();return pending.promise;},
   cancel(){cancels++;cancelled.resolve();},
  }),{headers:{'Content-Type':'application/wasm'}});
  const failed=rejects(loadWasm('game.wasm',{}, {signal,onProgress:event=>events.push(event),fetch:()=>response,
   wasm:{compile:WebAssembly.compile,compileStreaming:WebAssembly.compileStreaming,instantiate(){starts++;return {};}},
  }),{code:timedOut?'TIMEOUT':'CANCELLED',stage:'compile'});
  await entered.promise;await flush();
  if(timedOut)clock.fire();else signal.abort();
  await failed;await cancelled.promise;const count=events.length;
  pending.resolve();await flush();
  assert.equal(events.length,count);assert.equal(events.some(event=>event.done),false);
  assert.equal(cancels,1);assert.equal(starts,0);assert.equal(signal.listeners.size,0);
 }
});

await test('stream read failures preserve the same WasmLoadError with and without progress',async()=>{
 for(const streaming of [true,false])for(const cause of [Error('Connection lost'),new TypeError('Network read failed')]){
  const failures=[];
  for(const observed of [false,true]){
   const events=[];let downloads=0,starts=0;
   const wasm={compile:WebAssembly.compile,instantiate(){starts++;return {};}};
   if(streaming)wasm.compileStreaming=WebAssembly.compileStreaming;
   try{
    await loadWasm('game.wasm',{}, {wasm,onProgress:observed?event=>events.push(event):undefined,fetch(){
     downloads++;return new Response(new ReadableStream({pull(sink){sink.error(cause);}}),
      {headers:{'Content-Type':'application/wasm'}});
    }});
    assert.fail('broken stream must fail loading');
   }catch(error){
    assert.ok(error instanceof WasmLoadError);assert.equal(error.cause,cause);
    failures.push({code:error.code,stage:error.stage,url:error.url});
   }
   assert.equal(downloads,1);assert.equal(starts,0);assert.equal(events.some(event=>event.done),false);
   const count=events.length;await flush();assert.equal(events.length,count);
  }
  assert.deepEqual(failures[0],failures[1]);
 }
});

await test('early compilation failure cancels a pending progress source and isolates cleanup rejection',async()=>{
 for(const rejectCleanup of [false,true]){
  const cause=new WebAssembly.CompileError('Rejected before EOF'),events=[],cancelled=deferred();
  let cancels=0,starts=0;
  await rejects(loadWasm('game.wasm',{}, {onProgress:event=>events.push(event),
   fetch:()=>new Response(new ReadableStream({
    pull(){return new Promise(()=>{});},
    cancel(){cancels++;cancelled.resolve();if(rejectCleanup)return Promise.reject(Error('Cleanup failed'));},
   }),{headers:{'Content-Type':'application/wasm'}}),
   wasm:{compile(){assert.fail('must not buffer a compile failure');},
    compileStreaming(){throw cause;},instantiate(){starts++;return {};},
   },
  }),{code:'COMPILE_FAILED',stage:'compile',cause});
  await cancelled.promise;const count=events.length;await flush();
  assert.equal(cancels,1);assert.equal(starts,0);assert.equal(events.length,count);
  assert.deepEqual(events,[{loaded:0,total:null,done:false}]);
 }
});

await test('late buffered bytes after cancellation or timeout cannot emit completion progress',async clock=>{
 for(const timedOut of [false,true]){
  const f=fixture({streaming:false}),events=[],pending=deferred(),entered=deferred();
  f.host.onProgress=event=>events.push(event);f.behavior.body=()=>{entered.resolve();return pending.promise;};
  const failed=rejects(f.load(),{code:timedOut?'TIMEOUT':'CANCELLED',stage:'download'});
  await entered.promise;if(timedOut)clock.fire();else f.signal.abort();
  await failed;pending.resolve(new ArrayBuffer(2));await flush();
  assert.deepEqual(events,[{loaded:0,total:null,done:false}]);
  assert.equal(f.calls.compile,0);assert.equal(f.calls.instantiate,0);f.clean(true);
 }
});

await test('progress resets on retry while the cancelled compilation is still pending',async()=>{
 const f=fixture(),events=[],compiled=deferred(),entered=deferred();
 f.host.onProgress=event=>events.push(event);
 f.behavior.stream=()=>{entered.resolve();return compiled.promise;};
 const failed=rejects(f.load(),{code:'CANCELLED',stage:'compile'});
 await entered.promise;f.signal.abort();await failed;
 f.host.signal=new Signal();f.behavior.stream=()=>{throw new TypeError('MIME');};
 assert.deepEqual(await f.load(),f.result);const count=events.length;
 compiled.resolve(f.module);await flush();
 assert.equal(events.length,count);assert.deepEqual(events,[
  {loaded:0,total:null,done:false},{loaded:0,total:null,done:false},{loaded:2,total:null,done:true},
 ]);
 assert.equal(f.calls.fetch,2);assert.equal(f.calls.instantiate,1);assert.equal(f.host.signal.listeners.size,0);
});

await test('the documented progress callback example loads real Wasm with an unknown total',async()=>{
 const controller=new AbortController(),statusElement={textContent:''},messages=[];
 const pending=loadWasm('game.wasm',{}, {
  signal:controller.signal,
  onProgress({loaded,total,done}){
   statusElement.textContent=done?'Compiling game…':total===null
    ?`Downloaded ${loaded} bytes`:`Downloaded ${loaded} of ${total} bytes`;
   messages.push(statusElement.textContent);
  },
  fetch:()=>chunkedResponse(),
 });
 const {instance}=await pending;
 assert.ok(instance instanceof WebAssembly.Instance);
 assert.equal(messages[0],'Downloaded 0 bytes');assert.equal(statusElement.textContent,'Compiling game…');
});

// A small eventful DOM mock also dispatches clicks to disabled buttons, proving
// the handler itself guards against repeated or stale activation.
function errorTarget(){
 let reloads=0;
 const document={defaultView:{location:{reload(){reloads++;}}},createElement(tag){
  assert.equal(tag,'button');const listeners=new Set();
  return {style:{},listeners,disabled:false,
   addEventListener(type,listener){assert.equal(type,'click');listeners.add(listener);},
   removeEventListener(type,listener){assert.equal(type,'click');listeners.delete(listener);},
   click(){for(const listener of [...listeners])listener();},
   remove(){if(this.parent)this.parent.children=this.parent.children.filter(child=>child!==this);this.parent=null;},
  };
 }};
 return {children:[],ownerDocument:document,textContent:'',
  append(child){child.parent=this;this.children.push(child);},get reloads(){return reloads;},
 };
}

await test('retry reloads once and retires stale buttons when the error is replaced',async()=>{
 const target=errorTarget(),oldDispose=showLoadError(Error('Download failed. Retry.'),target);
 const oldButton=target.children[0];assert.equal(oldButton.type,'button');assert.equal(oldButton.textContent,'Retry game');
 const dispose=showLoadError(Error('Compilation failed. Reload.'),target),button=target.children[0];
 assert.equal(target.children.length,1);assert.equal(target.textContent,'Compilation failed. Reload.');
 oldButton.click();oldDispose();assert.equal(target.reloads,0);assert.equal(oldButton.listeners.size,0);
 button.click();button.click();assert.equal(target.reloads,1);assert.equal(button.disabled,true);
 assert.equal(button.textContent,'Reloading game…');
 dispose();dispose();button.click();assert.equal(target.reloads,1);assert.equal(button.listeners.size,0);assert.equal(target.children.length,0);
});

await test('non-Error failures have a safe message and disposing prevents retries',async()=>{
 const target=errorTarget(),dispose=showLoadError('<script>unsafe</script>',target),button=target.children[0];
 assert.equal(target.textContent,'The game could not start.');dispose();button.click();assert.equal(target.reloads,0);
});

console.log(`loader: ${passed} behavioral tests passed`);
