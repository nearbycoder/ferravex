// Source-only diagnostics/DOM checks: node web/tests/details.mjs --source-only
// Default also preserves the staged-Wasm integration checks below (parent-owned).
import assert from 'node:assert/strict';
import fs from 'node:fs';
import test from 'node:test';
import {bindInput} from '../input.js';
import {bindDetails,createDiagnosticSnapshot,diagnosticSnapshotMaxBytes,framebufferSize} from '../details.js';
assert.deepEqual(framebufferSize(390,260,3,0),{width:1170,height:780,scale:3});
assert.deepEqual(framebufferSize(390,260,3,2),{width:390,height:260,scale:1});
assert.deepEqual(framebufferSize(390,260,3,3),{width:585,height:390,scale:1.5});
for(const [w,h,dpr,setting,limit] of [[10000,9000,4,5,4096],[390,844,3,0,2048],[NaN,Infinity,NaN,99,NaN]]){
 const size=framebufferSize(w,h,dpr,setting,limit);assert.ok(size.width>=1&&size.height>=1&&size.width<=8192&&size.height<=8192);assert.ok(size.width*size.height<=8*1024*1024);
}

function readings(){
 const canvas={width:1280,height:720,clientWidth:640,clientHeight:360,requestPointerLock(){},dataset:{
  frames:'42',drawCalls:'9',shadowDrawCalls:'2',vertices:'120',uploadBytes:'3840',residentMeshes:'3',
  visible:'4',culled:'5',effectVertices:'12',skinnedDrawCalls:'1',skinJoints:'6',graphicsRestores:'0',
  renderScale:'2',graphicsState:'ready',audioState:'running',gpuSource:'WebGL2 EXT_disjoint_timer_query_webgl2',
  gpuDisjoint:'false',gpuUs:'230.5',gpuFrame:'40',
 }};
 const invoked=[];
 const unused=()=>{invoked.push(true);throw Error('Snapshot must not invoke Wasm or browser APIs');};
 const exports={memory:new WebAssembly.Memory({initial:1}),ferravex_details_get:unused,ferravex_details_set:unused,
  ferravex_diagnostics_export:unused,ferravex_diagnostics_len:unused,ferravex_performance_sample:unused};
 const host={
  document:{visibilityState:'visible',hasFocus:()=>true,fullscreenElement:null,pointerLockElement:canvas,documentElement:{requestFullscreen:unused}},
  navigator:{onLine:false,getGamepads:unused},innerWidth:640,innerHeight:480,devicePixelRatio:2,
  WebAssembly,WebGL2RenderingContext:unused,AudioContext:unused,AudioWorkletNode:unused,Blob,URL,
 };
 return {canvas,exports,host,invoked};
}

test('documented snapshot example captures actual counters and capabilities without calling exports',()=>{
 const {canvas,exports,host,invoked}=readings(),wasm={exports};
 const snapshot=createDiagnosticSnapshot(canvas,'neon-swarm',wasm.exports,host);
 const json=JSON.stringify(snapshot,null,2);
 assert.deepEqual(JSON.parse(json),snapshot);
 assert.equal(snapshot.schema,'ferravex.runtime-diagnostics');assert.equal(snapshot.version,1);assert.equal(snapshot.game,'neon-swarm');
 assert.deepEqual(snapshot.runtime,{wasmLoaded:true,memoryBytes:65536,detailsApi:true,playtestCsvApi:true,performanceSampleApi:true});
 assert.deepEqual(snapshot.counters,{frames:42,drawCalls:9,shadowDrawCalls:2,vertices:120,uploadBytes:3840,residentMeshes:3,visible:4,culled:5,effectVertices:12,skinnedDrawCalls:1,skinJoints:6,graphicsRestores:0});
 assert.deepEqual(snapshot.display,{width:1280,height:720,cssWidth:640,cssHeight:360,viewportWidth:640,viewportHeight:480,deviceRatio:2,renderScale:2});
 assert.deepEqual(snapshot.gpu,{source:'WebGL2 EXT_disjoint_timer_query_webgl2',disjoint:false,microseconds:230.5,frame:40});
 assert.deepEqual(snapshot.state,{graphics:'ready',audio:'running',visibility:'visible',focused:true,online:false,fullscreen:false,pointerLocked:true});
 assert.ok(Object.values(snapshot.capabilities).every(value=>value===true));
 canvas.dataset.frames='43';exports.memory.grow(1);
 assert.equal(snapshot.counters.frames,42);assert.equal(snapshot.runtime.memoryBytes,65536);
 const next=createDiagnosticSnapshot(canvas,'neon-swarm',exports,host);
 assert.equal(next.counters.frames,43);assert.equal(next.runtime.memoryBytes,131072);
 snapshot.counters.frames=999;
 assert.equal(canvas.dataset.frames,'43','snapshots have no live references');
 assert.deepEqual(invoked,[]);
});

