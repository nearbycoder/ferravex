// Offline Wasm ABI integration. No browser or live DOM is automated here.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import {bindInput} from '../input.js';
class Target {addEventListener(){} }
const data=new Map();let engine;
const host={window:new Target(),document:new Target(),navigator:{},storage:()=>({getItem:key=>data.get(key)??null,setItem:(key,value)=>data.set(key,value)}),atob,btoa};
const catalog=JSON.parse(fs.readFileSync(new URL('../../examples/catalog.json',import.meta.url)));
function key(code){engine.ferravex_key(code,1);engine.ferravex_key(code,0);}
function configFirst(bytes){const length=bytes[36],count=bytes[37],at=38+length;return {name:new TextDecoder().decode(bytes.subarray(38,at)),bindings:Array.from({length:count},(_,n)=>({tag:bytes[at+n*11],code:new DataView(bytes.buffer,bytes.byteOffset).getUint16(at+n*11+1,true)}))};}
for(const game of catalog){
 const input=bindInput(new Target(),game.id,()=>engine,()=>false,host);
 const module=new WebAssembly.Module(fs.readFileSync(new URL(`../../dist/web/${game.host}/${game.id.replaceAll('-','_')}.wasm`,import.meta.url)));
 assert.ok(WebAssembly.Module.imports(module).every(i=>i.module==='env'&&(i.name in input.imports||i.name==='ferravex_set_playing')),'unexpected host import');
 const instantiate=()=>new WebAssembly.Instance(module,{env:{...input.imports,ferravex_set_playing(){}}}).exports;
 engine=instantiate();(engine.ferravex_init??engine.init)();
 const frame=()=>game.id==='owned-orbit'?engine.orbit_controls_frame(1/60,1280,720):game.id==='owned-lab'?engine.ferravex_controls_step(1/60,0,1,16/9,1280,720):engine.ferravex_frame(1/60,1280,720);
 frame();key(0xffbf);frame();assert.equal(engine.ferravex_controls_open(),1,game.id+' opens controls');
 assert.ok(engine.ferravex_vertex_count()>0,game.id+' renders controls UI');
 if(game.id==='emberline'){
  for(let i=0;i<3;i++){key(0xff09);frame();}key(0xff0d);frame();key(106);frame();
  let raw=data.get('ferravex.controls.v1.'+game.id);assert.ok(raw,'real Wasm saved a control profile');
  let bytes=Uint8Array.from(atob(raw.slice(6)),c=>c.charCodeAt(0));
  assert.equal(new TextDecoder().decode(bytes.subarray(0,4)),'FXIN');assert.ok(configFirst(bytes).bindings.some(b=>b.tag===0&&b.code===106));
  engine=instantiate();engine.ferravex_init();frame();key(0xffbf);frame();
  // Edit a sensitivity after reload, ensuring saved bindings loaded before the new save.
  for(let i=0;i<2;i++){key(0xff09);frame();}key(0xff0d);frame();key(0xff09);frame();key(0xff0d);frame();
  raw=data.get('ferravex.controls.v1.'+game.id);bytes=Uint8Array.from(atob(raw.slice(6)),c=>c.charCodeAt(0));
  assert.ok(configFirst(bytes).bindings.some(b=>b.tag===0&&b.code===106),'saved binding survives a fresh instance and later settings edit');
 }
 key(0xffbf);frame();assert.equal(engine.ferravex_controls_open(),0,game.id+' closes controls');
 console.log(game.id+' shared input ABI passed');
}
