import assert from 'node:assert/strict';
import {playerLink,createWakeLock} from '../player.js';
const shared=new URL(playerLink('https://example.com/play/neon-swarm/?connect=secret&autoplay&look=ink#debug',2,1));
assert.equal(shared.search,'?look=cool&quality=medium');assert.equal(shared.hash,'');
let resolve,hidden=false,releases=0;const lock=createWakeLock({hidden:()=>hidden,request:()=>new Promise(r=>resolve=r)});
const pending=lock.set(true);await Promise.resolve();await Promise.resolve();const cancelled=lock.set(false);resolve({release:async()=>{releases++;}});await pending;await cancelled;assert.equal(releases,1,'late wake lock must be released after disable');assert.equal(lock.enabled,false);

const {SessionClock,duration}=await import('../player.js');const clock=new SessionClock();clock.sample(0,true);clock.sample(1000,true);clock.sample(2000,false);clock.sample(10000,true);assert.equal(clock.seconds,1);clock.sample(11000,true);assert.equal(clock.seconds,2);assert.equal(duration(125),'2:05');clock.reset();assert.equal(clock.seconds,0);
