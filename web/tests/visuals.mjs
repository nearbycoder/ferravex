// Source-only checks: node web/tests/visuals.mjs --source-only
// With no flag, also exercise the original staged-Wasm UI/persistence checks.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import {bindInput} from '../input.js';
import {bindVisualPreferences,configureVisuals} from '../visuals.js';

const args=process.argv.slice(2);
assert.ok(args.every(arg=>arg==='--source-only'),'Only --source-only is supported');
let passed=0;
function test(name,run){run();passed++;console.log('ok - '+name);}

class VisualTarget extends EventTarget {
 listeners=new Map();
 addEventListener(type,callback){if(!this.listeners.has(type))this.listeners.set(type,new Set());this.listeners.get(type).add(callback);super.addEventListener(type,callback);}
 removeEventListener(type,callback){this.listeners.get(type)?.delete(callback);super.removeEventListener(type,callback);}
 count(){return [...this.listeners.values()].reduce((sum,callbacks)=>sum+callbacks.size,0);}
 emit(type){const event=new Event(type,{cancelable:true});this.dispatchEvent(event);return event;}
}
function fixture(settings={reducedMotion:'system',reducedFlash:false,quality:1}) {
 const canvas=new VisualTarget(),window=new VisualTarget(),motion={matches:false};
 canvas.width=640;canvas.height=480;
 const h={canvas,window,motion,settings,applied:[],reads:0,readError:null,applyError:null};
 window.matchMedia=query=>{assert.equal(query,'(prefers-reduced-motion: reduce)');return motion;};
 h.visuals=bindVisualPreferences(canvas,()=>{h.reads++;if(h.readError)throw h.readError;return h.settings;},value=>{if(h.applyError)throw h.applyError;h.applied.push(value);},{window});
 return h;
}

test('URL overrides validate the whole query before changing the last good settings',()=>{
 const calls=[],state={look:1,quality:2};
 const exports={ferravex_look_override(value){calls.push('look');state.look=value;return 1;},ferravex_quality_override(value){calls.push('quality');state.quality=value;return 1;}};
 for(const query of ['?look=ink&quality=garbage','?look=ink&quality=','?look=bad&quality=low']){
  assert.throws(()=>configureVisuals(exports,query),/must be/);
  assert.deepEqual(state,{look:1,quality:2});assert.deepEqual(calls,[]);
 }
 // Execute the documented configureVisuals example against its existing ABI.
 configureVisuals(exports,'?look=ink&quality=low');
 assert.deepEqual(state,{look:3,quality:0});assert.deepEqual(calls,['look','quality']);
});

test('missing exports cannot partially apply a valid query',()=>{
 let writes=0;
 assert.throws(()=>configureVisuals({ferravex_look_override(){writes++;}},'?look=cool&quality=high'),/Missing visual export: ferravex_quality_override/);
 assert.throws(()=>configureVisuals({},'?look=warm'),/Missing visual export: ferravex_look_override/);
 assert.equal(writes,0);
 configureVisuals({},'?unrelated=1');
});

test('all existing URL choices, first-value handling and omitted overrides stay compatible',()=>{
 const state={};
 const exports={ferravex_look_override(value){state.look=value;},ferravex_quality_override(value){state.quality=value;}};
 for(const [index,look] of ['authored','warm','cool','ink'].entries()){configureVisuals(exports,'?look='+look);assert.equal(state.look,index);}
 for(const [index,quality] of ['low','medium','high'].entries()){configureVisuals(exports,'?quality='+quality);assert.equal(state.quality,index);}
 configureVisuals(exports,'?look=warm&look=invalid&quality=medium');
 configureVisuals(exports,'');assert.deepEqual(state,{look:1,quality:1});
});

test('documented live snapshot example and unchanged frames',()=>{
 const h=fixture({reducedMotion:'system',reducedFlash:true,quality:1});
 assert.equal(h.visuals.current,null);assert.equal(h.applied.length,0);
 assert.equal(h.visuals.refresh(),true);
 h.settings={...h.settings,quality:0};assert.equal(h.visuals.refresh(),true);
 assert.deepEqual(h.visuals.current,{reducedMotion:false,reducedFlash:true,quality:0});
 for(let i=0;i<100;i++)assert.equal(h.visuals.refresh(),true);
 assert.equal(h.applied.length,2,'unchanged frames do not reapply settings');
 assert.equal(Object.isFrozen(h.visuals.current),true);
 assert.throws(()=>{h.visuals.current.quality=2;},TypeError);
 h.visuals.dispose();
});

