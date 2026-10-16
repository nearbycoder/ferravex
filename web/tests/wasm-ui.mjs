// Actual release Wasm: game menus must activate on release, preserve drag-out
// cancellation, support keyboard focus and block while settings owns input.
import assert from 'node:assert/strict';
import fs from 'node:fs';
const catalog=JSON.parse(fs.readFileSync(new URL('../../examples/catalog.json',import.meta.url)));
function runtime(id){
 const game=catalog.find(g=>g.id===id);let playing=false;
 const module=new WebAssembly.Module(fs.readFileSync(new URL(`../../dist/web/${game.host}/${id.replaceAll('-','_')}.wasm`,import.meta.url)));
 const e=new WebAssembly.Instance(module,{env:{ferravex_save_len:()=>0,ferravex_save_load:()=>-1,ferravex_save_store:()=>-1,ferravex_controls_len:()=>0,ferravex_controls_load:()=>-1,ferravex_controls_store:()=>-1,ferravex_set_playing:v=>playing=!!v}}).exports;
 e.ferravex_init();
 const frame=()=>{e.ferravex_frame(1/60,1280,800);assert.ok(new Float32Array(e.memory.buffer,e.ferravex_vertices(),e.ferravex_vertex_count()*8).every(Number.isFinite));};
 const key=code=>{e.ferravex_key(code,1);e.ferravex_key(code,0);};
 const menu=()=>{
  const v=new Float32Array(e.memory.buffer,e.ferravex_vertices(),e.ferravex_vertex_count()*8);
  for(let i=0;i<v.length;i+=8)if(Math.abs(v[i+4]-.025)<1e-6&&Math.abs(v[i+5]-.055)<1e-6&&Math.abs(v[i+7]-.82)<1e-6)return true;
  return false;
 };
 const panel=(x,y,w,h,color)=>{
  const design=id==='orchard-guard'?[1200,760]:[1280,800],scale=Math.min(1280/design[0],800/design[1]);
  x=(1280-design[0]*scale)/2+x*scale;y=(800-design[1]*scale)/2+y*scale;w*=scale;h*=scale;
  const v=new Float32Array(e.memory.buffer,e.ferravex_vertices(),e.ferravex_vertex_count()*8),near=(a,b)=>Math.abs(a-b)<1e-5;
  for(let i=0;i+23<v.length;i+=8)if(near(v[i],x/1280*2-1)&&near(v[i+1],1-y/800*2)
   &&near(v[i+8],(x+w)/1280*2-1)&&near(v[i+9],1-y/800*2)&&near(v[i+17],1-(y+h)/800*2)
   &&color.every((c,n)=>near(v[i+4+n],c)))return true;
  return false;
 };
 return {e,frame,key,menu,panel,playing:()=>playing};
}
{
 const {e,frame,key,menu}=runtime('neon-swarm');frame();assert.ok(menu());
 e.ferravex_pointer(900,600,0,0);e.ferravex_button(1,1);e.ferravex_pointer(640,490,-260,-110);e.ferravex_button(1,0);frame();frame();assert.ok(menu(),'same-frame outside press cannot become an inside click');
 // Button occupies design x470..810/y468..520 at this exact fit viewport.
 e.ferravex_pointer(640,490,0,0);e.ferravex_button(1,1);frame();assert.ok(menu(),'press alone cannot activate');
 e.ferravex_pointer(900,600,260,110);e.ferravex_button(1,0);frame();frame();assert.ok(menu(),'drag-out release cancels');
 e.ferravex_touch(7,0,640,490);frame();assert.ok(menu());e.ferravex_touch(8,0,640,490);e.ferravex_touch(8,2,640,490);frame();assert.ok(menu(),'other finger cannot release capture');
 e.ferravex_touch(7,2,640,490);frame();frame();assert.ok(!menu(),'owning touch release starts game');
 key(0xff1b);frame();assert.ok(menu(),'pause uses shared menu');
 key(0xffbf);frame();assert.equal(e.ferravex_controls_open(),1);key(0xff0d);frame();assert.ok(menu(),'Done must not also activate underlying resume');
 assert.equal(e.ferravex_controls_open(),0,'Enter closes focused Done in the controls editor');
 key(0xff09);frame();key(0xff0d);frame();frame();assert.ok(!menu(),'keyboard focuses and resumes');
 console.log('neon-swarm shared UI release, capture, focus and settings integration passed');
}
{
 const {e,frame,key,playing}=runtime('strike');frame();assert.equal(playing(),false);
 key(0xff09);frame();key(0xff0d);frame();assert.equal(playing(),true,'keyboard starts Strike');
 key(0xff1b);frame();assert.equal(playing(),false);
 e.ferravex_pointer(150,420,0,0);e.ferravex_button(1,1);frame();assert.equal(playing(),false);
 e.ferravex_button(1,0);frame();assert.equal(playing(),true,'mouse releases Resume');
 console.log('strike shared menu keyboard and release integration passed');
}
{
 const {e,frame,key,panel}=runtime('orchard-guard');frame();
 assert.ok(panel(240,683,190,55,[.23,.39,.24,1]),'empty plot shows Seed build control');
 key(0xff09);frame();key(0xff0d);frame();frame();
 assert.ok(!panel(240,683,190,55,[.23,.39,.24,1]),'accepted build replaces empty plot controls');
 assert.ok(panel(260,683,190,55,[.23,.39,.24,1]),'built tower exposes upgrade control');
 key(0xff52);frame();key(0xff0d);frame();frame();
 assert.ok(panel(957,683,215,55,[177/255,180/255,156/255,1]),'focused wave control launches and disables itself');
 console.log('orchard-guard shared keyboard build, focus cycling and wave activation passed');
}
