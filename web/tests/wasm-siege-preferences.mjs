// Headless actual-Wasm test of visible barrel transforms under independent
// presentation flags. Supplied time/input is explicit; no game memory is set.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {createHash} from 'node:crypto';
const path=process.argv[2] ?? new URL('../../dist/web/siege-foundry/siege_foundry.wasm',import.meta.url);
const bytes=readFileSync(path);
console.log('Wasm SHA256',createHash('sha256').update(bytes).digest('hex'));
const e=new WebAssembly.Instance(new WebAssembly.Module(bytes),{env:{
  ferravex_save_len:()=>0,ferravex_save_load:()=>-1,ferravex_save_store:()=>-1,
  ferravex_controls_len:()=>0,ferravex_controls_load:()=>-1,ferravex_controls_store:()=>-1,
  ferravex_set_playing:()=>{},
}}).exports;
const frame=(dt=1/60)=>e.ferravex_frame(dt,320,448);
const key=code=>{e.ferravex_key(code,1);e.ferravex_key(code,0);frame();};
function barrel() {
  const view=new DataView(e.memory.buffer);
  for(let i=0;i<e.ferravex_retained_batch_count();i++) {
    const at=e.ferravex_retained_batches()+i*224,id=view.getUint32(at,true);
    const color=new Float32Array(e.memory.buffer,e.ferravex_mesh_vertices(id)+24,4);
    if(![65,81,78].every((c,j)=>Math.round(color[j]*255)===c))continue;
    const first=view.getUint32(at+4,true),count=view.getUint32(at+8,true);
    for(let j=first;j<first+count;j++) {
      const m=new Float32Array(e.memory.buffer,e.ferravex_retained_instances()+j*112,16);
      if(m[14]>13&&m[14]<16)return [...m.slice(12,15)];
    }
  }
  assert.fail('visible barrel instance missing');
}
e.ferravex_init();e.ferravex_autoplay(0);frame();
const rest=barrel();key(0xff0d);key(32);const fired=barrel();
assert.notDeepEqual(fired,rest,'accepted shot moves the rendered barrel');
key(0xff1b);const paused=barrel();
assert.equal(e.ferravex_feedback_preferences(0,1),1);frame(0);
assert.deepEqual(barrel(),paused,'reduced flash does not suppress recoil');
assert.equal(e.ferravex_feedback_preferences(1,0),1);frame(0);
assert.deepEqual(barrel(),rest,'reduced motion cancels recoil while paused');
assert.equal(e.ferravex_feedback_preferences(0,0),1);frame(0);
assert.deepEqual(barrel(),rest,'restoring motion does not revive old recoil');
e.ferravex_feedback_preferences(1,1);key(0xff1b);
for(let i=0;i<60;i++)frame();key(32);
assert.deepEqual(barrel(),rest,'new shot under reduced motion has no recoil');
e.ferravex_feedback_preferences(0,1);frame(0);
assert.deepEqual(barrel(),rest,'restoring motion cannot revive suppressed shot');
for(let i=0;i<60;i++)frame();key(32);
assert.notDeepEqual(barrel(),rest,'fresh accepted shot recoils with flash reduction on');
console.log(JSON.stringify({rest,fired,paused,independentFlash:true,pausedCancellation:true,noRevival:true,newAcceptedShot:true}));