test('missing telemetry is unknown rather than fabricated zero or ready',()=>{
 const snapshot=createDiagnosticSnapshot({},'owned-orbit',undefined,{});
 assert.equal(snapshot.runtime.wasmLoaded,false);assert.equal(snapshot.runtime.memoryBytes,null);
 assert.ok(Object.values(snapshot.counters).every(value=>value===null));
 assert.ok(Object.values(snapshot.display).every(value=>value===null));
 assert.deepEqual(snapshot.gpu,{source:'unknown',disjoint:null,microseconds:null,frame:null});
 assert.deepEqual(snapshot.state,{graphics:'unknown',audio:'unknown',visibility:'unknown',focused:null,online:null,fullscreen:null,pointerLocked:null});
 assert.ok(Object.values(snapshot.capabilities).every(value=>value===false));
 assert.doesNotThrow(()=>createDiagnosticSnapshot(null,null,null,null));
});

test('telemetry accepts bounded numeric readings and rejects coercions, fractions for counters and nonfinite values',()=>{
 const {canvas,exports,host}=readings();
 const invalid=[undefined,null,true,false,'',' ',' 2','2 ','-1','NaN','Infinity','0x10','1_000','1e400','9'.repeat(10000),-1,NaN,Infinity,Number.MAX_SAFE_INTEGER+1,{},[],{valueOf(){assert.fail('no coercion');}}];
 for(const value of invalid){
  canvas.dataset.frames=value;canvas.dataset.gpuUs=value;
  const snapshot=createDiagnosticSnapshot(canvas,'neon-swarm',exports,host);
  assert.equal(snapshot.counters.frames,null);assert.equal(snapshot.gpu.microseconds,null);
 }
 for(const value of ['0','42','1e3',Number.MAX_SAFE_INTEGER]){
  canvas.dataset.frames=value;assert.equal(createDiagnosticSnapshot(canvas,'neon-swarm',exports,host).counters.frames,Number(value));
 }
 for(const value of ['2.5',2.5]){
  canvas.dataset.frames=value;canvas.dataset.gpuUs=value;
  const snapshot=createDiagnosticSnapshot(canvas,'neon-swarm',exports,host);
  assert.equal(snapshot.counters.frames,null);assert.equal(snapshot.gpu.microseconds,2.5);
 }
 host.innerWidth=1.5;host.navigator.onLine='false';host.document.hasFocus=()=>1;
 const snapshot=createDiagnosticSnapshot(canvas,'neon-swarm',exports,host);
 assert.equal(snapshot.display.viewportWidth,null);assert.equal(snapshot.state.online,null);assert.equal(snapshot.state.focused,null);
});

