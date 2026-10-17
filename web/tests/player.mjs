import assert from 'node:assert/strict';
import {playerLink,createWakeLock} from '../player.js';
const shared=new URL(playerLink('https://example.com/play/neon-swarm/?connect=secret&autoplay&look=ink#debug',2,1));
assert.equal(shared.search,'?look=cool&quality=medium');assert.equal(shared.hash,'');
let resolve,hidden=false,releases=0;const lock=createWakeLock({hidden:()=>hidden,request:()=>new Promise(r=>resolve=r)});
const pending=lock.set(true);await Promise.resolve();await Promise.resolve();const cancelled=lock.set(false);resolve({release:async()=>{releases++;}});await pending;await cancelled;assert.equal(releases,1,'late wake lock must be released after disable');assert.equal(lock.enabled,false);

const {SessionClock,duration}=await import('../player.js');const clock=new SessionClock();clock.sample(0,true);clock.sample(1000,true);clock.sample(2000,false);clock.sample(10000,true);assert.equal(clock.seconds,1);clock.sample(11000,true);assert.equal(clock.seconds,2);assert.equal(duration(125),'2:05');clock.reset();assert.equal(clock.seconds,0);

const {FrameGate}=await import('../player.js');const gate=new FrameGate();gate.set(30);let rendered=0;for(let i=0;i<120;i++)rendered+=gate.accept(i*1000/120);assert.equal(rendered,30);gate.set(60);assert.equal(gate.accept(0),true);gate.set(-1);assert.equal(gate.limit,0);

const {readPlayerPreferences,validatePlayerPreferences}=await import('../player.js');assert.deepEqual(validatePlayerPreferences({hud:'1',cap:'30',unknown:'ignored'}),{hud:'1',cap:'30'});assert.throws(()=>validatePlayerPreferences({cap:'999'}));assert.deepEqual(readPlayerPreferences({getItem:()=>'{bad'},'key'),{});assert.deepEqual(readPlayerPreferences({getItem(){throw Error('blocked');}},'key'),{});
