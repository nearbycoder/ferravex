// Actual release Wasm, supplied fixed deltas. All outcomes follow ordinary
// input or the built-in legal-shot demonstration; no game state is written.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import {createHash} from 'node:crypto';
import {bindInput} from '../input.js';
class Target {constructor(){this.events=new Map();}addEventListener(t,f){this.events.set(t,f);}emit(t,e={}){this.events.get(t)?.({preventDefault(){},...e});}focus(){}setPointerCapture(){}getBoundingClientRect(){return {left:0,top:0};}}

const bytes=id=>fs.readFileSync(new URL(`../../dist/web/${id}/${id.replaceAll('-','_')}.wasm`,import.meta.url));
function runtime(id,w=320,h=640,auto=false){
 const b=bytes(id),module=new WebAssembly.Module(b);
 let playing=false;
 const e=new WebAssembly.Instance(module,{env:{ferravex_save_len:()=>0,ferravex_save_load:()=>-1,ferravex_save_store:()=>-1,ferravex_controls_len:()=>0,ferravex_controls_load:()=>-1,ferravex_controls_store:()=>-1,ferravex_set_playing:v=>playing=!!v}}).exports;
 const win=new Target();bindInput(new Target(),id,()=>e,()=>playing,{window:win,document:new Target(),navigator:{},storage:()=>({getItem:()=>null,setItem(){}}),atob,btoa});
 const browserKey=code=>{win.emit('keydown',{code});win.emit('keyup',{code});frame();frame();};
 const hold=(code,frames)=>{win.emit('keydown',{code});for(let i=0;i<frames;i++)frame();win.emit('keyup',{code});frame();};
 e.ferravex_init();e.ferravex_autoplay(+auto);
 const frame=(dt=1/60)=>{e.ferravex_frame(dt,w,h);};
 const status=()=>JSON.parse(new TextDecoder().decode(new Uint8Array(e.memory.buffer,e.golf_status_v1(),e.golf_status_v1_len())));
 const key=k=>{e.ferravex_key(k,1);e.ferravex_key(k,0);frame();frame();};
 const click=(x,y)=>{e.ferravex_pointer(x,y,0,0);e.ferravex_button(1,1);frame();e.ferravex_button(1,0);frame();frame();};
 const glyphs=()=>{const v=new Float32Array(e.memory.buffer,e.ferravex_vertices(),e.ferravex_vertex_count()*8);assert.ok(v.every(Number.isFinite));let found=false,count=0;for(let at=0;at<v.length;at+=8){const x=(v[at]+1)*w/2,y=(1-v[at+1])*h/2;const dark=Math.abs(v[at+4]-27/255)<1e-5&&Math.abs(v[at+5]-51/255)<1e-5;if(dark&&Math.abs(y-(h-212)/2)<0.01&&Math.abs(x-(w-Math.min(w-32,420))/2)<0.01)found=true;const paper=Math.abs(v[at+4]-246/255)<1e-5&&Math.abs(v[at+5]-239/255)<1e-5;if(found&&paper)count++;}assert.ok(found);return count;};
 frame();return{e,frame,key,browserKey,hold,click,status,glyphs,playing:()=>playing,hash:createHash('sha256').update(b).digest('hex')};
}
for(const id of ['mini-golf-2d','mini-golf-3d']){
 const menu=[];
 for(const [w,h]of [[1280,720],[390,640],[320,640],[320,480],[844,390],[400,300]]){
  const r=runtime(id,w,h);menu.push(r.glyphs());assert.equal(r.status().strokes,0);r.key(0xff0d);assert.equal(r.status().strokes,0,'start does not putt');r.key(32);assert.equal(r.status().strokes,1);for(let i=0;i<240;i++)r.frame();assert.equal(r.status().phase,'Holed','default first putt earns the cup');
  r.key(0xff0d);assert.equal(r.status().hole,2);assert.equal(r.status().scores[0],1);
  r.browserKey('KeyP');const before=r.status();for(let i=0;i<40;i++)r.frame();assert.deepEqual(r.status(),before,'manual pause holds rules');r.key(0xffbf);assert.equal(r.e.ferravex_controls_open(),1);r.key(0xff1b);assert.equal(r.e.ferravex_controls_open(),0);for(let i=0;i<10;i++)r.frame();assert.deepEqual(r.status(),before,'Controls preserves manual pause');
  r.browserKey('KeyP');r.browserKey('KeyC');r.key(0xff0d);assert.equal(r.status().hole,2,'card return does not reset the hole');
  // All twelve practice buttons remain real UI, including the last row.
  r.browserKey('KeyP');const by=(h-264)/2;r.click(w/2-60,by+152+22); // Pause > Practice
  const tw=Math.min(w-32,420)-32,tx=(w-Math.min(w-32,420))/2+16,cw=(tw-18)/4;
  r.click(tx+3*(cw+6)+cw/2,(h-268)/2+54+2*50+22);
  assert.equal(r.status().hole,12);assert.equal(r.status().practice,true);
  console.log(`${id} ${w}x${h}: putt, cup, progression, pause, Controls, card and practice passed`);
 }
 assert.ok(menu.every(n=>n===menu[0]),`${id}: all welcome-menu glyphs survive narrow wrapping`);
 {
  const r=runtime(id);r.browserKey('Enter');const power=r.status().power;r.hold('KeyW',24);assert.ok(r.status().power>power+0.1,'real browser W binding changes power');r.hold('KeyA',10);
  // Real touch journal. The owner alone supplies aim and release; cancellation
  // cannot spend a stroke, and another finger cannot finish that gesture.
  const touch=(id,phase,x,y)=>{r.e.ferravex_touch(id,phase,x,y);r.frame();};
  touch(7,0,80,280);touch(7,1,200,280);touch(7,3,200,280);assert.equal(r.status().strokes,0,'cancelled drag does not putt');
  touch(8,0,80,280);touch(9,0,160,310);touch(9,2,160,310);assert.equal(r.status().strokes,0,'another finger cannot release owner');touch(8,1,210,280);touch(8,2,210,280);assert.equal(r.status().strokes,1,'owner release accepts one putt');
 }
 const r=runtime(id,390,640,true);for(let i=0;i<2200&&r.status().phase!=='Finished';i++)r.frame(0.1);
 const s=r.status();assert.equal(s.phase,'Finished');assert.equal(s.hole,12);assert.ok(s.scores.every(v=>v>0&&v<12),JSON.stringify(s));assert.ok(s.total<80);console.log(`${id} legal twelve-hole demonstration: ${s.total} strokes; SHA256 ${r.hash}`);
}