test('GPU measurements require an actual available, nondisjoint source and keep zero readings',()=>{
 const {canvas,exports,host}=readings();
 for(const source of ['disabled','unavailable',undefined,'secret driver string']){
  canvas.dataset.gpuSource=source;
  const snapshot=createDiagnosticSnapshot(canvas,'neon-swarm',exports,host);
  assert.equal(snapshot.gpu.microseconds,null);assert.equal(snapshot.gpu.frame,null);
 }
 canvas.dataset.gpuSource='WebGL2 EXT_disjoint_timer_query_webgl2';
 for(const disjoint of ['true',undefined,'bad']){
  canvas.dataset.gpuDisjoint=disjoint;
  const snapshot=createDiagnosticSnapshot(canvas,'neon-swarm',exports,host);
  assert.equal(snapshot.gpu.microseconds,null);assert.equal(snapshot.gpu.frame,null);
 }
 canvas.dataset.gpuDisjoint='false';canvas.dataset.gpuUs='0';canvas.dataset.gpuFrame='0';
 const snapshot=createDiagnosticSnapshot(canvas,'neon-swarm',exports,host);
 assert.equal(snapshot.gpu.microseconds,0);assert.equal(snapshot.gpu.frame,0);
});

test('snapshot never reads saves, URLs, device identities, raw errors or Wasm memory contents',()=>{
 const {canvas,exports,host}=readings();
 const secret='private-peer-token-and-save';
 new Uint8Array(exports.memory.buffer).set(new TextEncoder().encode(secret));
 canvas.dataset.save=secret;canvas.dataset.lightingError=secret;canvas.dataset.materialError=secret;
 canvas.dataset.graphicsState=secret;canvas.dataset.audioState=secret;canvas.dataset.gpuSource=secret;
 const privateReads=[];
 const forbidden=()=>{privateReads.push(true);throw Error('private source read');};
 for(const key of ['location','localStorage','sessionStorage','performance'])Object.defineProperty(host,key,{get:forbidden});
 for(const key of ['userAgent','clipboard','connection'])Object.defineProperty(host.navigator,key,{get:forbidden});
 Object.defineProperty(host.document,'cookie',{get:forbidden});
 Object.defineProperty(exports,'savedGame',{get:forbidden});
 Object.defineProperty(canvas.dataset,'secret',{get:forbidden,enumerable:true});
 const json=JSON.stringify(createDiagnosticSnapshot(canvas,'neon-swarm',exports,host));
 assert.ok(!json.includes(secret));assert.ok(!json.includes('lightingError'));assert.ok(!json.includes('savedGame'));
 for(const game of ['../secret','game?token=secret','https://private.test','é','a'.repeat(65),'','game\n',{}]){
  assert.equal(createDiagnosticSnapshot(canvas,game,exports,host).game,null);
 }
 assert.deepEqual(privateReads,[]);
});

test('inaccessible optional telemetry is isolated and exception messages never enter the snapshot',()=>{
 const {canvas,exports,host}=readings();
 const denied=()=>{throw Error('private-token');};
 Object.defineProperty(canvas.dataset,'drawCalls',{get:denied});
 Object.defineProperty(exports,'memory',{get:denied});
 Object.defineProperty(host,'AudioContext',{get:denied});
 host.document.hasFocus=denied;
 const snapshot=createDiagnosticSnapshot(canvas,'neon-swarm',exports,host);
 assert.equal(snapshot.counters.frames,42);assert.equal(snapshot.counters.drawCalls,null);
 assert.equal(snapshot.runtime.memoryBytes,null);assert.equal(snapshot.state.focused,null);
 assert.equal(snapshot.capabilities.audioContextApi,false);
 assert.ok(!JSON.stringify(snapshot).includes('private-token'));
});

