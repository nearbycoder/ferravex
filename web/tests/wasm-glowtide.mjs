// Actual release Wasm driven only by production input/settings exports.
// Existing diagnostic ticks prove lifecycle progression; no game-state writes,
// test-only exports, browser automation, wall-clock waits or debug hooks.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import {resolve} from 'node:path';
import {pathToFileURL} from 'node:url';
import {createHash} from 'node:crypto';
const root=process.argv[2]?pathToFileURL(resolve(process.argv[2])+'/'):new URL('../../',import.meta.url);
const wasmPath=process.argv[3]?resolve(process.argv[3]):new URL('dist/web/glowtide/glowtide.wasm',root);
const bytes=fs.readFileSync(wasmPath),module=new WebAssembly.Module(bytes);
const sha=createHash('sha256').update(bytes).digest('hex');
function runtime(w,h){
 const e=new WebAssembly.Instance(module,{env:{
  ferravex_save_len:()=>0,ferravex_save_load:()=>-1,ferravex_save_store:()=>-1,
  ferravex_controls_len:()=>0,ferravex_controls_load:()=>-1,ferravex_controls_store:()=>-1,
 }}).exports;
 const frame=(n=1)=>{for(let i=0;i<n;i++){
  assert.equal(e.ferravex_frame(1/60,w,h),1);
  // Inject a valid slow-frame diagnostic sample so the existing trace records
  // the current simulation tick every frame. Adaptation remains disabled.
  assert.equal(e.ferravex_performance_sample(45,0.2,-1),1);
 }};
 const tick=()=>{
  const ptr=e.ferravex_diagnostics_export(),len=e.ferravex_diagnostics_len();
  const rows=new TextDecoder().decode(new Uint8Array(e.memory.buffer,ptr,len)).trim().split('\n');
  return Number(rows.at(-1).split(',')[1]);
 };
 const geometry=()=>{
  const count=e.ferravex_vertex_count();assert.ok(count>0&&count<1_000_000,'bounded visible geometry');
  const v=new Float32Array(e.memory.buffer,e.ferravex_vertices(),count*8);
  assert.ok(v.every(Number.isFinite),'all clip coordinates and colors are finite');return v;
 };
 // Locate connected, opaque cyan fill triangles in the actual painted HUD.
 // The primary action is a large component; glyphs, glows and icons are smaller.
 const primary=()=>{
  const v=geometry(),triangles=[],owners=new Map(),parents=[];
  const find=i=>parents[i]===i?i:(parents[i]=find(parents[i]));
  const cyan=at=>v[at+4]>=.44&&v[at+4]<=.65&&v[at+5]>=.94&&v[at+6]>=.99&&v[at+7]>=.98;
  for(let at=0;at+24<=v.length;at+=24){
   if(![at,at+8,at+16].every(cyan))continue;
   const points=[at,at+8,at+16].map(i=>[(v[i]+1)*w/2,(1-v[i+1])*h/2]);
   const area=Math.abs((points[1][0]-points[0][0])*(points[2][1]-points[0][1])-(points[2][0]-points[0][0])*(points[1][1]-points[0][1]))/2;
   if(area<16)continue;
   const index=triangles.length;parents.push(index);triangles.push(points);
   for(const [x,y]of points){const key=`${x.toFixed(3)},${y.toFixed(3)}`;
    if(owners.has(key))parents[find(index)]=find(owners.get(key));else owners.set(key,index);
   }
  }
  const groups=new Map();
  for(let i=0;i<triangles.length;i++){const id=find(i);if(!groups.has(id))groups.set(id,[]);groups.get(id).push(...triangles[i]);}
  const candidates=[...groups.values()].map(points=>({
   left:Math.min(...points.map(p=>p[0])),right:Math.max(...points.map(p=>p[0])),
   top:Math.min(...points.map(p=>p[1])),bottom:Math.max(...points.map(p=>p[1])),
  })).filter(r=>r.right-r.left>=80&&r.bottom-r.top>=25);
  assert.equal(candidates.length,1,`one painted primary action: ${JSON.stringify(candidates)}`);
  const r=candidates[0];assert.ok(r.left>=0&&r.right<=w+.01&&r.top>=0&&r.bottom<=h+.01,'action is fully visible');
  assert.ok(r.top>=60||r.right<=w-72,'action clears fullscreen corner');return[(r.left+r.right)/2,(r.top+r.bottom)/2];
 };
 const click=(x,y)=>{e.ferravex_pointer(x,y,0,0);e.ferravex_button(1,1);frame();e.ferravex_button(1,0);frame(2);};
 const key=code=>{e.ferravex_key(code,1);e.ferravex_key(code,0);frame(2);};
 e.ferravex_init();frame(75);return{e,frame,tick,geometry,primary,click,key};
}
for(const [w,h]of [[1280,800],[1600,1000],[320,480],[390,844],[844,390]]){
 const r=runtime(w,h),{e}=r;
 assert.equal(r.tick(),0,'title attract mode does not advance the player run');
 r.click(...r.primary());r.frame(70);
 assert.equal(r.tick(),0,'character selection does not start gameplay');
 r.click(...r.primary());r.frame(70);
 const running=r.tick();assert.ok(running>20,'Dive starts authoritative fixed ticks');
 r.key(0xff1b);r.frame(10);const paused=r.tick();r.frame(30);
 assert.equal(r.tick(),paused,'pause holds simulation ticks');
 r.geometry();r.click(...r.primary());r.frame(12);
 assert.ok(r.tick()>paused,'painted Resume action resumes ticks');
 r.key(0xff1b);r.frame(5);const manuallyPaused=r.tick();
 r.key(0xffbf);assert.equal(e.ferravex_controls_open(),1,'F2 opens Controls');
 r.key(0xff1b);assert.equal(e.ferravex_controls_open(),0,'Escape closes Controls');
 r.frame(10);assert.equal(r.tick(),manuallyPaused,'closing Controls preserves manual pause');
 r.key(0xff1b);r.frame(5);assert.ok(r.tick()>manuallyPaused,'Escape resumes after settings');
 const beforeRestart=r.tick();assert.ok(beforeRestart>50);
 r.key(114);r.frame(4);assert.ok(r.tick()<beforeRestart/2,'R starts a fresh simulation');
 // Pointer and a zero-tick key tap use ordinary exported input, including dash.
 e.ferravex_key(100,1);r.frame(12);e.ferravex_key(100,0);r.frame();
 const beforeTap=r.tick();e.ferravex_key(32,1);e.ferravex_key(32,0);
 e.ferravex_frame(0,w,h);r.frame(2);assert.ok(r.tick()>beforeTap);
 for(const [motion,flash]of [[0,0],[1,0],[0,1],[1,1]]){
  assert.equal(e.ferravex_feedback_preferences(motion,flash),1);r.frame(2);r.geometry();
  const post=new Float32Array(e.memory.buffer,e.ferravex_post_frame(),28);
  assert.ok(post.every(Number.isFinite));
  if(flash)assert.ok(post[4]<=.6+.00001,'reduced flash caps bloom');
 }
 assert.equal(e.ferravex_feedback_preferences(2,0),0,'invalid accessibility flags are rejected');
 assert.equal(e.ferravex_ui_preferences_v1(1,2,1),1,'large text/high contrast accepted');r.frame(2);r.geometry();
 assert.equal(e.ferravex_details_set(0,1),1);r.frame(2);
 assert.equal(new Float32Array(e.memory.buffer,e.ferravex_post_frame(),28)[0],0,'Low quality disables post');
 assert.equal(e.ferravex_details_set(0,3),1);r.frame(2);r.geometry();
 console.log(`Glowtide ${w}x${h}: title → select → play, pause/Resume, Controls, restart, settings and finite geometry passed`);
}
console.log(`Glowtide release Wasm SHA256 ${sha}`);
