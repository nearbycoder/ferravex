// Actual release Wasm + first-party browser backend, sharing only a storage Map.
import assert from 'node:assert/strict';import fs from 'node:fs';import {bindInput} from '../input.js';
import {resolve} from 'node:path';import {pathToFileURL} from 'node:url';
const root=process.argv[2]?pathToFileURL(resolve(process.argv[2])+'/'):new URL('../../',import.meta.url);
class Target {addEventListener(){}}
const catalog=JSON.parse(fs.readFileSync(new URL('examples/catalog.json',root)));
function runtime(game,data){
 let e,reads=0,writes=0;
 const input=bindInput(new Target(),game.id,()=>e,()=>false,{window:new Target(),document:new Target(),navigator:{},storage:()=>({getItem:k=>{reads++;return data.get(k)??null;},setItem:(k,v)=>{writes++;data.set(k,v);}}),atob,btoa});
 const module=new WebAssembly.Module(fs.readFileSync(new URL(`dist/web/${game.host}/${game.id.replaceAll('-','_')}.wasm`,root)));
 e=new WebAssembly.Instance(module,{env:{...input.imports,ferravex_set_playing(){}}}).exports;(e.ferravex_init??e.init)();
 const frame=()=>game.id==='owned-orbit'?e.orbit_controls_frame(1/60,1280,720):game.id==='owned-lab'?e.ferravex_controls_step(1/60,0,1,16/9,1280,720):e.ferravex_frame(1/60,1280,720);
 const key=code=>{e.ferravex_key(code,1);e.ferravex_key(code,0);frame();};
 return {e,frame,key,counts:()=>({reads,writes})};
}
function editMaster(r){
 r.key(0xffbf);r.key(0xff09);r.key(0xff09);r.key(0xff0d); // Devices tab.
 for(let i=0;i<4;i++)r.key(0xff52);r.key(0xff0d); // Next page.
 for(let i=0;i<4;i++)r.key(0xff09);r.key(0xff0d); // Master +.
}
for(const game of catalog){
 const data=new Map(),key='ferravex.save.v1.'+game.id+'.preferences';let r=runtime(game,data);r.frame();
 editMaster(r);assert.ok(Math.abs(r.e.ferravex_audio_master_gain()-.75)<1e-6,game.id+' applies volume');
 assert.ok(data.get(key)?.startsWith('fxsv1:'),game.id+' persists typed preferences');
 // Devices row follows the readable editor header at panel.y + 143.
 const rowCenter=(720-Math.min(620,Math.max(300,720-24)))/2+143+15.5;
 r.e.ferravex_pointer(965,rowCenter,0,0);r.e.ferravex_button(1,1);r.frame();r.e.ferravex_button(1,0);r.frame();
 assert.ok(Math.abs(r.e.ferravex_audio_master_gain()-.8)<1e-6,game.id+' mouse plus changes volume once');
 r.e.ferravex_pointer(890,rowCenter,0,0);r.e.ferravex_button(1,1);r.frame();r.e.ferravex_button(1,0);r.frame();
 assert.ok(Math.abs(r.e.ferravex_audio_master_gain()-.75)<1e-6,game.id+' mouse minus changes volume once');

 if(game.id==='emberline'){
  for(let i=0;i<12;i++)r.key(0xff09);for(let i=0;i<3;i++)r.key(0xff0d);
  assert.equal(new DataView(r.e.memory.buffer).getUint32(r.e.ferravex_light_environment()+380,true),2,'graphics setting reaches actual renderer environment');
 }

 const initialCounts=r.counts();for(let i=0;i<120;i++)r.frame();assert.deepEqual(r.counts(),initialCounts,game.id+' ordinary frames do no storage I/O');
 r=runtime(game,data);r.frame();assert.ok(Math.abs(r.e.ferravex_audio_master_gain()-.75)<1e-6,game.id+' reloads volume');
 if(game.id==='emberline'){
  const quality=()=>new DataView(r.e.memory.buffer).getUint32(r.e.ferravex_light_environment()+380,true);
  assert.equal(quality(),2,'graphics quality survives reload');const saved=data.get(key);r.e.ferravex_quality_override(0);r.frame();assert.equal(quality(),0);assert.equal(data.get(key),saved,'explicit session quality override does not change saved preference');
 }

 r.e.ferravex_audio_mute(1);r.frame();r=runtime(game,data);r.frame();assert.equal(r.e.ferravex_audio_muted(),1,game.id+' reloads sound preference');
 if(game.id==='emberline'){
  const bad=Uint8Array.from(atob(data.get(key).slice(6)),c=>c.charCodeAt(0));bad[bad.length-1]^=1;const damaged='fxsv1:'+btoa(String.fromCharCode(...bad));data.set(key,damaged);
  r=runtime(game,data);r.frame();assert.ok(Math.abs(r.e.ferravex_audio_master_gain()-.7)<1e-6);editMaster(r);assert.ok(Math.abs(r.e.ferravex_audio_master_gain()-.75)<1e-6);assert.equal(data.get(key),damaged,'rejected file is not auto-overwritten');
  for(let i=0;i<5;i++)r.key(0xff52);r.key(0xff0d);assert.notEqual(data.get(key),damaged,'explicit Devices Reset replaces rejected settings');
  r=runtime(game,data);r.frame();assert.ok(Math.abs(r.e.ferravex_audio_master_gain()-.7)<1e-6);assert.equal(r.e.ferravex_audio_muted(),0);
 }
 console.log(game.id+' real-Wasm volume/mute persistence and idle I/O checks passed');
}
{
 const game=catalog.find(g=>g.id==='orchard-guard'),data=new Map();let r=runtime(game,data);r.frame();r.key(0xff09);r.key(0xff0d);r.frame();
 const key='ferravex.save.v1.orchard-guard.checkpoint';assert.ok(data.has(key),'actual build writes a safe checkpoint');
 const stored=data.get(key);r=runtime(game,data);r.frame();assert.equal(data.get(key),stored,'loading progress does not rewrite it');
 // A restored tower exposes the 415-wide upgrade panel, not either 190-wide
 // empty-plot build control. The upgrade now shares their left edge.
 const v=new Float32Array(r.e.memory.buffer,r.e.ferravex_vertices(),r.e.ferravex_vertex_count()*8);
 const scale=Math.min(1280/1200,720/760),x=(1280-1200*scale)/2+240*scale,y=683*scale,w=415*scale,h=55*scale;
 const near=(a,b)=>Math.abs(a-b)<1e-5;
 let upgrade=false;for(let i=0;i+23<v.length;i+=8)if(near(v[i],x/1280*2-1)&&near(v[i+1],1-y/720*2)
  &&near(v[i+8],(x+w)/1280*2-1)&&near(v[i+9],1-y/720*2)&&near(v[i+17],1-(y+h)/720*2)
  &&[.23,.39,.24,1].every((c,n)=>near(v[i+4+n],c))){upgrade=true;break;}
 assert.ok(upgrade,'fresh Wasm instance restores the actual tower and upgrade HUD');
 r.key(114);r.frame();assert.notEqual(data.get(key),stored,'restart clears the checkpoint explicitly');
 console.log('Orchard actual build, native-code checkpoint serialization, fresh-instance resume and restart passed');
}
{
 const game=catalog.find(g=>g.id==='neon-swarm'),data=new Map(),key='ferravex.save.v1.neon-swarm.records';
 for(let run=1;run<=2;run++){
  const r=runtime(game,data);r.frame();r.key(0xff0d);const previous=data.get(key);
  for(let frame=0;frame<7200&&data.get(key)===previous;frame++)r.frame();
  assert.ok(data.has(key)&&data.get(key)!==previous,'normal gameplay must save an actual completed attempt');
  const ptr=r.e.ferravex_diagnostics_export(),len=r.e.ferravex_diagnostics_len();
  const csv=new TextDecoder().decode(new Uint8Array(r.e.memory.buffer,ptr,len));
  const deaths=csv.trim().split('\n').filter(line=>line.split(',')[3]==='death');
  assert.equal(deaths.length,1,'actual defeat emits one diagnostic');
  assert.ok(Number(deaths[0].split(',')[1])>0,'defeat carries the real fixed-clock tick');
  const raw=Uint8Array.from(atob(data.get(key).slice(6)),c=>c.charCodeAt(0)),view=new DataView(raw.buffer);let at=36,runs=null;
  for(let field=0;field<view.getUint16(32,true);field++){
   const n=raw[at++],name=new TextDecoder().decode(raw.subarray(at,at+n));at+=n;const tag=raw[at++],len=view.getUint32(at,true);at+=4;
   if(name==='runs.0'){assert.equal(tag,1);runs=view.getBigUint64(at,true);}at+=len;
  }
  assert.equal(runs,BigInt(run),'fresh instances preserve the previous completed-attempt count');
 }
 console.log('Neon normal gameplay records two completed attempts across real-Wasm instances');
}