test('fixed schema stays below its UTF-8 bound even at numeric/string limits',()=>{
 const {canvas,exports,host}=readings();
 for(const key of Object.keys(canvas.dataset))if(!['graphicsState','audioState','gpuSource','gpuDisjoint'].includes(key))canvas.dataset[key]=String(Number.MAX_SAFE_INTEGER);
 for(const key of ['width','height','clientWidth','clientHeight'])canvas[key]=Number.MAX_SAFE_INTEGER;
 for(const key of ['innerWidth','innerHeight','devicePixelRatio'])host[key]=Number.MAX_SAFE_INTEGER;
 for(const value of [Number.MAX_SAFE_INTEGER,Number.MIN_VALUE,1.2345678901234568e-200]){
  canvas.dataset.gpuUs=value;canvas.dataset.renderScale=value;
  const snapshot=createDiagnosticSnapshot(canvas,'a'.repeat(64),exports,host);
  assert.equal(snapshot.game.length,64);
  assert.ok(Buffer.byteLength(JSON.stringify(snapshot,null,2),'utf8')<=diagnosticSnapshotMaxBytes);
 }
});

// Minimal first-party DOM/browser doubles exercise bindDetails and its real
// imported player/fullscreen binders. No staged files, GPU or browser is used.
function panel(t){
 const elements=[],downloads=[],blobs=[],revoked=[],timers=new Map();let nextTimer=0;
 const doc=new EventTarget();doc.hidden=false;doc.visibilityState='visible';doc.hasFocus=()=>true;
 class Element extends EventTarget{
  constructor(tag){
   super();this.tagName=tag;this.children=[];this.attributes=new Map();this.dataset={};this.style={};this.disabled=false;this._text='';
   const classes=new Set();this.classList={add:value=>classes.add(value),remove:value=>classes.delete(value),contains:value=>classes.has(value),toggle(value,on){if(on)classes.add(value);else classes.delete(value);}};
   elements.push(this);
  }
  get value(){return this._value??(this.tagName==='select'?this.children[0]?.value:'')??'';}
  set value(value){this._value=String(value);}
  get textContent(){return this._text+this.children.map(child=>child.textContent).join('');}
  set textContent(value){this.replaceChildren();this._text=String(value);}
  append(...nodes){for(const node of nodes){node.parentElement=this;this.children.push(node);}}
  insertBefore(node,before){const at=this.children.indexOf(before);assert.ok(at>=0);node.parentElement=this;this.children.splice(at,0,node);}
  insertAdjacentElement(where,node){assert.equal(where,'beforebegin');this.parentElement.insertBefore(node,this);}
  remove(){if(this.parentElement){const parent=this.parentElement;parent.children.splice(parent.children.indexOf(this),1);this.parentElement=null;}}
  replaceChildren(...nodes){for(const child of this.children)child.parentElement=null;this.children=[];this._text='';this.append(...nodes);}
  contains(node){return node===this||this.children.some(child=>child.contains(node));}
  setAttribute(name,value){this.attributes.set(name,String(value));}
  getAttribute(name){return this.attributes.get(name)??null;}
  focus(){doc.activeElement=this;}
  showModal(){this.open=true;}
  close(){this.open=false;}
  click(){
   if(this.disabled)return;
   if(this.tagName==='a'){
    assert.ok(this.parentElement,'download anchor is temporarily connected');
    if(fail.click)throw Error('private-click-error');
    downloads.push({href:this.href,download:this.download});
   }
   this.dispatchEvent(new Event('click'));
  }
 }
 doc.createElement=tag=>new Element(tag);doc.body=new Element('body');doc.head=new Element('head');doc.documentElement=new Element('html');
 const controls=new Element('button');doc.body.append(controls);
 doc.querySelector=selector=>{
  if(selector==='[data-controls],#controls')return controls;
  if(selector==='[data-audio],#audio')return null;
  if(selector==='.ferravex-details-trigger')return elements.find(node=>node.className==='ferravex-details-trigger');
  if(selector==='.ferravex-details-trigger,[data-controls],#controls')return elements.find(node=>node.className==='ferravex-details-trigger');
  if(selector==='dialog[open]')return elements.find(node=>node.tagName==='dialog'&&node.open)??null;
  assert.fail('Unexpected selector '+selector);
 };
 const canvas=Object.assign(new Element('canvas'),{width:640,height:360,clientWidth:640,clientHeight:360});doc.body.append(canvas);
 canvas.dataset={frames:'3',drawCalls:'5',graphicsState:'lost',audioState:'interrupted'};
 doc.fullscreenElement=null;doc.pointerLockElement=canvas;doc.exitPointerLock=()=>{doc.pointerLockElement=null;};
 const win=new EventTarget();win.parent=win;
 const media=Object.assign(new EventTarget(),{matches:false}),fail={};
 const settings=[0,0,0,0,0,0,0],calls=[];
 const exports={memory:new WebAssembly.Memory({initial:1}),
  ferravex_details_get:index=>settings[index],ferravex_details_set:(index,value)=>{settings[index]=value;},
  ferravex_blur:()=>calls.push('blur'),ferravex_focus:value=>calls.push(['focus',value]),ferravex_details_open:value=>calls.push(['details',value]),
  ferravex_controls_open:()=>0,
 };
 const localStorage={getItem:()=>null,setItem(){assert.fail('diagnostics must not save settings');}};
 const NativeBlob=globalThis.Blob;
 const globals={document:doc,window:win,navigator:{onLine:true},localStorage,matchMedia:()=>media,
  MutationObserver:class{observe(){}disconnect(){}},
  Blob:class extends NativeBlob{
   constructor(...args){if(fail.blob)throw Error('private-blob-error');super(...args);}
   get size(){return fail.size?diagnosticSnapshotMaxBytes+1:super.size;}
  },
  location:{search:'?connect=private-token',href:'https://private.test/?connect=private-token'},
  innerWidth:640,innerHeight:480,devicePixelRatio:1,
  setTimeout:callback=>{if(fail.timer)throw Error('private-timer-error');const id=++nextTimer;timers.set(id,callback);return id;},clearTimeout:id=>timers.delete(id),
 };
 for(const [name,value] of Object.entries(globals)){
  const before=Object.getOwnPropertyDescriptor(globalThis,name);
  Object.defineProperty(globalThis,name,{value,writable:true,configurable:true});
  t.after(()=>{if(before)Object.defineProperty(globalThis,name,before);else delete globalThis[name];});
 }
 t.mock.method(URL,'createObjectURL',blob=>{if(fail.url)throw Error('private-url-error');blobs.push(blob);return `blob:test-${blobs.length}`;});
 t.mock.method(URL,'revokeObjectURL',url=>revoked.push(url));
 const details=bindDetails(canvas,'neon-swarm',()=>{if(fail.exports)throw Error('private-exports-error');return exports;});
 const button=text=>{const node=elements.find(node=>node.tagName==='button'&&node.textContent===text);assert.ok(node,text);return node;};
 const dialog=elements.find(node=>node.tagName==='dialog'),download=button('Download diagnostics JSON');
 const status=download.parentElement.children.find(node=>node.getAttribute('aria-atomic')==='true');
 return {canvas,doc,win,details,elements,downloads,blobs,revoked,timers,fail,button,dialog,download,status,calls,settings,
  flush(){for(const [id,callback] of [...timers]){timers.delete(id);callback();}},
 };
}

