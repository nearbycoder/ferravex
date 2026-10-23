// Actual Crystal Wasm geometry regression. Supplied fixed dt is a headless
// renderer/simulation check, not live browser play or performance evidence.
// Defaults to the staged module; pass an explicit path to verify a frozen build.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {createHash} from 'node:crypto';
const path = process.argv[2] ?? new URL('../../dist/web/crystal-cavern/crystal_cavern.wasm', import.meta.url);
const bytes = readFileSync(path);
console.log(`Wasm ${path} sha256 ${createHash('sha256').update(bytes).digest('hex')}`);
const module = new WebAssembly.Module(bytes);
const near = (a,b) => Math.abs(a-b) < 1e-5;
const warm = [202/255,165/255,107/255,1], white = [1,1,1,1];
function runtime(width,height,autoplay=false,config=null) {
  let e;
  e = new WebAssembly.Instance(module, {env:{
    ferravex_save_len:()=>0,ferravex_save_load:()=>-1,ferravex_save_store:()=>-1,
    ferravex_controls_len:()=>config?.length??0,
    ferravex_controls_load:(ptr,len)=>{assert.equal(len,config.length);new Uint8Array(e.memory.buffer,ptr,len).set(config);return 0;},
    ferravex_controls_store:()=>-1,
    ferravex_set_playing:()=>{},
  }}).exports;
  e.ferravex_init();e.ferravex_autoplay(+autoplay);
  const frame = () => e.ferravex_frame(1/60,width,height);
  const key = code => {e.ferravex_key(code,1);e.ferravex_key(code,0);frame();frame();};
  const top = Math.min(Math.max(height*.28,110),Math.max(height-240,0));
  function glyphs(color,y0,y1,menu=true) {
    const v = new Float32Array(e.memory.buffer,e.ferravex_vertices(),e.ferravex_vertex_count()*8);
    assert.ok(v.every(Number.isFinite));
    // Both text regions are queued after their distinct panel. Starting there
    // excludes the same-colored aiming reticle that precedes the prompt panel.
    const panelColor=menu?[.03,.08,.08,.94]:[.03,.08,.08,.85];
    let panel=-1;
    for(let i=0;i<v.length;i+=8) if(panelColor.every((c,j)=>near(c,v[i+4+j]))) {panel=i;break;}
    assert.ok(panel>=0,menu?'expedition menu panel exists':'nearby prompt panel exists');
    const points=[];
    for(let i=panel+48;i<v.length;i+=8) {
      if(!color.every((c,j)=>near(c,v[i+4+j])))continue;
      const x=(v[i]+1)*width/2,y=(1-v[i+1])*height/2;
      if(y>=y0-.01&&y<=y1+.01)points.push([x,y]);
    }
    assert.ok(points.length>0,`nonempty glyph geometry in ${width}x${height} band ${y0}..${y1}`);
    assert.ok(points.every(([x])=>x>=15.99&&x<=width-15.99),'wrapped copy stays inside horizontal margins');
    return points.length;
  }
  function copy() {return {detail:glyphs(warm,top+62,top+122),guidance:glyphs(white,top+120,top+180)};}
  frame();
  return {e,frame,key,copy,glyphs};
}
const sizes=[[1280,576],[390,607],[320,409],[320,360]];
const counts={};
for(const [w,h] of sizes) {
  const r=runtime(w,h);
  const ready=r.copy();
  r.key(0xff0d);r.key(0xff1b);
  const paused=r.copy();
  // Use the existing authored pilot through real rules; never set outcomes.
  const pilot=runtime(w,h,true);
  for(let i=0;i<7200;i++)pilot.frame();
  const won=pilot.copy();
  const current={ready,paused,won};
  if(w===1280)Object.assign(counts,current);
  else assert.deepEqual(current,counts,`${w}x${h}: every menu glyph survives wrapping (including reset guidance)`);
  pilot.key(0xff0d); // Existing retry/confirm flow returns to autoplay.
  for(let i=0;i<7200;i++)pilot.frame();
  assert.deepEqual(pilot.copy(),won,'retry completes the same puzzle and renders the same result');
  console.log(`${w}x${h} Ready/Paused/Won full glyph counts and retry passed`,current);
}