test('system motion is live, explicit motion wins, and flash stays independent',()=>{
 const h=fixture();h.visuals.refresh();
 h.motion.matches=true;h.visuals.refresh();
 assert.deepEqual(h.visuals.current,{reducedMotion:true,reducedFlash:false,quality:1});
 h.settings={...h.settings,reducedMotion:false,reducedFlash:true};h.visuals.refresh();
 assert.deepEqual(h.visuals.current,{reducedMotion:false,reducedFlash:true,quality:1});
 const count=h.applied.length;h.motion.matches=false;h.visuals.refresh();assert.equal(h.applied.length,count);
 h.settings.reducedMotion=true;h.visuals.refresh();assert.equal(h.visuals.current.reducedMotion,true);
 h.settings.reducedMotion='system';h.visuals.refresh();assert.equal(h.visuals.current.reducedMotion,false);
 h.visuals.dispose();
});

test('quality presets preserve motion and flash through window and canvas resize',()=>{
 const h=fixture({reducedMotion:true,reducedFlash:true,quality:2});h.visuals.refresh();
 for(const quality of [0,1,2]){
  h.settings={...h.settings,quality};h.visuals.refresh();
  const expected={reducedMotion:true,reducedFlash:true,quality};
  assert.deepEqual(h.visuals.current,expected);
  let count=h.applied.length;h.window.emit('resize');h.window.emit('resize');
  assert.equal(h.applied.length,count,'resize only invalidates; application waits for the host');
  h.visuals.refresh();assert.equal(h.applied.length,count+1);assert.deepEqual(h.visuals.current,expected);
  count=h.applied.length;h.canvas.width++;h.visuals.refresh();assert.equal(h.applied.length,count+1);
  count=h.applied.length;h.canvas.height++;h.visuals.refresh();assert.equal(h.applied.length,count+1);
 }
 h.visuals.dispose();
});

test('restoration waits for graphics rebuild and uses the latest preferences',()=>{
 const h=fixture();h.visuals.refresh();
 assert.equal(h.canvas.emit('webglcontextlost').defaultPrevented,false,'graphics lifecycle owns cancellation');
 h.settings={reducedMotion:true,reducedFlash:true,quality:0};
 assert.equal(h.visuals.refresh(),true);assert.equal(h.applied.length,1,'no calls into a lost renderer');
 h.canvas.width=1280;h.window.emit('resize');h.visuals.refresh();assert.equal(h.applied.length,1);
 h.canvas.emit('webglcontextrestored');assert.equal(h.applied.length,1,'the host finishes rebuilding before refresh');
 h.settings={...h.settings,quality:2};h.visuals.refresh();
 assert.deepEqual(h.visuals.current,{reducedMotion:true,reducedFlash:true,quality:2});assert.equal(h.applied.length,2);
 for(let i=0;i<3;i++){
  h.canvas.emit('webglcontextlost');h.canvas.emit('webglcontextrestored');
  const count=h.applied.length;h.visuals.refresh();assert.equal(h.applied.length,count+1);
  assert.deepEqual(h.visuals.current,{reducedMotion:true,reducedFlash:true,quality:2});
 }
 h.visuals.dispose();
});

test('all motion, flash and quality combinations survive a recovery cycle',()=>{
 const h=fixture();
 for(const reducedMotion of [false,true])for(const reducedFlash of [false,true])for(const quality of [0,1,2]){
  h.settings={reducedMotion,reducedFlash,quality};assert.equal(h.visuals.refresh(),true);
  h.canvas.width++;h.window.emit('resize');h.visuals.refresh();
  h.canvas.emit('webglcontextlost');h.canvas.emit('webglcontextrestored');h.visuals.refresh();
  assert.deepEqual(h.visuals.current,h.settings);
 }
 h.visuals.dispose();
});

