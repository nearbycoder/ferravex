// Production Wasm touch input, using the actual painted 44-pixel targets.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import test from 'node:test';
import {readStatusV1} from '../status.js';
const games=JSON.parse(readFileSync(new URL('../../docs/games20/plan.json',import.meta.url)));
const owner=id=>{
 let hash=0xcbf29ce484222325n;
 for(const byte of new TextEncoder().encode(id))hash=BigInt.asUintN(64,(hash^BigInt(byte))*0x100000001b3n);
 return (hash||1n).toString();
};
function runtime(id,w,h){
 const bytes=readFileSync(new URL(`../../dist/web/${id}/${id.replaceAll('-','_')}.wasm`,import.meta.url));
 const e=new WebAssembly.Instance(new WebAssembly.Module(bytes),{env:{ferravex_save_len:()=>0,ferravex_save_load:()=>-1,ferravex_save_store:()=>-1,ferravex_controls_len:()=>0,ferravex_controls_load:()=>-1,ferravex_controls_store:()=>-1,ferravex_set_playing(){}}}).exports;
 e.ferravex_init();e.ferravex_autoplay(0);
 const frame=(dt=1/60)=>{e.ferravex_frame(dt,w,h);return readStatusV1(()=>e,{owner:owner(id)});};
 const state=s=>s.items.find(x=>x.kind==='lifecycle').label;
 const facts=s=>s.items.filter(x=>x.kind!=='lifecycle');
 const tap=(x,y)=>{
  e.ferravex_touch_pointer(x,y);e.ferravex_touch(7,0,x,y);frame();
  e.ferravex_touch(7,2,x,y);return frame();
 };
 const buttons=()=>{
  const v=new Float32Array(e.memory.buffer,e.ferravex_vertices(),e.ferravex_vertex_count()*8),result=[];
  const near=(a,b)=>Math.abs(a-b)<1e-4;
  for(let at=0;at+48<=v.length;at+=8){
   if(!Array.from({length:6},(_,i)=>at+i*8).every(p=>near(v[p+4],.1)&&near(v[p+5],.17)&&near(v[p+6],.22)))continue;
   const points=Array.from({length:6},(_,i)=>[(v[at+i*8]/v[at+i*8+3]+1)*w/2,(1-v[at+i*8+1]/v[at+i*8+3])*h/2]);
   const xs=points.map(p=>p[0]),ys=points.map(p=>p[1]);
   const left=Math.min(...xs),right=Math.max(...xs),top=Math.min(...ys),bottom=Math.max(...ys);
   if(!near(bottom-top,44)||right-left<43.99)continue;
   assert.ok(left>=-.01&&right<=w+.01&&top>=-.01&&bottom<=h+.01,'touch target stays on screen');
   result.push([(left+right)/2,(top+bottom)/2]);at+=40;
  }
  assert.equal(result.length,14,'all game commands have visible finger-sized targets');return result;
 };
 const logical=(x,y,inset=0)=>{
  const scale=Math.min(w/960,(h-inset)/640);
  return [(w-960*scale)/2+x*scale,(h-inset-640*scale)/2+y*scale];
 };
 return {e,frame,state,facts,tap,buttons,logical};
}
for(const {id} of games)test(`${id}: touch starts, pauses, reads help and resumes at phone sizes`,()=>{
 for(const [w,h] of [[390,844],[844,390],[320,480]]){
  const r=runtime(id,w,h);assert.equal(r.state(r.frame()),'Ready');
  // A real canvas tap starts the title menu before any keyboard or virtual key.
  const started=r.tap(...r.logical(300,508));assert.equal(r.state(started),'Playing');
  const buttons=r.buttons();
  r.tap(...buttons[12]);const paused=r.frame();assert.equal(r.state(paused),'Paused');
  assert.deepEqual(r.facts(r.frame(.1)),r.facts(paused),'pause discards elapsed game time');
  r.tap(...buttons[13]);const guide=r.frame();assert.equal(r.state(guide),'Help');
  r.tap(...buttons[3]);assert.deepEqual(r.facts(r.frame(.1)),r.facts(guide),'guide arrows never move the game');
  r.tap(...buttons[13]);assert.equal(r.state(r.frame()),'Paused');
  r.tap(...buttons[4]);assert.equal(r.state(r.frame()),'Playing');
 }
});
test('Prism Foundry: direct touch rotates both mirrors and a touch choice seals the chamber',()=>{
 const r=runtime('prism-foundry',844,390);r.frame();r.tap(...r.logical(300,508));
 const controls=r.buttons();
 r.tap(...r.logical(262,446));r.tap(...r.logical(262,238));
 const sealed=r.tap(...controls[11]);
 assert.match(sealed.items.find(x=>x.kind==='telemetry').label,/Score 1750 /);
 r.tap(...controls[12]);assert.equal(r.state(r.frame()),'Paused');
 const restarted=r.tap(...r.logical(400,424));
 assert.equal(r.state(restarted),'Playing');
 assert.match(restarted.items.find(x=>x.kind==='telemetry').label,/Score 0 /);
 assert.ok(BigInt(restarted.epoch)>BigInt(sealed.epoch),'the pause card starts a fresh campaign by touch');
});
