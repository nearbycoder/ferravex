import assert from 'node:assert/strict';
import {loadWasm} from '../loader.js';
let fetched=0,streamed=0,buffered=0;const imports={env:{}};
const response={ok:true,clone(){return this;},arrayBuffer:async()=>new Uint8Array([1,2]).buffer};
const host={fetch:async()=>{fetched++;return response;},wasm:{instantiateStreaming:async(_r,i)=>{assert.equal(i,imports);streamed++;throw Error('MIME');},instantiate:async(b,i)=>{buffered++;assert.equal(b.byteLength,2);assert.equal(i,imports);return 'fallback';}}};
assert.equal(await loadWasm('game.wasm',imports,host),'fallback');assert.equal(fetched,1);assert.equal(streamed,1);assert.equal(buffered,1);
host.wasm.instantiateStreaming=async()=> 'streamed';assert.equal(await loadWasm('game.wasm',imports,host),'streamed');assert.equal(buffered,1);
await assert.rejects(loadWasm('missing',imports,{...host,fetch:async()=>({ok:false,status:404})}),/404/);

let aborted=false;await assert.rejects(loadWasm('hung',imports,{...host,timeoutMs:5,fetch:(_url,{signal})=>new Promise(()=>{signal.addEventListener('abort',()=>aborted=true);})}),/timed out/);assert.equal(aborted,true);
// A stalled body is bounded as well as the initial HTTP request.
await assert.rejects(loadWasm('body',imports,{...host,timeoutMs:5,fetch:async()=>({...response,arrayBuffer:()=>new Promise(()=>{})}),wasm:{instantiate(){}}}),/timed out/);