test('valid updates during loss are retained when the restoration read fails',()=>{
 const h=fixture();h.visuals.refresh();h.canvas.emit('webglcontextlost');
 h.settings={reducedMotion:true,reducedFlash:true,quality:0};assert.equal(h.visuals.refresh(),true);
 h.settings=null;h.canvas.emit('webglcontextrestored');assert.equal(h.visuals.refresh(),false);
 assert.deepEqual(h.visuals.current,{reducedMotion:true,reducedFlash:true,quality:0});
 assert.equal(h.applied.length,2);h.visuals.dispose();
});

test('invalid complete snapshots retain the last valid snapshot across resize and restore',()=>{
 const h=fixture({reducedMotion:true,reducedFlash:true,quality:2});h.visuals.refresh();
 const good=h.visuals.current;
 for(const invalid of [null,[],{},
  {reducedMotion:1,reducedFlash:false,quality:0},
  {reducedMotion:false,reducedFlash:'false',quality:0},
  ...[-1,3,NaN,Infinity,1.5,'1',null].map(quality=>({reducedMotion:false,reducedFlash:false,quality})),
 ]){
  h.settings=invalid;const count=h.applied.length;
  assert.equal(h.visuals.refresh(),false);assert.ok(h.visuals.error instanceof Error);
  assert.equal(h.applied.length,count);assert.deepEqual(h.visuals.current,good);
  h.window.emit('resize');assert.equal(h.visuals.refresh(),false);assert.equal(h.applied.length,count+1);
  h.canvas.emit('webglcontextlost');assert.equal(h.visuals.refresh(),false);
  h.canvas.emit('webglcontextrestored');assert.equal(h.visuals.refresh(),false);
  assert.deepEqual(h.visuals.current,good);assert.equal(h.applied.length,count+2);
 }
 h.settings={reducedMotion:false,reducedFlash:false,quality:0};assert.equal(h.visuals.refresh(),true);
 assert.equal(h.visuals.error,null);assert.deepEqual(h.visuals.current,h.settings);
 h.visuals.dispose();
});

test('a throwing read cannot install partial values or alias a mutable source object',()=>{
 const h=fixture();h.visuals.refresh();
 h.settings.quality=99;
 h.readError=new Error('Preferences unavailable');h.window.emit('resize');
 assert.equal(h.visuals.refresh(),false);assert.equal(h.visuals.error,h.readError);assert.equal(h.visuals.current.quality,1);
 h.motion.matches=true;assert.equal(h.visuals.refresh(),false);
 assert.equal(h.visuals.current.reducedMotion,true,'last valid system choice still follows live OS preference');
 h.readError=null;h.settings={reducedMotion:true,get reducedFlash(){throw new Error('Incomplete read');},quality:0};
 assert.equal(h.visuals.refresh(),false);assert.match(h.visuals.error.message,/Incomplete read/);
 assert.deepEqual(h.visuals.current,{reducedMotion:true,reducedFlash:false,quality:1});
 h.visuals.dispose();
});

test('invalid initial reads do not invent defaults and can recover',()=>{
 const h=fixture(null);assert.equal(h.visuals.refresh(),false);assert.equal(h.applied.length,0);assert.equal(h.visuals.current,null);
 h.settings={reducedMotion:false,reducedFlash:true,quality:0};assert.equal(h.visuals.refresh(),true);
 assert.deepEqual(h.visuals.current,h.settings);assert.equal(h.visuals.error,null);
 h.visuals.dispose();
});

test('apply failures report errors and retry without advancing the applied snapshot',()=>{
 const h=fixture();h.visuals.refresh();const previous=h.visuals.current;
 h.settings={reducedMotion:true,reducedFlash:true,quality:0};h.applyError=new Error('Renderer rebuilding');
 assert.equal(h.visuals.refresh(),false);assert.equal(h.visuals.current,previous);assert.equal(h.visuals.error,h.applyError);
 h.applyError=null;assert.equal(h.visuals.refresh(),true);assert.deepEqual(h.visuals.current,h.settings);
 assert.equal(h.visuals.error,null);const count=h.applied.length;
 assert.equal(h.visuals.refresh(true),true);assert.equal(h.applied.length,count+1,'explicit invalidation covers same-size target resets');
 h.visuals.dispose();
});

