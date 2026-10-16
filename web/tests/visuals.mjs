// Exercise the actual settings UI, persistence and explicit shader overrides in every game.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import {bindInput} from '../input.js';
import {configureVisuals} from '../visuals.js';
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
 r.e.ferravex_pointer(965,482,0,0);r.e.ferravex_button(1,1);r.frame();r.e.ferravex_button(1,0);r.frame();
 assert.equal(r.e.ferravex_visual_look(),1,game.id+' visual style UI changes the renderer');
 r=runtime();r.frame();assert.equal(r.e.ferravex_visual_look(),1,game.id+' visual style survives reload');
 const saved=new Map(data);configureVisuals(r.e,'?look=ink&quality=low');r.frame();
 assert.equal(r.e.ferravex_visual_look(),3);assert.equal(r.e.ferravex_visual_quality(),0);assert.deepEqual(data,saved,'session overrides preserve preferences');
 assert.equal(r.e.ferravex_look_override(4),0);assert.equal(r.e.ferravex_visual_look(),3);
 console.log(game.id+' actual-Wasm style selection, persistence and low-quality override passed');
}
assert.throws(()=>configureVisuals({},'?look=garbage'),/look must be/);
