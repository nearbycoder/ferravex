import assert from 'node:assert/strict';
import {bindStorage,MAX_SAVE_BYTES} from '../storage.js';
const data=new Map(),engine={memory:new WebAssembly.Memory({initial:2})};let failed=false;
const host={storage:()=>({getItem:key=>data.get(key)??null,setItem:(key,value)=>{if(failed)throw new Error('quota');data.set(key,value);}}),atob,btoa};
const bridge=bindStorage(()=>engine,'game',host).imports,bytes=()=>new Uint8Array(engine.memory.buffer),put=(at,s)=>bytes().set(new TextEncoder().encode(s),at);
put(0,'game');put(80,'records');put(160,'other');bytes().set([1,2,3,4],200);
assert.equal(bridge.ferravex_save_len(0,4,80,7),0);
assert.equal(bridge.ferravex_save_store(0,4,80,7,200,4),0);
assert.equal(bridge.ferravex_save_len(0,4,80,7),4);assert.equal(bridge.ferravex_save_load(0,4,80,7,220,4),0);assert.deepEqual(Array.from(bytes().slice(220,224)),[1,2,3,4]);
assert.equal(bridge.ferravex_save_load(0,4,80,7,220,3),-1);assert.equal(bridge.ferravex_save_load(0,4,80,7,131071,4),-1);
assert.equal(bridge.ferravex_save_store(160,5,80,7,200,4),-1,'cannot address a different game');
assert.equal(bridge.ferravex_save_store(0,4,80,7,200,MAX_SAVE_BYTES+1),-1);
const saved=data.get('ferravex.save.v1.game.records');failed=true;bytes()[200]=9;assert.equal(bridge.ferravex_save_store(0,4,80,7,200,4),-1);assert.equal(data.get('ferravex.save.v1.game.records'),saved);failed=false;
engine.memory.grow(1);assert.equal(bridge.ferravex_save_len(0,4,80,7),4);assert.equal(bridge.ferravex_save_load(0,4,80,7,220,4),0);assert.equal(bytes()[220],1,'cache survives memory growth');
for(const raw of ['','garbage','fxsv1:','fxsv1:%%%']){data.set('ferravex.save.v1.game.records',raw);assert.equal(bridge.ferravex_save_len(0,4,80,7),-1);}
data.set('ferravex.save.v1.game.records','x'.repeat(Math.ceil(MAX_SAVE_BYTES/3)*4+7));assert.equal(bridge.ferravex_save_len(0,4,80,7),-2);
const locked=bindStorage(()=>engine,'game',{...host,storage:()=>{throw new Error('blocked');}}).imports;assert.equal(locked.ferravex_save_len(0,4,80,7),-1);assert.equal(locked.ferravex_save_store(0,4,80,7,200,4),-1);
console.log('browser save transport scopes, bounds, quota, corruption and memory growth passed');