// Profile recorded from ordinary Controls remapping in the live tab:
// interact=Backspace, restart=PageDown, pause=PageUp. No game state is restored.
{
  const config=Buffer.from('RlhJTgEADwAzM/M/bxKDOwAAIEAAAABA7FE4PgAAgD8AAAAACQNtb3ZlX2xlZnQAYQAAAIA/AAAAAAIOAAAAgD8AAAAABAwAAACAPwAAAAAKA21vdmVfcmlnaHQAZAAAAIA/AAAAAAIPAAAAgD8AAAAABA0AAACAPwAAAAAGAm1vdmVfeAMAAAAAgD8AAAAABAAAAACAPwAAAAAMA21vdmVfZm9yd2FyZAB3AAAAgD8AAAAAAgwAAACAPwAAAAAEDgAAAIA/AAAAAAkDbW92ZV9iYWNrAHMAAACAPwAAAAACDQAAAIA/AAAAAAQPAAAAgD8AAAAABgJtb3ZlX3kDAQAAAIA/AAAAAAQBAAAAgD8AAAAABgJsb29rX3gFAAAAAIA/AAAAAAYAAAAAgD8AAAAABgJsb29rX3kFAQAAAIA/AAAAAAYBAAAAgD8AAAAACwNsb29rX3JhdGVfeAMCAAAAgD8AAAAAAFH/7FE4vwAAAAAAU//sUTg/AAAAAAsDbG9va19yYXRlX3kDAwAAAIA/AAAAAABS/wAAAL8AAAAAAFT/AAAAPwAAAAAIA2ludGVyYWN0AgIAAACAPwAAAAAEBAAAAIA/AAAAAAAI/wAAgD8AAAAABwNyZXN0YXJ0AggAAACAPwAAAAAECQAAAIA/AAAAAABW/wAAgD8AAAAABwNjb25maXJtAA3/AACAPwAAAAACAAAAAIA/AAAAAAQKAAAAgD8AAAAABQNwYXVzZQIJAAAAgD8AAAAABAsAAACAPwAAAAAAVf8AAIA/AAAAAAQBaGVscABoAAAAgD8AAAAAJ03POw==','base64');
  let baseline;
  for(const [w,h] of [[1280,576],[320,360]]) {
    const r=runtime(w,h,false,config),ready=r.copy();
    r.key(0xff0d);r.key(0xff55);
    const paused=r.copy();
    const pilot=runtime(w,h,true,config);
    for(let i=0;i<7200;i++)pilot.frame();
    const won=pilot.copy(),actual={ready,paused,won};
    if(!baseline)baseline=actual;
    else assert.deepEqual(actual,baseline,'long saved remap labels preserve all menu glyphs');
    console.log(`${w}x${h} saved Backspace/PageUp/PageDown hints passed`,actual);
  }
}

// Walk the authored corridor into the closed seal, then back/left to altar I.
// Rejection keeps progress locked and paints the identical blocker twice.
let blockerBaseline;
for(const [w,h] of sizes) {
  const r=runtime(w,h);r.key(0xff0d);
  const hold=(code,ticks)=>{r.e.ferravex_key(code,1);for(let i=0;i<ticks;i++)r.frame();r.e.ferravex_key(code,0);r.frame();};
  hold(119,600);hold(115,69);hold(97,104);r.key(101);
  const promptTop=Math.max(114,Math.min(h*.55,h-230));
  const notice=r.glyphs(warm,h-150,h-78,false);
  const prompt=r.glyphs(warm,promptTop,promptTop+72,false);
  if(blockerBaseline===undefined)blockerBaseline=notice;
  assert.equal(notice,blockerBaseline,'every rejected-use notice glyph survives narrow wrapping');
  assert.equal(prompt,blockerBaseline,'nearby locked altar paints the complete matching blocker');
  console.log(`${w}x${h} walked locked-altar prompt and rejected-use notice passed`,{prompt,notice});
}