test('details dialog downloads fresh bounded JSON and keeps keyboard focus and live announcements',async t=>{
 const h=panel(t),trigger=h.button('Details'),done=h.button('Done');
 assert.equal(trigger.disabled,true);h.details.ready();trigger.click();
 assert.equal(h.dialog.open,true);assert.equal(h.dialog.getAttribute('aria-labelledby'),'ferravex-details-title');
 assert.equal(trigger.getAttribute('aria-expanded'),'true');assert.equal(h.doc.activeElement,done);
 assert.equal(h.doc.pointerLockElement,null);assert.deepEqual(h.calls,['blur',['focus',1],['details',1]]);
 assert.equal(h.download.type,'button');assert.equal(h.download.getAttribute('aria-describedby'),'ferravex-diagnostic-hint');
 assert.equal(h.status.getAttribute('role'),'status');assert.equal(h.status.getAttribute('aria-live'),'polite');assert.equal(h.status.getAttribute('aria-atomic'),'true');
 const calls=[...h.calls];h.download.focus();h.download.click();
 assert.deepEqual(h.calls,calls,'export never changes gameplay, focus exports, or menu state');
 assert.equal(h.doc.activeElement,h.download);assert.equal(h.dialog.open,true);
 assert.equal(h.download.disabled,false);assert.equal(h.download.getAttribute('aria-disabled'),'true');
 assert.equal(h.blobs[0].type,'application/json');assert.ok(h.blobs[0].size<=diagnosticSnapshotMaxBytes);
 assert.deepEqual(h.downloads,[{href:'blob:test-1',download:'ferravex-neon-swarm-diagnostics-v1.json'}]);
 const snapshot=JSON.parse(await h.blobs[0].text());
 assert.equal(snapshot.version,1);assert.equal(snapshot.counters.frames,3);assert.equal(snapshot.counters.drawCalls,5);
 assert.equal(snapshot.state.graphics,'lost');assert.equal(snapshot.state.audio,'interrupted');
 assert.ok(!(await h.blobs[0].text()).includes('private-token'));
 assert.equal(h.status.textContent,'Diagnostic download started.');h.details.update();assert.equal(h.status.textContent,'Diagnostic download started.');
 assert.ok(h.elements.filter(node=>node.tagName==='a'&&node.download).every(node=>!node.parentElement));
 h.download.click();h.download.dispatchEvent(new Event('click'));assert.equal(h.downloads.length,1);assert.equal(h.timers.size,1);
 h.flush();assert.deepEqual(h.revoked,['blob:test-1']);assert.equal(h.download.getAttribute('aria-disabled'),'false');assert.equal(h.timers.size,0);
 h.canvas.dataset.frames='4';h.download.click();assert.equal(JSON.parse(await h.blobs[1].text()).counters.frames,4);
 h.flush();done.click();assert.equal(h.dialog.open,false);assert.equal(trigger.getAttribute('aria-expanded'),'false');assert.equal(h.doc.activeElement,h.canvas);
 assert.deepEqual(h.calls.slice(-3),[['details',0],'blur',['focus',1]]);
});

