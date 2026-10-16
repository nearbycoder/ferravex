import assert from 'node:assert/strict';
import {bindGraphicsLifecycle} from '../lifecycle.js';
class Canvas extends EventTarget{}
const canvas=new Canvas(),pending=new Map();let next=1,frames=0,suspended=0,resumed=0,restored=0;
const life=bindGraphicsLifecycle(canvas,{restore(){restored++;},suspend(){suspended++;},resume(){resumed++;},error(e){throw e;},request(f){const id=next++;pending.set(id,f);return id;},cancel(id){pending.delete(id);}});
const frame=()=>{frames++;life.schedule(frame);};life.schedule(frame);life.schedule(frame);assert.equal(pending.size,1);
const tick=()=>{const [id,f]=pending.entries().next().value;pending.delete(id);f(1);};tick();assert.equal(frames,1);assert.equal(pending.size,1);
canvas.dispatchEvent(new Event('webglcontextlost',{cancelable:true}));assert.equal(suspended,1);assert.equal(pending.size,0);assert.equal(life.active(),false);
canvas.dispatchEvent(new Event('webglcontextrestored'));assert.equal(restored,1);assert.equal(resumed,1);assert.equal(pending.size,1);tick();assert.equal(frames,2);
// Repeated restores cannot create competing render loops or reset game counters.
canvas.dispatchEvent(new Event('webglcontextrestored'));assert.equal(pending.size,1);tick();assert.equal(frames,3);
life.stop();assert.equal(pending.size,0);canvas.dispatchEvent(new Event('webglcontextrestored'));assert.equal(pending.size,0);
let failed=false;const broken=new Canvas();const bad=bindGraphicsLifecycle(broken,{restore(){throw Error('unsupported GPU');},suspend(){},resume(){assert.fail('must not resume after failed rebuild');},error(){failed=true;},request(){return 1;},cancel(){}});broken.dispatchEvent(new Event('webglcontextlost'));broken.dispatchEvent(new Event('webglcontextrestored'));assert.equal(bad.active(),false);assert.equal(failed,true);
console.log('Graphics lifecycle: loss cancels pending frames, restoration preserves state and one render loop, failed rebuild stops safely');
