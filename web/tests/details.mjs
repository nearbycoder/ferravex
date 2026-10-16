// Real game builds: shared detail setters, bounded framebuffers and durable preferences.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import {bindInput} from '../input.js';
import {framebufferSize} from '../details.js';
assert.deepEqual(framebufferSize(390,260,3,0),{width:1170,height:780,scale:3});
assert.deepEqual(framebufferSize(390,260,3,2),{width:390,height:260,scale:1});
assert.deepEqual(framebufferSize(390,260,3,3),{width:585,height:390,scale:1.5});
for(const [w,h,dpr,setting,limit] of [[10000,9000,4,5,4096],[390,844,3,0,2048],[NaN,Infinity,NaN,99,NaN]]){
 const size=framebufferSize(w,h,dpr,setting,limit);assert.ok(size.width>=1&&size.height>=1&&size.width<=8192&&size.height<=8192);assert.ok(size.width*size.height<=8*1024*1024);
}
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