test('Escape and dialog key isolation preserve the existing modal contract',t=>{
 const h=panel(t);h.details.ready();h.button('Details').click();
 const key=new Event('keydown',{bubbles:true});const stop=t.mock.method(key,'stopPropagation');h.dialog.dispatchEvent(key);assert.equal(stop.mock.callCount(),1);
 const cancel=new Event('cancel',{cancelable:true});h.dialog.dispatchEvent(cancel);
 assert.equal(cancel.defaultPrevented,true);assert.equal(h.details.isOpen(),false);assert.equal(h.doc.activeElement,h.canvas);
});

for(const stage of ['exports','blob','size','url','click','timer'])test(`download ${stage} failure is announced safely and a retry works`,t=>{
 const h=panel(t);h.fail[stage]=true;h.download.click();
 assert.match(h.status.textContent,/download unavailable/);assert.ok(!h.status.textContent.includes('private'));
 assert.equal(h.download.getAttribute('aria-disabled'),'false');assert.equal(h.timers.size,0);assert.equal(h.downloads.length,0);
 assert.deepEqual(h.revoked,stage==='click'||stage==='timer'?['blob:test-1']:[]);
 assert.ok(h.elements.filter(node=>node.tagName==='a'&&node.download).every(node=>!node.parentElement));
 h.details.update();assert.match(h.status.textContent,/download unavailable/);
 h.fail[stage]=false;h.download.click();assert.equal(h.downloads.length,1);h.flush();assert.equal(h.download.getAttribute('aria-disabled'),'false');
});