test('disposal removes bounded listeners and never reads or applies again',()=>{
 const h=fixture();h.visuals.refresh();assert.equal(h.canvas.count(),2);assert.equal(h.window.count(),1);
 h.visuals.dispose();h.visuals.dispose();assert.equal(h.canvas.count(),0);assert.equal(h.window.count(),0);
 const reads=h.reads,count=h.applied.length;
 h.window.emit('resize');h.canvas.emit('webglcontextrestored');h.motion.matches=true;
 assert.equal(h.visuals.refresh(true),false);assert.equal(h.reads,reads);assert.equal(h.applied.length,count);
});

test('no media API is required and callbacks/event targets are checked',()=>{
 const canvas=new VisualTarget(),window=new VisualTarget();
 canvas.ownerDocument={defaultView:window};
 const visuals=bindVisualPreferences(canvas,()=>({reducedMotion:'system',reducedFlash:true,quality:0}),()=>{});
 visuals.refresh();assert.deepEqual(visuals.current,{reducedMotion:false,reducedFlash:true,quality:0});
 visuals.dispose();assert.equal(window.count(),0);
 assert.throws(()=>bindVisualPreferences({},()=>{},()=>{}),/visual event target/);
 assert.throws(()=>bindVisualPreferences(canvas,null,()=>{}),/callbacks are required/);
 assert.throws(()=>bindVisualPreferences(canvas,()=>{},null),/callbacks are required/);
 assert.equal(canvas.count(),0);
});

console.log(`${passed} source-only visual preference checks passed`);

if(!args.includes('--source-only')){
// Exercise the actual settings UI, persistence and explicit shader overrides in every game.
class Target {addEventListener(){}}
const catalog=JSON.parse(fs.readFileSync(new URL('../../examples/catalog.json',import.meta.url)));
for(const game of catalog){
 const data=new Map();
 function runtime(){
  let e;const input=bindInput(new Target(),game.id,()=>e,()=>false,{window:new Target(),document:new Target(),navigator:{},storage:()=>({getItem:k=>data.get(k)??null,setItem:(k,v)=>data.set(k,v)}),atob,btoa});
  const module=new WebAssembly.Module(fs.readFileSync(new URL(`../../dist/web/${game.host}/${game.id.replaceAll('-','_')}.wasm`,import.meta.url)));
  e=new WebAssembly.Instance(module,{env:{...input.imports,ferravex_set_playing(){}}}).exports;(e.ferravex_init??e.init)();
  const frame=()=>game.id==='owned-orbit'?e.orbit_controls_frame(1/60,1280,720):game.id==='owned-lab'?e.ferravex_controls_step(1/60,0,1,16/9,1280,720):e.ferravex_frame(1/60,1280,720);
  const key=code=>{e.ferravex_key(code,1);e.ferravex_key(code,0);frame();};
  return {e,frame,key};
 }
 let r=runtime();r.frame();assert.equal(r.e.ferravex_visual_look(),0);
 r.key(0xffbf);r.key(0xff09);r.key(0xff09);r.key(0xff0d);
 for(let i=0;i<4;i++)r.key(0xff52);r.key(0xff0d);
 // The readable paged editor resets focus to Done after Next. At this
 // 1280x720 viewport, Visual style is row seven on Devices page two; select
 // its + action by keyboard instead of clicking an obsolete row coordinate.
 for(let i=0;i<18;i++)r.key(0xff09);r.key(0xff0d);
 assert.equal(r.e.ferravex_visual_look(),1,game.id+' visual style UI changes the renderer');
 r=runtime();r.frame();assert.equal(r.e.ferravex_visual_look(),1,game.id+' visual style survives reload');
 const saved=new Map(data);configureVisuals(r.e,'?look=ink&quality=low');r.frame();
 assert.equal(r.e.ferravex_visual_look(),3);assert.equal(r.e.ferravex_visual_quality(),0);assert.deepEqual(data,saved,'session overrides preserve preferences');
 assert.equal(r.e.ferravex_look_override(4),0);assert.equal(r.e.ferravex_visual_look(),3);
 console.log(game.id+' actual-Wasm style selection, persistence and low-quality override passed');
}
assert.throws(()=>configureVisuals({},'?look=garbage'),/look must be/);
}
