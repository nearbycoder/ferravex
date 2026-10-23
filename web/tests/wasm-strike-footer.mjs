// Actual released render buffers: the replay panel and its shadowed prompts
// must clear the painted health/ammo digits. Fixed supplied deltas and ordinary
// input/replay commands are used; no simulation state or outcomes are written.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import {createHash} from 'node:crypto';
const path=process.argv[2]??new URL('../../dist/web/strike/strike.wasm',import.meta.url);
const bytes=fs.readFileSync(path),module=new WebAssembly.Module(bytes);
console.log('Wasm SHA256',createHash('sha256').update(bytes).digest('hex'));
const near=(a,b)=>Math.abs(a-b)<1e-4;
for(const [width,height] of [[320,480],[390,608],[1280,576]]){
 const e=new WebAssembly.Instance(module,{env:{
  ferravex_save_len:()=>0,ferravex_save_load:()=>-1,ferravex_save_store:()=>0,
  ferravex_controls_len:()=>0,ferravex_controls_load:()=>-1,ferravex_controls_store:()=>0,
  ferravex_set_playing(){},
 }}).exports;
 e.ferravex_init();
 const frame=()=>e.ferravex_frame(1/64,width,height);
 const key=code=>{e.ferravex_key(code,1);e.ferravex_key(code,0);frame();};
 frame();key(0xff0d);e.ferravex_replay_command(1);frame();
 assert.equal(e.ferravex_replay_status()&1,1,'ordinary recording makes status visible');
 const v=new Float32Array(e.memory.buffer,e.ferravex_vertices(),e.ferravex_vertex_count()*8);
 const color=(at,c)=>c.every((n,j)=>near(v[at+4+j],n));
 const rect=at=>{
  const points=Array.from({length:6},(_,n)=>[(v[at+n*8]+1)*width/2,(1-v[at+n*8+1])*height/2]);
  return {left:Math.min(...points.map(p=>p[0])),right:Math.max(...points.map(p=>p[0])),top:Math.min(...points.map(p=>p[1])),bottom:Math.max(...points.map(p=>p[1]))};
 };
 let panelAt=-1;
 for(let at=0;at+48<=v.length;at+=8){
  if(!Array.from({length:6},(_,n)=>at+n*8).every(i=>color(i,[.04,.065,.055,.88])))continue;
  const r=rect(at);
  if(r.right-r.left>width*.8&&r.bottom-r.top>80){panelAt=at;break;}
 }
 assert.ok(panelAt>=0,'the real replay status panel is painted');
 const panel=rect(panelAt),vitals={health:[],ammo:[]};
 // The painter emits a shadow quad followed by its corresponding ink quad.
 // Pair their cell sizes and offsets to exclude icons/world polygons.
 for(let at=0;at+96<=panelAt;at+=8){
  if(!Array.from({length:6},(_,n)=>at+n*8).every(i=>color(i,[0,0,0,.7])))continue;
  if(!Array.from({length:6},(_,n)=>at+48+n*8).every(i=>color(i,[.94,.93,.87,1])))continue;
  const shadow=rect(at),ink=rect(at+48);
  if(!near(ink.bottom-ink.top,3.75)||!near(shadow.top-ink.top,1.875)||!near(shadow.left-ink.left,1.875))continue;
  if(ink.top<height-70)continue;
  const group=ink.left>=49.99&&ink.right<140?'health':ink.left>width-140&&ink.right<=width-44.99?'ammo':null;
  if(group)vitals[group].push(ink,shadow);
 }
 const tops={};
 for(const [name,rects] of Object.entries(vitals)){
  assert.ok(rects.length>=20,`actual ${name} glyphs and shadows are present`);
  tops[name]=Math.min(...rects.map(r=>r.top));
  assert.ok(panel.bottom+4<=tops[name],`${width}x${height}: replay panel bottom ${panel.bottom} overlaps or crowds ${name} at ${tops[name]}`);
 }
 const prompts=[];
 for(let at=panelAt+48;at+48<=v.length;at+=8){
  if(!Array.from({length:6},(_,n)=>at+n*8).every(i=>color(i,[.9,.85,.65,1])))continue;
  const r=rect(at);if(r.top>=panel.top&&r.bottom<=panel.bottom+.01)prompts.push(r);
 }
 assert.ok(prompts.length>20,'actual replay copy is present');
 // Size14 ink is accompanied by a half-cell shadow, so include its extent.
 const promptBottom=Math.max(...prompts.map(r=>r.bottom))+.875;
 assert.ok(promptBottom<Math.min(...Object.values(tops)),'painted status copy and shadows clear both vitals');
 console.log(JSON.stringify({width,height,panel,vitalsTop:tops,promptBottom,healthQuads:vitals.health.length,ammoQuads:vitals.ammo.length}));
}