test('pagehide releases the single pending URL/timer and permits a new export on return',t=>{
 const h=panel(t);h.download.click();assert.equal(h.timers.size,1);
 h.win.dispatchEvent(new Event('pagehide'));assert.equal(h.timers.size,0);assert.deepEqual(h.revoked,['blob:test-1']);assert.equal(h.download.getAttribute('aria-disabled'),'false');
 h.win.dispatchEvent(new Event('pagehide'));assert.deepEqual(h.revoked,['blob:test-1']);
 h.download.click();h.flush();assert.deepEqual(h.revoked,['blob:test-1','blob:test-2']);
});

// Default invocation still needs dist/web/*.wasm; --source-only never reads it.
if(!process.argv.includes('--source-only')){
class Target{addEventListener(){}}
const games=JSON.parse(fs.readFileSync(new URL('../../examples/catalog.json',import.meta.url)));
for(const game of games){
 const storage=new Map();
 function runtime(){let e;const input=bindInput(new Target(),game.id,()=>e,()=>false,{window:new Target(),document:new Target(),navigator:{},storage:()=>({getItem:k=>storage.get(k)??null,setItem:(k,v)=>storage.set(k,v)}),atob,btoa});
  const bytes=fs.readFileSync(new URL(`../../dist/web/${game.host}/${game.id.replaceAll('-','_')}.wasm`,import.meta.url));e=new WebAssembly.Instance(new WebAssembly.Module(bytes),{env:{...input.imports,ferravex_set_playing(){}}}).exports;(e.ferravex_init??e.init)();
  const frame=()=>game.id==='owned-orbit'?e.orbit_controls_frame(1/60,390,320):game.id==='owned-lab'?e.ferravex_controls_step(1/60,0,1,390/320,390,320):e.ferravex_frame(1/60,390,320);
  frame();return {e,frame};
 }
 let r=runtime();const values=[1,5,3,4,2];r.e.ferravex_details_open(1);r.frame();assert.equal(r.e.ferravex_controls_open(),1,game.id+' pauses gameplay for external details');
 r.e.ferravex_key(0xffbf,1);r.e.ferravex_key(0xffbf,0);r.frame();assert.equal(r.e.ferravex_controls_open(),1,'F2 cannot dismiss the modal pause');
 r.e.ferravex_input_settings();r.frame();assert.equal(r.e.ferravex_controls_open(),1,'a second controls request cannot dismiss the modal pause');
 for(let i=0;i<values.length;i++)assert.equal(r.e.ferravex_details_set(i,values[i]),1);
 assert.equal(r.e.ferravex_details_set(1,6),0);assert.equal(r.e.ferravex_details_set(99,0),0);r.frame();
 assert.equal(r.e.ferravex_resolution(),5);assert.equal(r.e.ferravex_shadow_detail(),3);assert.equal(r.e.ferravex_visual_quality(),0);assert.equal(r.e.ferravex_visual_look(),2);
 const saved=new Map(storage);for(let i=0;i<30;i++)r.frame();assert.deepEqual(storage,saved,'idle frames do not write settings');
 r.e.ferravex_details_open(0);r.frame();assert.equal(r.e.ferravex_controls_open(),0);
 r=runtime();for(let i=0;i<values.length;i++)assert.equal(r.e.ferravex_details_get(i),values[i],game.id+' reloads independent fidelity');
 if(game.id==='crystal-cavern'){
  const at=r.e.ferravex_light_environment(),view=new DataView(r.e.memory.buffer);assert.equal(view.getUint32(at+368,true),3,'explicit local lights exceed the Low preset cap');
  r.e.ferravex_details_set(3,1);r.frame();assert.equal(new DataView(r.e.memory.buffer).getUint32(r.e.ferravex_light_environment()+368,true),0,'explicit lights Off');
 }
 if(game.id==='strike'||game.id==='dune-skimmer')assert.equal(new DataView(r.e.memory.buffer).getUint32(r.e.ferravex_light_environment()+372,true),1024,'High shadows independent of Low lighting');
 console.log(game.id+' shared Details, pause, independent density/lighting/shadows and persistence passed');
}
}
