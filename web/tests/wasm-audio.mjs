// Offline real-Wasm gameplay/audio integration. No browser automation or device.
import assert from 'node:assert/strict';
import fs from 'node:fs';
const catalog=JSON.parse(fs.readFileSync(new URL('../../examples/catalog.json',import.meta.url)));
const results=[];
for(const game of catalog){
 const path=new URL(`../../dist/web/${game.host}/${game.id.replaceAll('-','_')}.wasm`,import.meta.url);
 const module=new WebAssembly.Module(fs.readFileSync(path));
 const env={ferravex_controls_len:()=>0,ferravex_controls_load:()=>-1,ferravex_controls_store:()=>-1,ferravex_set_playing(){}};
 const e=new WebAssembly.Instance(module,{env}).exports;(e.ferravex_init??e.init)();e.ferravex_autoplay(1);e.ferravex_audio_state(1);
 const key=code=>{e.ferravex_key(code,1);e.ferravex_key(code,0);};let peak=0,frames=0;
 for(;frames<600&&peak<.0001;frames++){
  if(frames===1){if(!game.id.startsWith('physics-'))key(32);key(0xff0d);}
  if(frames===3)e.ferravex_button(1,1);
  if(game.id==='owned-orbit')e.orbit_controls_frame(1/60,960,540);
  else if(game.id==='owned-lab')e.ferravex_controls_step(1/60,1,1,16/9,960,540);
  else e.ferravex_frame(1/60,960,540);
  assert.equal(e.ferravex_audio_render(800,48000),800);
  const pcm=new Float32Array(e.memory.buffer,e.ferravex_audio_pcm(),1600);
  for(const value of pcm){assert.ok(Number.isFinite(value)&&Math.abs(value)<=1,game.id+' finite bounded PCM');peak=Math.max(peak,Math.abs(value));}
 }
 assert.ok(peak>.0001,game.id+' must emit actual gameplay sound');
 e.ferravex_audio_state(0);e.ferravex_audio_mute(1);e.ferravex_audio_render(512,48000);
 assert.ok(new Float32Array(e.memory.buffer,e.ferravex_audio_pcm(),1024).every(v=>v===0),game.id+' mute clears transients and emits silence');
 assert.equal(e.ferravex_audio_render(4097,48000),0);assert.equal(e.ferravex_audio_render(512,1),0);
 const row={game:game.id,frames,peak};results.push(row);console.log(JSON.stringify(row));
}
console.log(`All ${results.length} Wasm games produced gameplay audio.`);
