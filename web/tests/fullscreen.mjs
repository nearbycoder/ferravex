// Platform contracts, including browsers without native fullscreen/orientation.
import assert from 'node:assert/strict';
import {fullscreenSession,gameOrientation,bindPlayerExpansion} from '../fullscreen.js';
assert.equal(gameOrientation('marble-run'),'landscape');assert.equal(gameOrientation('owned-orbit'),'portrait');assert.equal(gameOrientation('salvage-pinball'),'portrait');
function platform(){const calls=[];let native=false;const p={mobile:true,request:async()=>{calls.push('request');native=true;},exit:async()=>{native=false;},isNative:()=>native,expand:async active=>calls.push(['expand',active]),lock:async()=>calls.push('lock'),unlock:()=>calls.push('unlock'),reset:()=>calls.push('reset'),change:mode=>calls.push(mode),hint:text=>calls.push(['hint',text])};return {p,calls,escape(){native=false;}};}
const flush=()=>new Promise(resolve=>setImmediate(resolve));
{
 const {p,calls,escape}=platform(),s=fullscreenSession(p);await s.toggle();await flush();assert.equal(s.mode,'native');assert.ok(calls.includes('lock'));escape();s.sync();assert.equal(s.mode,'inline');assert.ok(calls.includes('unlock'));
}
{
 const {p,calls}=platform();p.request=async()=>{throw Error('unsupported');};p.lock=async()=>{throw Error('unsupported');};const s=fullscreenSession(p);await s.toggle();await flush();assert.equal(s.mode,'expanded');assert.ok(calls.some(c=>c[0]==='hint'));await s.toggle();assert.equal(s.mode,'inline');assert.deepEqual(calls.filter(c=>Array.isArray(c)&&c[0]==='expand'),[['expand',true],['expand',false]]);
}
{
 const {p,calls}=platform();let resolve;p.lock=()=>new Promise(r=>resolve=r);const s=fullscreenSession(p);await s.toggle();await flush();await s.toggle();const before=calls.filter(c=>c==='unlock').length;resolve();await flush();assert.equal(s.mode,'inline');assert.ok(calls.filter(c=>c==='unlock').length>before,'a late orientation lock is released after exit');
}
{
 const {p,calls}=platform(),pending=[];p.lock=()=>new Promise(resolve=>pending.push(resolve));const s=fullscreenSession(p);
 await s.toggle();await flush();await s.toggle();await s.toggle();await flush();
 const before=calls.filter(c=>c==='unlock').length;pending[0]();await flush();assert.equal(s.mode,'native');assert.equal(calls.filter(c=>c==='unlock').length,before,'an old lock cannot unlock a newer fullscreen session');
 pending[1]();await flush();await s.toggle();
}
{
 const {p,calls}=platform();p.mobile=false;const s=fullscreenSession(p);await s.toggle();await flush();assert.ok(!calls.includes('lock'),'desktop fullscreen never forces orientation');await s.toggle();
}
{
 const {p}=platform();p.request=async()=>{throw Error();};p.expand=async()=>{throw Error();};const s=fullscreenSession(p);await s.toggle();assert.equal(s.mode,'inline','failed fallback does not claim fullscreen');
}
{
 const classes=new Set(),messages=[],events=new Map();let focus=0,restored;
 const player={classList:{add:c=>classes.add(c),remove:c=>classes.delete(c)}},source={postMessage:(...args)=>messages.push(args)};
 const frame={src:'https://local.test/play/marble-run/',contentWindow:source,closest:()=>player,focus:()=>focus++};
 const doc={documentElement:{style:{overflow:'clip'}},querySelectorAll:()=>[frame]},win={location:{origin:'https://local.test',href:'https://local.test/examples/marble-run/'},scrollX:12,scrollY:80,scrollTo:value=>restored=value,addEventListener:(type,callback)=>events.set(type,callback)};
 bindPlayerExpansion(doc,win);const send=(origin=win.location.origin,sender=source,active=true)=>events.get('message')({origin,source:sender,data:{type:'ferravex:expand',active,id:1}});
 send('https://other.test');send(win.location.origin,{});assert.equal(classes.size,0,'untrusted origins/windows cannot expand a player');
 send();assert.ok(classes.has('player-expanded'));assert.equal(doc.documentElement.style.overflow,'hidden');assert.equal(focus,1);assert.equal(messages[0][0].ok,true);
 send(win.location.origin,source,false);assert.equal(classes.size,0);assert.equal(doc.documentElement.style.overflow,'clip');assert.deepEqual(restored,{left:12,top:80,behavior:'instant'});
 frame.src='https://other.test/game';send();assert.equal(classes.size,0,'only first-party embedded games may control the page fallback');
}
console.log('Fullscreen: native/fallback entry and exit, orientation rejection and late completion, Escape synchronization, desktop behavior and trusted embed expansion passed');
