// Real release Wasm: optional adaptation, fixed user fidelity and bounded CSVs.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import {bindInput} from '../input.js';
import {framebufferSize} from '../details.js';
class Target {addEventListener(){}}
for(const game of JSON.parse(fs.readFileSync(new URL('../../examples/catalog.json',import.meta.url)))) {
 const storage=new Map();
 const module=new WebAssembly.Module(fs.readFileSync(new URL(`../../dist/web/${game.host}/${game.id.replaceAll('-','_')}.wasm`,import.meta.url)));
 function runtime() {
  let e,reads=0,writes=0;
  const input=bindInput(new Target(),game.id,()=>e,()=>false,{window:new Target(),document:new Target(),navigator:{},storage:()=>({getItem:k=>{reads++;return storage.get(k)??null;},setItem:(k,v)=>{writes++;storage.set(k,v);}}),atob,btoa});
  e=new WebAssembly.Instance(module,{env:{...input.imports,ferravex_set_playing(){}}}).exports;(e.ferravex_init??e.init)();
  const frame=()=>game.id==='owned-orbit'?e.orbit_controls_frame(1/60,390,320):game.id==='owned-lab'?e.ferravex_controls_step(1/60,0,1,390/320,390,320):e.ferravex_frame(1/60,390,320);
  frame();e.ferravex_focus(1);
  return {e,frame,counts:()=>({reads,writes})};
 }
 let r=runtime(),e=r.e;
 const pressure=(gpu=30,cpu=2)=>{for(let i=0;i<600;i++)assert.equal(e.ferravex_performance_sample(33,cpu,gpu),1);};
 assert.equal(e.ferravex_details_get(6),0,game.id+' adaptation defaults off');
 pressure();assert.equal(e.ferravex_adaptive_scale(),1,'disabled adaptation keeps user fidelity');
 assert.equal(e.ferravex_details_set(6,1),1);r.frame();
 assert.equal(e.ferravex_details_get(6),1,game.id+' Details preference survives PlayerInput frame');
 assert.ok(storage.has(`ferravex.save.v1.${game.id}.preferences`),'explicit change persists');
 const saved=new Map(storage);
 r=runtime();e=r.e;
 assert.equal(e.ferravex_details_get(6),1,game.id+' fresh instance reloads adaptive preference');
 const resolution=e.ferravex_resolution(),counts=r.counts();
 e.ferravex_details_open(1);r.frame();pressure();assert.equal(e.ferravex_adaptive_scale(),1,'Details pauses adaptation');
 e.ferravex_details_open(0);r.frame();
 pressure(-1);assert.equal(e.ferravex_adaptive_scale(),1,'missing GPU timers never lower fidelity');
 pressure(30,30);assert.equal(e.ferravex_adaptive_scale(),1,'CPU overload never lowers fidelity');
 pressure();assert.equal(e.ferravex_adaptive_scale(),.5,game.id+' confirmed GPU pressure reduces presentation');
 assert.equal(e.ferravex_resolution(),resolution);assert.deepEqual(storage,saved,'adaptation never writes saved preferences');
 assert.deepEqual(r.counts(),counts,'ordinary frames and adaptation do no storage I/O');
 const ptr=e.ferravex_diagnostics_export(),len=e.ferravex_diagnostics_len();assert.ok(len<65536);
 const csv=new TextDecoder().decode(new Uint8Array(e.memory.buffer,ptr,len));assert.ok(csv.startsWith('frame,tick,replay,kind'));assert.ok(csv.includes('fidelity'));
 assert.equal(e.ferravex_performance_sample(NaN,1,-1),0);
 assert.equal(e.ferravex_details_set(6,0),1);r.frame();assert.equal(e.ferravex_adaptive_scale(),1);
 r=runtime();e=r.e;assert.equal(e.ferravex_details_get(6),0,'disabled preference also survives reload');
 pressure();assert.equal(e.ferravex_adaptive_scale(),1);
 console.log(game.id+' adaptation, fresh-instance persistence, idle I/O and local export passed');
}
const original=framebufferSize(390,320,3,0),adaptive=framebufferSize(390,320,3,0,8192,.5);
assert.equal(adaptive.scale,original.scale*.5);assert.ok(framebufferSize(390,320,1,1,8192,.5).scale>=.5);
