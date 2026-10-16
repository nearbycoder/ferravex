// GPU query lifecycle only; this fake driver does not verify WebGL rendering.
import assert from 'node:assert/strict';
import {GpuTimer} from '../../dist/web/retained.js';
const canvas={dataset:{}};
let disjoint=false,created=0,deleted=0,begun=0,resultReads=0;
const gl={
 QUERY_RESULT_AVAILABLE:1,QUERY_RESULT:2,
 getExtension:()=>({GPU_DISJOINT_EXT:3,TIME_ELAPSED_EXT:4}),
 createQuery:()=>{created++;return {ready:false,ns:30_000_000};},
 deleteQuery:()=>deleted++,getParameter:()=>disjoint,
 getQueryParameter:(q,field)=>{if(field===1)return q.ready;resultReads++;assert.ok(q.ready,'never read a pending result');return q.ns;},
 beginQuery:()=>begun++,endQuery(){}
};
const timer=new GpuTimer(gl,canvas,true);
for(let frame=1;frame<=20;frame++){timer.begin(frame);timer.end();}
assert.equal(created,16);assert.equal(begun,16,'saturated queries drop samples without growing storage');
assert.equal(resultReads,0);assert.equal(timer.latest,null);
timer.pending[0].query.ready=true;timer.begin(21);timer.end();
assert.deepEqual(timer.latest,{frame:1,us:30000},'timings retain the actual source frame');
assert.equal(created,16,'completed query is reused');
disjoint=true;timer.begin(22);timer.end();
assert.equal(timer.pending.length,0);assert.equal(timer.latest,null);assert.equal(deleted,16);
assert.equal(canvas.dataset.gpuUs,undefined,'disjoint results are cleared, including displayed timing');
disjoint=false;timer.begin(23);timer.end();
timer.pending[0].query.ready=true;timer.begin(24);timer.end();
assert.deepEqual(timer.latest,{frame:23,us:30000},'only post-disjoint queries become valid');
const unavailable=new GpuTimer({...gl,getExtension:()=>null},{dataset:{}},true);
unavailable.begin(1);unavailable.end();assert.equal(unavailable.latest,null);
console.log('GPU timing pool, source-frame identity, disjoint invalidation and unavailable timers passed');
