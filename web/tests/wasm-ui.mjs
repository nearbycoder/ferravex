// Actual release Wasm: game menus must activate on release, preserve drag-out
// cancellation, support keyboard focus and block while settings owns input.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import {resolve} from 'node:path';
import {pathToFileURL} from 'node:url';
// An optional immutable source/artifact root permits parent integration checks
// without changing the captured source or publishing an unverified build.
const root=process.argv[2]?pathToFileURL(resolve(process.argv[2])+'/'):new URL('../../',import.meta.url);
const catalog=JSON.parse(fs.readFileSync(new URL('examples/catalog.json',root)));
function runtime(id){
 const game=catalog.find(g=>g.id===id);let playing=false;
 const module=new WebAssembly.Module(fs.readFileSync(new URL(`dist/web/${game.host}/${id.replaceAll('-','_')}.wasm`,root)));
 const e=new WebAssembly.Instance(module,{env:{ferravex_save_len:()=>0,ferravex_save_load:()=>-1,ferravex_save_store:()=>-1,ferravex_controls_len:()=>0,ferravex_controls_load:()=>-1,ferravex_controls_store:()=>-1,ferravex_set_playing:v=>playing=!!v}}).exports;
 e.ferravex_init();
 const frame=(width=1280,height=800)=>{e.ferravex_frame(1/60,width,height);assert.ok(new Float32Array(e.memory.buffer,e.ferravex_vertices(),e.ferravex_vertex_count()*8).every(Number.isFinite));};
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
for(const [width,height] of [[1280,800],[390,608],[320,480]]){
 const {e,frame,key,playing}=runtime('strike'),step=()=>frame(width,height);
 step();assert.equal(playing(),false);
 key(0xff09);step();key(0xff0d);step();assert.equal(playing(),true,'keyboard starts Strike');
 key(0xff1b);step();assert.equal(playing(),false);
 // Locate the actual primary button fill rather than an obsolete fixed click.
 // Six colored vertices must form one visible, sufficiently large rectangle.
 const vertices=new Float32Array(e.memory.buffer,e.ferravex_vertices(),e.ferravex_vertex_count()*8);
 const color=[215/255,181/255,109/255,1],near=(a,b)=>Math.abs(a-b)<1e-5,buttons=[];
 for(let i=0;i+48<=vertices.length;i+=8){
  if(!Array.from({length:6},(_,n)=>i+n*8).every(at=>color.every((c,j)=>near(vertices[at+4+j],c))))continue;
  const points=Array.from({length:6},(_,n)=>[(vertices[i+n*8]+1)*width/2,(1-vertices[i+n*8+1])*height/2]);
  const left=Math.min(...points.map(p=>p[0])),right=Math.max(...points.map(p=>p[0]));
  const top=Math.min(...points.map(p=>p[1])),bottom=Math.max(...points.map(p=>p[1]));
  if(right-left<44-.01||bottom-top<44-.01)continue;
  assert.ok(points.every(([x,y])=>(near(x,left)||near(x,right))&&(near(y,top)||near(y,bottom))),'primary fill is rectangular');
  buttons.push({left,right,top,bottom});i+=40;
 }
 assert.equal(buttons.length,1,'exactly one primary Resume button is painted');
 const {left,right,top,bottom}=buttons[0];
 assert.ok(left>=0&&right<=width&&top>=0&&bottom<=height,'button stays in viewport');
 const x=(left+right)/2,y=(top+bottom)/2;
 e.ferravex_pointer(x,y,0,0);e.ferravex_button(1,1);step();assert.equal(playing(),false,'press alone cannot resume');
 e.ferravex_pointer(0,0,-x,-y);e.ferravex_button(1,0);step();assert.equal(playing(),false,'outside release cancels Resume');
 e.ferravex_pointer(x,y,x,y);e.ferravex_button(1,1);step();assert.equal(playing(),false,'fresh press still waits for release');
 e.ferravex_button(1,0);step();assert.equal(playing(),true,'mouse releases rendered Resume');
 console.log(`strike shared menu keyboard, rendered bounds, drag cancellation and release passed at ${width}x${height}`);
}
{
 const {e,frame,key,panel}=runtime('orchard-guard');frame();
 assert.ok(panel(240,683,190,55,[.23,.39,.24,1]),'empty plot shows Seed build control');
 key(0xff09);frame();key(0xff0d);frame();frame();
 assert.ok(!panel(240,683,190,55,[.23,.39,.24,1]),'accepted build replaces empty plot controls');
 assert.ok(panel(240,683,415,55,[.23,.39,.24,1]),'built tower exposes the wider upgrade control with room for current costs and hints');
 // Building removes the focused Seed button. Focus the replacement Upgrade,
 // then reverse through the current HUD (which also includes Pause/Restart).
 key(0xff09);frame();
 assert.ok(panel(240,683,415,2,[.88,.62,.22,1]),'keyboard focuses the new upgrade control');
 key(0xff52);frame();key(0xff0d);frame();frame();
 assert.ok(panel(957,683,215,55,[97/255,105/255,86/255,1]),'focused wave control launches and disables itself');
 assert.ok(panel(695,683,110,55,[.23,.39,.24,1]),'starting the first wave enables Pause');
 // Starting play clears focus. Navigate Upgrade -> Pause, then activate the
 // modal's Resume and verify the running-wave HUD returns without a restart.
 key(0xff09);frame();key(0xff09);frame();key(0xff0d);frame();frame();
 assert.ok(panel(375,404,215,55,[.23,.39,.24,1]),'keyboard Pause opens the resume modal');
 key(0xff0d);frame();frame();
 assert.ok(!panel(375,404,215,55,[.23,.39,.24,1]),'modal Resume returns to the orchard');
 assert.ok(panel(240,683,415,55,[.23,.39,.24,1]),'resume retains the built tower');
 assert.ok(panel(957,683,215,55,[97/255,105/255,86/255,1]),'resume retains the active wave');
 console.log('orchard-guard shared keyboard build, focus cycling, wave, pause and resume passed');
}

// Read actual glyph geometry after the HUD panel, excluding world geometry.
// Consecutive cream vertices form one text run, separated by its shadow pass.
for(const width of [1280,390,320]){
 const height=800,{e,frame}=runtime('pocket-rally');frame(width,height);
 const v=new Float32Array(e.memory.buffer,e.ferravex_vertices(),e.ferravex_vertex_count()*8);
 const near=(a,b)=>Math.abs(a-b)<1e-5;
 const color=(i,c)=>c.every((n,j)=>near(v[i+4+j],n));
 const point=i=>[(v[i]+1)*width/2,(1-v[i+1])*height/2];
 const findPanel=alpha=>{
  for(let i=0;i<v.length;i+=8)if(color(i,[.12,.19,.17,alpha]))return i;
  assert.fail(`rally panel ${alpha} exists`);
 };
 const runs=(start,end)=>{
  const out=[];let run;
  for(let i=start;i<end;i+=8){
   if(!color(i,[.96,.92,.81,1])){run=null;continue;}
   if(!run){run={left:Infinity,right:-Infinity,top:Infinity,bottom:-Infinity};out.push(run);}
   const [x,y]=point(i);run.left=Math.min(run.left,x);run.right=Math.max(run.right,x);run.top=Math.min(run.top,y);run.bottom=Math.max(run.bottom,y);
  }
  return out;
 };
 const hud=findPanel(.78),countdown=findPanel(.94);
 const header=runs(hud+48,countdown).filter(r=>r.top<80);
 assert.equal(header.length,5,'lap, speed, lap time, gates and race time are rendered');
 for(const r of header)assert.ok(r.left>=8&&r.right<=width-8,`rally HUD stays inside ${width}px viewport: ${JSON.stringify(r)}`);
 const [lap,speed,lapTime,gates,raceTime]=header;
 assert.ok(speed.left-lap.right>=8,'lap and speed stay separate');
 assert.ok(speed.right<=width-72,'the fullscreen menu corner stays clear');
 assert.ok(raceTime.left-gates.right>=8,'race time remains separate from gate progress');
 const [left,top]=point(countdown),[right,bottom]=point(countdown+40);
 const labels=runs(countdown+48,v.length);
 assert.equal(labels.length,3,'ready, countdown number and starting instruction are rendered');
 for(const r of labels)assert.ok(r.left>=left+10&&r.right<=right-10&&r.top>=top+10&&r.bottom<=bottom-10,`countdown text stays inside its panel at ${width}px: ${JSON.stringify(r)}`);
 console.log(`pocket-rally actual glyph bounds, speed spacing and countdown panel passed at ${width}x${height}`);
}

// Actual Dune glyph cells: compact lap and timer must leave each other and the
// host's 44px menu corner clear, rather than merely keeping their anchor points.
for(const [width,height] of [[320,480],[390,844],[844,390]]){
 const {e,frame}=runtime('dune-skimmer');frame(width,height);
 const v=new Float32Array(e.memory.buffer,e.ferravex_vertices(),e.ferravex_vertex_count()*8);
 const bounds=(color,size,maxY=32)=>{
  const points=[];
  for(let at=0;at+48<=v.length;at+=48){
   if(!color.every((c,n)=>Math.abs(v[at+4+n]-c)<1e-5))continue;
   const q=Array.from({length:6},(_,i)=>[(v[at+i*8]+1)*width/2,(1-v[at+i*8+1])*height/2]);
   const ys=q.map(p=>p[1]),xs=q.map(p=>p[0]);
   if(Math.max(...ys)>maxY||Math.min(...ys)<0||Math.abs(Math.max(...ys)-Math.min(...ys)-size/8)>.01)continue;
   points.push(...q);
  }
  assert.ok(points.length>20,'real header bitmap glyphs exist');
  return {left:Math.min(...points.map(p=>p[0])),right:Math.max(...points.map(p=>p[0]))};
 };
 if(width<600){
  const lap=bounds([1,1,1,1],14),timer=bounds([1,1,1,1],20);
  assert.ok(lap.right+8<=timer.left,`lap and timer overlap at ${width}: ${JSON.stringify({lap,timer})}`);
  assert.ok(timer.right<=width-72,'timer clears the fullscreen menu');
 }else{
  const gate=bounds([56/255,159/255,151/255,1],14,52);
  assert.ok(gate.right<=width-72,'gate counter clears the fullscreen menu');
 }
 console.log(`dune-skimmer compact lap/timer spacing passed at ${width}x${height}`);
}
